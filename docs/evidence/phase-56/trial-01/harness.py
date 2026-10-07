import sys,os,json,time,base64,importlib.util
from pathlib import Path
ROOT=Path('/home/rgame/dev/Fold_Dev/Fold'); E=ROOT/'docs/evidence/phase-56/trial-01'
spec=importlib.util.spec_from_file_location('transport',ROOT/'skills/fold/scripts/live_badge.py');m=importlib.util.module_from_spec(spec);spec.loader.exec_module(m)
class Logged(m.Client):
 def tool(self,method,args):
  t=time.monotonic()
  try:r=super().tool(method,args);err=None
  except Exception as ex:r=None;err=str(ex)
  def clean(v):
   if isinstance(v,dict):
    if v.get('type')=='image' and 'data' in v:
     p=E/f'image-{self.serial}.png';p.write_bytes(base64.b64decode(v['data']));return {'type':'image','path':str(p)}
    return {k:clean(x) for k,x in v.items()}
   if isinstance(v,list):return [clean(x) for x in v]
   return v
  with (E/'public-transcript.jsonl').open('a') as f:f.write(json.dumps({'call':self.serial,'method':method,'arguments':args,'elapsed':time.monotonic()-t,'result':clean(r),'error':err})+'\n')
  if err:raise RuntimeError(err)
  return r
c=Logged(ROOT/'target/release/fold-cli')
(E/'ready').write_text(c.session)
try:
 for i in range(150):
  p=E/f'request-{i}.json'
  while not p.exists():time.sleep(.2)
  q=json.loads(p.read_text())
  if q['method']=='close':break
  try:
   r=c.tool(q['method'],dict(session=c.session,**q.get('args',{})))
   if q.get('job'):r=c.job(r)
   def strip(v):
    if isinstance(v,dict):
     if v.get('type')=='image' and 'data'in v:return {'path':str(E/f'image-{c.serial}.png')}
     return {k:strip(x) for k,x in v.items()}
    if isinstance(v,list):return [strip(x) for x in v]
    return v
   (E/f'response-{i}.json').write_text(json.dumps(strip(r),indent=2))
  except Exception as ex:(E/f'response-{i}.json').write_text(json.dumps({'error':str(ex)}))
finally:c.close()
