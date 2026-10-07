import sys,json,time,uuid,base64,hashlib
from pathlib import Path
R=Path('/home/rgame/dev/Fold_Dev/Fold'); O=R/'docs/evidence/phase-56/independent-review'
sys.path.insert(0,str(R/'skills/fold/scripts'))
from live_badge import Client
class Logged(Client):
 def tool(self,method,args):
  start=time.monotonic();r=super().tool(method,args)
  def clean(v):
   if isinstance(v,dict):
    if v.get('type')=='image' and 'data'in v:return {'type':'image','sha256':hashlib.sha256(base64.b64decode(v['data'])).hexdigest()}
    return {k:clean(x) for k,x in v.items()}
   if isinstance(v,list):return [clean(x) for x in v]
   return v
  with (O/'controls-transcript.jsonl').open('a') as f:f.write(json.dumps({'method':method,'arguments':args,'elapsed':time.monotonic()-start,'result':clean(r)})+'\n')
  return r
 def job(self,a):
  start=time.monotonic()
  while time.monotonic()-start<120:
   time.sleep(.1);r=self.call('jobs',job=a['job'])
   if r['status']=='complete':
    if not r['outcome']['ok']:raise RuntimeError(r['outcome']['error'])
    result=r['outcome']['result']
    if r.get('_images'):result['image']=r['_images'][0]
    return result
  raise TimeoutError(a)
c=Logged(R/'target/release/fold-cli',R/'demos/cell-type-rust.fold')
try:
 s=c.call('state');epoch=s['project_epoch']
 graph=next(d['id'] for d in s['documents'] if d['type']=='fold.motion.document')
 comp=next(d['id'] for d in s['documents'] if d['type']=='fold.composition.document')
 records=c.job(c.call('inspect',document=graph,pointer='/rust_assets'))['data']
 definition,record=next(iter(records.items()))
 nodes=c.job(c.call('inspect',document=graph,pointer='/graph/nodes'))['data']
 speed=next(n for n in nodes if n['kind']=='fold.motion.published_control')
 candidate=c.call('candidate',action='begin',request_id=str(uuid.uuid4()),revision=s['revision'],project_epoch=epoch)
 cid=candidate['candidate'];revision=candidate['revision'];generation=0
 def edit(edits):
  global revision,generation
  start=time.monotonic()
  batch={'request_id':str(uuid.uuid4()),'label':'Independent source/control verification','revision':revision,'project_epoch':epoch,'operations':[{'op':'edit','document':graph,'edits':edits}]}
  result=c.job(c.call('candidate',action='edit',request_id=str(uuid.uuid4()),candidate=cid,generation=generation,project_epoch=epoch,batch=batch))
  generation=result['generation'];revision=result['revision']
  return time.monotonic()-start
 source_edit={'op':'rust_asset','definition':definition,'source':record['source']}
 cold=edit([source_edit]);warm=edit([source_edit])
 after=c.job(c.call('inspect',document=graph,pointer='/graph/nodes',candidate=cid,generation=generation,project_epoch=epoch))['data']
 assert after==nodes,'Source revision changed artist-owned nodes'
 baseline=c.job(c.call('render',source={'document':comp,'output':'video'},time={'numerator':1,'denominator':1},dimensions=[500,888],candidate=cid,generation=generation,project_epoch=epoch))
 (O/'speed-original.png').write_bytes(base64.b64decode(baseline['image']['data']))
 edit([{'op':'set','node':speed['id'],'input':'default','value':{'Scalar':.25}}])
 variant=c.job(c.call('render',source={'document':comp,'output':'video'},time={'numerator':1,'denominator':1},dimensions=[500,888],candidate=cid,generation=generation,project_epoch=epoch))
 (O/'speed-half.png').write_bytes(base64.b64decode(variant['image']['data']))
 assert baseline['image']['data']!=variant['image']['data']
 c.call('candidate',action='discard',request_id=str(uuid.uuid4()),candidate=cid,generation=generation,project_epoch=epoch)
 assert c.call('state')['revision']==s['revision']
 summary={'cold_author_revision_seconds':cold,'warm_identical_revision_seconds':warm,'poll_seconds':.1,'artist_nodes_identical_after_two_revisions':True,'speed_render_changed':True,'live_revision_unchanged':True,'cold_scope':'fresh server, author compiler and runtime Wrangle compiler caches cold; source lowering and native validation included; wall time includes polling','binary_sha256':hashlib.sha256((R/'target/release/fold-cli').read_bytes()).hexdigest()}
 (O/'controls-check.json').write_text(json.dumps(summary,indent=2));print(summary)
finally:c.close()
