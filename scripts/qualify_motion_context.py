#!/usr/bin/env python3
"""Check the generated native/script Motion worker and save frame evidence."""
import argparse, json, subprocess, pathlib, array, math, zlib, struct, sys
parser=argparse.ArgumentParser(description=__doc__)
parser.add_argument('root',type=pathlib.Path)
parser.add_argument('--worker-profile', choices=['debug', 'release', 'unknown'], default='unknown')
parser.add_argument('--scaled-reference',type=pathlib.Path)
parser.add_argument('--reference-scale',type=float,default=1.0)
transport=parser.add_mutually_exclusive_group(required=True)
transport.add_argument('--worker',type=pathlib.Path)
transport.add_argument('--readback',type=pathlib.Path)
args=parser.parse_args()
root=args.root
requests=[json.loads(line) for line in (root/'requests.jsonl').read_text().splitlines()]
for obj in requests[1]['canvas']['objects'].values():
 if obj['content']['kind']=='script':
  for control in obj['content']['controls'].values():
   if control['name']=='Count': control['value']['value']['value']=8000
payload=''.join(json.dumps(req)+'\n' for req in requests).encode()
if args.worker:
 p=subprocess.run([str(args.worker)],input=payload,stdout=subprocess.PIPE,stderr=subprocess.PIPE,timeout=45)
 (root/'worker-stderr.txt').write_bytes(p.stderr)
 print('worker_exit',p.returncode,flush=True)
 print(p.stderr.decode()[-1200:],flush=True)
 output=p.stdout; exit_code=p.returncode
else:
 output=args.readback.read_bytes(); exit_code=0
frames=[]; offset=0; evidence=[]
def chunk(tag,data): return struct.pack('>I',len(data))+tag+data+struct.pack('>I',zlib.crc32(tag+data))
for i in range(3):
 end=output.find(b'\n',offset)
 if end<0: raise RuntimeError('Missing worker header')
 header=json.loads(output[offset:end]); print(header,flush=True)
 assert header['id']==requests[i]['id']
 assert header['encoding']=='linear_srgb' and header['alpha']=='premultiplied'
 assert header['dimensions']==[640,360] and header['bytes']==640*360*16
 if header['error']: raise RuntimeError(header['error'])
 start=end+1; offset=start+header['bytes']
 frame=array.array('f');frame.frombytes(output[start:offset])
 if sys.byteorder!='little': frame.byteswap()
 assert len(frame)==640*360*4 and all(math.isfinite(v) for v in frame)
 alpha=frame[3::4]; assert min(alpha)>=0 and max(alpha)<=1
 text=sum(v>0 for v in alpha[:80*640]); dots=sum(v>0 for v in alpha[110*640:270*640]); assert text>100 and dots>10000
 strokes=sum(v>0 for v in alpha[280*640:]); partial_strokes=sum(0<v<1 for v in alpha[280*640:])
 if any(obj['content']['kind']=='path' for obj in requests[i]['canvas']['objects'].values()):
  assert strokes>1000 and partial_strokes>500, (strokes,partial_strokes)
 frames.append(frame); rgb=bytearray()
 for j in range(0,len(frame),4):
  for c,bg in enumerate([0.015,0.015,0.022]):
   v=frame[j+c]+(1-frame[j+3])*bg
   v=12.92*v if v<=0.0031308 else 1.055*max(v,0)**(1/2.4)-0.055
   rgb.append(round(min(1,max(0,v))*255))
 raw=b''.join(b'\0'+rgb[y*640*3:(y+1)*640*3] for y in range(360))
 png=b'\x89PNG\r\n\x1a\n'+chunk(b'IHDR',struct.pack('>IIBBBBB',640,360,8,2,0,0,0))+chunk(b'IDAT',zlib.compress(raw))+chunk(b'IEND',b'')
 (root/f'frame-{i}.png').write_bytes(png)
 evidence.append({'frame':i,'text_pixels':text,'dot_pixels':dots,'stroke_pixels':strokes,'partial_alpha_stroke_pixels':partial_strokes,'alpha_min':min(alpha),'alpha_max':max(alpha),'dimensions':header['dimensions']})
assert offset==len(output) and exit_code==0
assert max(abs(a-b) for a,b in zip(frames[0],frames[2]))>0.01
assert evidence[1]['dot_pixels']<evidence[0]['dot_pixels']
stroke_motion=None
if any(obj['content']['kind']=='path' for obj in requests[0]['canvas']['objects'].values()):
 stroke_motion=max(abs(a-b) for a,b in zip(frames[0][280*640*4:],frames[2][280*640*4:]))>0.01
 assert stroke_motion
if args.scaled_reference:
 assert math.isfinite(args.reference_scale) and 0<=args.reference_scale<=1
 reference=args.scaled_reference.read_bytes(); position=0; errors=[]
 for i,frame in enumerate(frames):
  end=reference.find(b'\n',position); assert end>=0
  header=json.loads(reference[position:end]); assert header['id']==requests[i]['id']
  assert header['dimensions']==[640,360] and header['bytes']==len(frame)*4
  assert header['encoding']=='linear_srgb' and header['alpha']=='premultiplied' and not header['error']
  position=end+1; values=array.array('f');values.frombytes(reference[position:position+header['bytes']]);position+=header['bytes']
  if sys.byteorder!='little': values.byteswap()
  assert len(values)==len(frame) and all(math.isfinite(v) for v in values)
  error=max(abs(actual-expected*args.reference_scale) for actual,expected in zip(frame,values))
  assert error<=1e-4,error
  errors.append(error)
 assert position==len(reference)
 (root/'graph-pixel-evidence.json').write_text(json.dumps({'scale':args.reference_scale,'maximum_absolute_errors':errors,'passed':True},indent=2)+'\n')
(root/'render-evidence.json').write_text(json.dumps({'frames':evidence,'runtime_count_edit':True,'animation_changes_pixels':True,'subpixel_stroke_motion_changes_pixels':stroke_motion,'worker_profile':args.worker_profile,'performance_qualified':False},indent=2))
print('Native text, 10,000 script dots, animation and runtime count edit verified.',flush=True)
