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
 with (E/'public-transcript.jsonl').open('a') as f:f.write(json.dumps({'call':count,'phase':'polished-reopen','over_invocation_budget':True,'method':method,'arguments':args,'elapsed':time.monotonic()-t,'result':clean(r),'error':err})+'\n')
 if err:raise RuntimeError(err)
 return r
m.Client.tool=logged
c=m.Client(R/'target/release/fold-cli',R/'demos/cell-type-rust.fold')
try:
 s=c.call('state');(E/'polished-reopened-state.json').write_text(json.dumps(s,indent=2))
 doc=next(d['id'] for d in s['documents'] if d['type']=='fold.composition.document');graph=next(d['id'] for d in s['documents'] if d['type']=='fold.motion.document')
 for method,args in [('inspect',{'document':graph,'pointer':'/graph/nodes'}),('render',{'source':{'document':doc,'output':'video'},'time':{'numerator':1,'denominator':1},'dimensions':[500,888]})]:
  j=c.call(method,**args)
  time.sleep(2)
  r=c.call('jobs',job=j['job'])
  (E/f'polished-reopened-{method}.json').write_text(json.dumps({k:v for k,v in r.items() if k!='_images'},indent=2))
  assert r['status']=='complete' and r['outcome']['ok']
finally:c.close()
print('Public invocations:',count)
