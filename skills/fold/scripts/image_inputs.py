#!/usr/bin/env python3
"""Build the media-input acceptance study through a live Fold session.
Usage: image_inputs.py FOLD_CLI SESSION NEW_PROJECT.fold
Creates small linked PPM/MP4 fixtures beside the project. Requires FFmpeg.
"""
import argparse, base64, json, math, os, subprocess, time, uuid
from pathlib import Path
p=argparse.ArgumentParser(description=__doc__)
p.add_argument('cli');p.add_argument('session');p.add_argument('project')
a=p.parse_args();destination=Path(a.project).resolve()
if destination.exists(): raise FileExistsError(destination)
def call(method,params):
 r=subprocess.run([a.cli,'agent','call',a.session,method,'-'],input=json.dumps(params),text=True,capture_output=True)
 if not r.stdout: raise RuntimeError(r.stderr)
 value=json.loads(r.stdout)
 if not value['ok']:raise RuntimeError(value)
 return value['result']
def wait(job):
 end=time.monotonic()+180
 while time.monotonic()<end:
  r=call('jobs',{'job':job})
  if r['status']=='complete':
   if not r['outcome']['ok']:raise RuntimeError(r['outcome'])
   return r['outcome']['result']
  time.sleep(.05)
 raise TimeoutError(job)
def batch(ops,ids=(),label='Image input study'):
 s=call('state',{});r=wait(call('batch',{'request_id':str(uuid.uuid4()),'revision':s['revision'],'project_epoch':s['project_epoch'],'label':label,'ids':list(ids),'operations':ops})['job'])
 print(json.dumps({'label':label,'ids':r.get('ids')}),flush=True);return r
ref=lambda n:{'$ref':n}
t=lambda n,d=1:{'numerator':n,'denominator':d}
add=lambda name,kind:{'op':'add','id':ref(name),'kind':'fold.motion.'+kind}
connect=lambda target,input,source,socket='value':{'op':'connect','node':ref(target),'input':input,'source':{'node':ref(source),'socket':socket}}
setval=lambda target,input,value:{'op':'set','node':ref(target),'input':input,'value':{'Vector' if isinstance(value,list) else 'Scalar':value}}
info={'width':640,'height':360,'rate':[30,1],'frames':90}
still=destination.with_name(destination.stem+'-source.ppm');video=destination.with_name(destination.stem+'-source.mp4')
if not still.exists():
 pixels=bytearray()
 for y in range(180):
  for x in range(320):
   radius=math.hypot((x-160)/160,(y-90)/90);lines=x%32<2 or y%30<2
   pixels.extend((255 if lines else int(255*x/319),255 if lines else int(255*y/179),255 if lines else int(127+127*math.cos(radius*12))))
 still.write_bytes(b'P6\n320 180\n255\n'+pixels)
if not video.exists():
 subprocess.run(['ffmpeg','-v','error','-nostdin','-n','-f','lavfi','-i','testsrc2=size=320x180:rate=30','-t','3','-c:v','libx264','-threads','1','-pix_fmt','yuv420p','-color_range','tv','-colorspace','bt709','-color_trc','bt709','-color_primaries','bt709',str(video)],check=True)
batch([{'op':'import','paths':[str(still),str(video)]}],label='Import image and video fixtures')
assets={Path(v['location']).name:v['id'] for v in call('state',{})['assets']}
entries=[{'source':{'Asset':assets[path.name]},'info':{'width':320,'height':180,'rate':[30,1],'frames':90},'offset':t(0)} for path in [still,video]]
created={}
for name,kind in [('Warped texture','image_grid'),('Media carousel','image_cards'),('Video ASCII','image_ascii')]:
 edits=[{'op':'info','value':info},add('image','image'),{'op':'node','node':ref('image'),'value':{'settings':{'sources':entries if kind=='image_cards' else [entries[1] if kind=='image_ascii' else entries[0]],'outside':'Loop'}}},{'op':'object','id':ref('object'),'kind':'fold.motion.'+kind,'name':name},connect('object','image','image','image'),setval('object','size',[560,280] if kind!='image_cards' else [175,105]),{'op':'object_inputs','object':ref('object'),'role':'transform','inputs':{'translation':{'Value':{'Vector':[320,180]}}}}]
 ids=['document','image','object']
 if kind=='image_ascii': edits += [setval('object','rows',30)]
 if kind=='image_grid':
  edits += [add('index','index'),add('time','time'),add('wave','wave'),setval('wave','frequency',.035),setval('wave','amplitude',20),connect('wave','coordinate','index'),connect('wave','phase','time'),add('offset','combine_xy'),connect('offset','y','wave'),connect('object','offset','offset')]
  ids += ['index','time','wave','offset']
 if kind=='image_cards':
  edits += [add('points','grid_points'),setval('points','columns',3),setval('points','rows',2),setval('points','spacing',[200,150]),setval('points','origin',[-200,-75]),add('index','index'),connect('object','points','points','points'),connect('object','index','index'),add('time','time'),add('wave','wave'),connect('wave','coordinate','time'),setval('wave','amplitude',15),add('offset','combine_xy'),connect('offset','y','wave'),connect('points','origin','offset')]
  # Center the moving layout using the point grid's own base origin via wave offsets.
  edits += [setval('offset','x',-200),setval('wave','offset',-75)]
  ids += ['points','index','time','wave','offset']
 r=batch([{'op':'create','id':ref('document'),'kind':'fold.motion.document','name':name},{'op':'edit','document':ref('document'),'edits':edits}],ids,label=name)
 created[name]=r['ids']
 for frame in [0,15,45]:
  result=wait(call('render',{'source':{'document':r['ids']['document'],'output':'video'},'time':t(frame,30),'dimensions':[640,360]})['job'])
  destination.with_name(destination.stem+'-'+kind+'-'+str(frame)+'.png').write_bytes(base64.b64decode(result['image']['data']))
# Prove a document output can feed Motion without flattening or creating a cycle.
r=batch([{'op':'create','id':ref('document'),'kind':'fold.motion.document','name':'Nested image output'},{'op':'place','source':created['Warped texture']['document'],'target':ref('document'),'at':t(0)}],['document'],label='Place document image output')
result=wait(call('render',{'source':{'document':r['ids']['document'],'output':'video'},'time':t(1),'dimensions':[640,360]})['job'])
destination.with_name(destination.stem+'-nested.png').write_bytes(base64.b64decode(result['image']['data']))
s=call('state',{});wait(call('save',{'request_id':str(uuid.uuid4()),'revision':s['revision'],'project_epoch':s['project_epoch'],'path':str(destination)})['job'])
print(json.dumps({'documents':created,'state':call('state',{})},indent=2))
