#!/usr/bin/env python3
"""Reproduce the phase-50 synthetic source/layout workload through public tools."""
from pathlib import Path
import argparse, subprocess, json, time, uuid
parser = argparse.ArgumentParser(description="Create six Motion video benchmarks in an authorized disposable session")
parser.add_argument("--session", required=True)
parser.add_argument("--directory", required=True, type=Path)
parser.add_argument("--cli", default="target/release/fold-cli")
parser.add_argument("--bake", action="store_true", help="Also bake the ten-source case")
parser.add_argument("--compact-sdr", action="store_true", help="Explicitly allow lossy BC7 and clamp out-of-range RGB")
parser.add_argument("--resolution", type=int, default=1024)
parser.add_argument("--frames", type=int, default=31)
args = parser.parse_args()
cli, session, root = args.cli, args.session, args.directory.resolve()
if (root / 'benchmark.fold').exists():
    parser.error('Choose a new benchmark directory; benchmark.fold already exists')
(root / 'media').mkdir(parents=True, exist_ok=True)
for i in range(10):
    path = root / 'media' / f'video-{i:02d}.mp4'
    if not path.exists():
        subprocess.run(['ffmpeg','-v','error','-nostdin','-n','-f','lavfi','-i','testsrc2=size=1920x1080:rate=30','-vf',f'hue=h={i*29},drawbox=x={i*140}:y=30:w=100:h=100:color=white:t=fill','-t','5','-c:v','libx264','-preset','ultrafast','-crf','23','-threads','2','-g','30','-bf','0','-pix_fmt','yuv420p','-color_range','tv','-colorspace','bt709','-color_trc','bt709','-color_primaries','bt709',str(path)], check=True)
def call(m,p):
 r=subprocess.run([cli,'agent','call',session,m,'-'],input=json.dumps(p),text=True,capture_output=True)
 if not r.stdout: raise RuntimeError(r.stderr)
 v=json.loads(r.stdout);assert v['ok'],v;return v['result']
def wait(j):
 end=time.monotonic()+3600
 while time.monotonic()<end:
  r=call('jobs',{'job':j})
  if r['status']=='complete':assert r['outcome']['ok'],r;return r['outcome']['result']
  time.sleep(.05)
 raise TimeoutError(j)
def batch(ops,ids=(),label='Video texture benchmark'):
 s=call('state',{});return wait(call('batch',{'request_id':str(uuid.uuid4()),'revision':s['revision'],'project_epoch':s['project_epoch'],'label':label,'ids':list(ids),'operations':ops})['job'])
ref=lambda n:{'$ref':n}
t=lambda n,d=1:{'numerator':n,'denominator':d}
add=lambda name,kind:{'op':'add','id':ref(name),'kind':'fold.motion.'+kind}
connect=lambda target,input,source,socket='value':{'op':'connect','node':ref(target),'input':input,'source':{'node':ref(source),'socket':socket}}
setval=lambda target,input,value:{'op':'set','node':ref(target),'input':input,'value':{'Vector' if isinstance(value,list) else 'Scalar':value}}
paths=sorted((root/'media').glob('*.mp4'))
known={Path(v['location']).resolve() for v in call('state',{})['assets']}
missing=[str(p) for p in paths if p.resolve() not in known]
if missing:batch([{'op':'import','paths':missing}])
assets={Path(v['location']).name:v['id'] for v in call('state',{})['assets']}
info={'width':1920,'height':1080,'rate':[30,1],'frames':150}
entries=[{'source':{'Asset':assets[p.name]},'info':info,'offset':t(0)} for p in paths]
cases={}
for name,count,mode in [('one',1,'distinct'),('two',2,'distinct'),('five',5,'distinct'),('ten',10,'distinct'),('shared',10,'shared'),('offset',10,'offset')]:
 selected=entries[:count] if mode=='distinct' else ([entries[0]] if mode=='shared' else [dict(entries[0],offset=t(i,10)) for i in range(10)])
 edits=[{'op':'info','value':info},add('image','image'),{'op':'node','node':ref('image'),'value':{'settings':{'sources':selected,'outside':'Loop'}}},{'op':'object','id':ref('object'),'kind':'fold.motion.image_cards','name':name},{'op':'object_inputs','object':ref('object'),'role':'transform','inputs':{'translation':{'Value':{'Vector':[0,0]}}}},connect('object','image','image','image'),setval('object','size',[360,202.5]),add('points','grid_points'),setval('points','columns',min(5,count)),setval('points','rows',2 if count==10 else 1),setval('points','spacing',[380,240]),setval('points','origin',[200,420]),connect('object','points','points','points'),add('index','index'),connect('object','index','index')]
 r=batch([{'op':'create','id':ref('document'),'kind':'fold.motion.document','name':'Video benchmark '+name},{'op':'edit','document':ref('document'),'edits':edits}],['document','image','object','points','index'])
 cases[name]=r['ids'];print(name,r['ids']['document'],flush=True)
s=call('state',{});wait(call('save',{'request_id':str(uuid.uuid4()),'revision':s['revision'],'project_epoch':s['project_epoch'],'path':str(root/'benchmark.fold')})['job'])
(root/'cases.json').write_text(json.dumps(cases,indent=2))
print('saved',call('state',{})['dirty'],flush=True)

if args.bake:
 c=cases['ten']
 r=batch([{'op':'edit','document':c['document'],'edits':[add('bake','image_bake'),{'op':'node','node':ref('bake'),'value':{'settings':{'resolution':args.resolution,'end':args.frames,'format':'Bc7' if args.compact_sdr else 'Auto','clamp_sdr':args.compact_sdr}}},{'op':'connect','node':ref('bake'),'input':'image','source':{'node':c['image'],'socket':'image'}},{'op':'connect','node':c['object'],'input':'image','source':{'node':ref('bake'),'socket':'image'}}]}],['bake'])
 c['bake']=r['ids']['bake']
 start=time.monotonic()
 batch([{'op':'command','id':'fold.app.image-bake','arguments':{'document':c['document'],'node':c['bake']}}])
 (root/'bake-time.json').write_text(json.dumps({'seconds':time.monotonic()-start}))
 s=call('state',{});wait(call('save',{'request_id':str(uuid.uuid4()),'revision':s['revision'],'project_epoch':s['project_epoch'],'path':str(root/'benchmark-baked.fold')})['job'])
 (root/'cases.json').write_text(json.dumps(cases,indent=2))
 print('Baked ten-source benchmark saved',flush=True)
