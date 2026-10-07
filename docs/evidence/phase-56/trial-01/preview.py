import importlib.util,json,time,base64
from pathlib import Path
R=Path('/home/rgame/dev/Fold_Dev/Fold');E=R/'docs/evidence/phase-56/trial-01'
spec=importlib.util.spec_from_file_location('transport',R/'skills/fold/scripts/live_badge.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
old=m.Client.tool
count=len((E/'public-transcript.jsonl').read_text().splitlines())
def logged(self,method,args):
 global count
 count+=1;t=time.monotonic();err=None
 try:r=old(self,method,args)
 except Exception as ex:r=None;err=str(ex)
 def clean(v):
  if isinstance(v,dict):
   if v.get('type')=='image' and 'data' in v:
    p=E/f'reopen-image-{count}.png';p.write_bytes(base64.b64decode(v['data']));return {'path':str(p)}
   return {k:clean(x) for k,x in v.items()}
  if isinstance(v,list):return [clean(x) for x in v]
  return v
 with (E/'public-transcript.jsonl').open('a') as f:f.write(json.dumps({'call':count,'phase':'polished-preview','over_invocation_budget':True,'method':method,'arguments':args,'elapsed':time.monotonic()-t,'result':clean(r),'error':err})+'\n')
 if err:raise RuntimeError(err)
 return r
m.Client.tool=logged
c=m.Client(R/'target/release/fold-cli',R/'demos/cell-type-rust.fold')
try:
 s=c.call('state');doc=next(d['id'] for d in s['documents'] if d['type']=='fold.composition.document')
 frames=[]
 for start in [0,12]:
  j=c.call('observe',source={'document':doc,'output':'video'},times=[{'numerator':n,'denominator':4} for n in range(start,start+12)],dimensions=[250,444])
  time.sleep(8)
  r=c.call('jobs',job=j['job'])
  assert r['status']=='complete' and r['outcome']['ok']
  from PIL import Image
  data=r['_images'][0]['data'];im=Image.open(__import__('io').BytesIO(base64.b64decode(data)))
  for f in r['outcome']['result']['metadata']['frames']:
   x,y,w,h=f['rect'];frames.append(im.crop((x,y,x+w,y+h)))
 frames[0].save(R/'demos/cell-type-rust.gif',save_all=True,append_images=frames[1:],duration=250,loop=0)
finally:c.close()
print('Public invocations:',count)
