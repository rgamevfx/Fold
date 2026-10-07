import sys, time, json, base64, hashlib
from pathlib import Path
from PIL import Image, ImageChops, ImageStat
from io import BytesIO
ROOT=Path('/home/rgame/dev/Fold_Dev/Fold')
sys.path.insert(0,str(ROOT/'skills/fold/scripts'))
from live_badge import Client
OUT=ROOT/'docs/evidence/phase-56/independent-review'
OUT.mkdir(exist_ok=True)
class Logged(Client):
 def tool(self,method,args):
  start=time.monotonic()
  try:r=super().tool(method,args)
  except Exception as e:
   with (OUT/'public-transcript.jsonl').open('a') as f:f.write(json.dumps({'method':method,'elapsed':time.monotonic()-start,'error':str(e)})+'\n')
   raise
  def clean(v):
   if isinstance(v,dict):
    if 'data' in v and v.get('type')=='image':return {'type':'image','sha256':hashlib.sha256(base64.b64decode(v['data'])).hexdigest()}
    return {k:clean(x) for k,x in v.items()}
   if isinstance(v,list):return [clean(x) for x in v]
   return v
  with (OUT/'public-transcript.jsonl').open('a') as f:f.write(json.dumps({'method':method,'arguments':args,'elapsed':time.monotonic()-start,'result':clean(r)})+'\n')
  return r
 def job(self,a):
  start=time.monotonic()
  while time.monotonic()-start<120:
   time.sleep(.5)
   r=self.call('jobs',job=a['job'])
   if r['status']=='complete':
    if not r['outcome']['ok']:raise RuntimeError(r['outcome']['error'])
    result=r['outcome']['result']
    if r.get('_images'):result['image']=r['_images'][0]
    return result
  raise TimeoutError(a)
c=Logged(ROOT/'target/release/fold-cli',ROOT/'demos/cell-type-rust.fold')
try:
 state=c.call('state')
 document=next(d['id'] for d in state['documents'] if d['type']=='fold.composition.document')
 (OUT/'state.json').write_text(json.dumps(state,indent=2))
 frames=[]
 for start in range(0,150,12):
  indices=list(range(start,min(150,start+12)))
  result=c.job(c.call('observe',source={'document':document,'output':'video'},times=[{'numerator':i,'denominator':25} for i in indices],dimensions=[250,444]))
  picture=Image.open(BytesIO(base64.b64decode(result.pop('image')['data']))).convert('RGB')
  picture.save(OUT/f'strip-{start:03}.png')
  (OUT/f'strip-{start:03}.json').write_text(json.dumps(result,indent=2))
  if start==0:print(json.dumps(result)[:2000],flush=True)
  # The public observation receipt describes each tile's actual rectangle.
  for row in result['metadata']['frames']:
   rect=row['rect']
   if isinstance(rect,dict):x,y,w,h=rect['x'],rect['y'],rect['width'],rect['height']
   else:x,y,w,h=rect
   frames.append(picture.crop((x,y,x+w,y+h)))
  print('frames',len(frames),flush=True)
 frames[0].save(OUT/'cycle-25fps.gif',save_all=True,append_images=frames[1:],duration=40,loop=0,disposal=2)
 contact=Image.new('RGB',(1000,1332))
 for j,i in enumerate(range(0,150,13)):
  contact.paste(frames[i],((j%4)*250,(j//4)*444))
 contact.save(OUT/'contact-sheet.jpg',quality=95)
 result=c.job(c.call('render',source={'document':document,'output':'video'},time={'numerator':6,'denominator':1},dimensions=[250,444]))
 repeated=Image.open(BytesIO(base64.b64decode(result['image']['data']))).convert('RGB')
 differences=[sum(ImageStat.Stat(ImageChops.difference(frames[i],frames[(i+1)%150])).mean)/3 for i in range(150)]
 summary={'frames':150,'fps':25,'period_seconds':6,'repeated_phase_equal':frames[0].tobytes()==repeated.tobytes(),'distinct_frames':len({hashlib.sha256(f.tobytes()).hexdigest() for f in frames}),'adjacent_mae':differences,'wrap_mae':differences[-1],'maximum_mae':max(differences),'binary_sha256':hashlib.sha256((ROOT/'target/release/fold-cli').read_bytes()).hexdigest(),'project_sha256':hashlib.sha256((ROOT/'demos/cell-type-rust.fold').read_bytes()).hexdigest()}
 (OUT/'cycle-check.json').write_text(json.dumps(summary,indent=2))
 print({k:v for k,v in summary.items() if k!='adjacent_mae'},flush=True)
finally:c.close()
