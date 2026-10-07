"""Independent full-cycle public observation: PROJECT FRAME_COUNT FPS OUTPUT_DIR."""
import base64, hashlib, json, sys, time
from io import BytesIO
from pathlib import Path
from PIL import Image, ImageChops, ImageStat
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'skills/fold/scripts'))
from live_badge import Client
project=Path(sys.argv[1]).resolve(); count=int(sys.argv[2]); fps=int(sys.argv[3]); out=Path(sys.argv[4])
out.mkdir(parents=True,exist_ok=True)
class Logged(Client):
    def tool(self,method,args):
        start=time.monotonic()
        result=super().tool(method,args)
        def clean(v):
            if isinstance(v,dict):
                if v.get('type')=='image' and 'data' in v:
                    return {'type':'image','sha256':hashlib.sha256(base64.b64decode(v['data'])).hexdigest()}
                return {k:clean(x) for k,x in v.items()}
            if isinstance(v,list):return [clean(x) for x in v]
            return v
        with (out/'transcript.jsonl').open('a') as f:f.write(json.dumps({'method':method,'arguments':args,'elapsed':time.monotonic()-start,'result':clean(result)})+'\n')
        return result
c=Logged(ROOT/'target/release/fold-cli',project)
try:
    state=c.call('state')
    doc=next(d['id'] for d in state['documents'] if d['type']=='fold.composition.document')
    (out/'state.json').write_text(json.dumps(state,indent=2))
    frames=[]; timings=[]
    for first in range(0,count,12):
        indices=list(range(first,min(count,first+12)))
        if (out/f'strip-{first:03}.png').exists() and (out/f'strip-{first:03}.json').exists():
            r=json.loads((out/f'strip-{first:03}.json').read_text())
            picture=Image.open(out/f'strip-{first:03}.png').convert('RGB')
        else:
            r=c.job(c.call('observe',source={'document':doc,'output':'video'},times=[{'numerator':i,'denominator':fps} for i in indices],dimensions=[250,444]))
            picture=Image.open(BytesIO(base64.b64decode(r.pop('image')['data']))).convert('RGB')
            picture.save(out/f'strip-{first:03}.png'); (out/f'strip-{first:03}.json').write_text(json.dumps(r,indent=2))
        for metadata in r['metadata']['frames']:
            rect=metadata['rect']
            if isinstance(rect,dict):x,y,w,h=[rect[k] for k in ['x','y','width','height']]
            else:x,y,w,h=rect
            frames.append(picture.crop((x,y,x+w,y+h)))
            timings.append(metadata)
        print(f'Reviewed render {len(frames)}/{count}',flush=True)
    frames[0].save(out/'cycle.gif',save_all=True,append_images=frames[1:],duration=1000/fps,loop=0,disposal=2)
    sheet=Image.new('RGB',(1000,1332))
    for j in range(12):sheet.paste(frames[j*count//12],((j%4)*250,(j//4)*444))
    sheet.save(out/'contact-sheet.jpg',quality=95)
    seam=Image.new('RGB',(1000,444))
    for j,i in enumerate([count-2,count-1,0,1]):seam.paste(frames[i],(j*250,0))
    seam.save(out/'seam.jpg',quality=95)
    endpoint_equal=None
    endpoint_note=None
    try:
        r=c.job(c.call('render',source={'document':doc,'output':'video'},time={'numerator':count,'denominator':fps},dimensions=[250,444]))
        endpoint=Image.open(BytesIO(base64.b64decode(r['image']['data']))).convert('RGB')
        endpoint.save(out/'endpoint.png')
        endpoint_equal=frames[0].tobytes()==endpoint.tobytes()
    except RuntimeError as error:
        if str(error)!='render time outside output range':raise
        endpoint_note=str(error)+'; review last frame to first frame and repeated phases inside the valid range'
    diffs=[sum(ImageStat.Stat(ImageChops.difference(frames[i],frames[(i+1)%count])).mean)/3 for i in range(count)]
    summary={'frames':count,'fps':fps,'period_seconds':count/fps,'endpoint_equal':endpoint_equal,'endpoint_note':endpoint_note,'half_cycle_equal':all(frames[i].tobytes()==frames[i+count//2].tobytes() for i in range(count//2)),'distinct_frames':len({hashlib.sha256(f.tobytes()).hexdigest() for f in frames}),'adjacent_mae':diffs,'wrap_mae':diffs[-1],'max_mae':max(diffs),'binary_sha256':hashlib.sha256((ROOT/'target/release/fold-cli').read_bytes()).hexdigest(),'project_sha256':hashlib.sha256(project.read_bytes()).hexdigest()}
    (out/'cycle-check.json').write_text(json.dumps(summary,indent=2));print({k:v for k,v in summary.items() if k!='adjacent_mae'},flush=True)
finally:c.close()
