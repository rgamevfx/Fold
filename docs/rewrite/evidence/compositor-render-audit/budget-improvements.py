import os,subprocess,json,statistics,pathlib,gzip,math
root=pathlib.Path('/home/rgame/.codex/worktrees/efc8/Fold');out=root/'docs/rewrite/evidence/compositor-render-audit/improvements'
env=os.environ.copy();env.update(FOLD_COLOR_ROOT=str(root/'target'),FOLD_VIDEO_HELPER='/tmp/fold-render-audit/video-runtime/bin/fold-video-helper',FOLD_VIDEO_DECODER='cuda-native')
exe=str(root/'target/release/examples/render_profile');hevc='/home/rgame/Videos/Create-one-continuous-feeling-15-second.mp4';prores='/home/rgame/Videos/Cutdown_02_VFX_v01.mov'
rows=[]
with gzip.open(out/'budget-frames.jsonl.gz','wt') as raw,open(out/'budgets.log','w') as log:
 for budget in [1024,2048,4096]:
  env['FOLD_PROFILE_GPU_MIB']=str(budget)
  # Stable native identities isolate the GPU cache benefit without software
  # frame-identity expiration or ProRes reverse-seek process startup noise.
  run=subprocess.run([exe,hevc,'60','8','source','pingpong'],env=env,text=True,capture_output=True,cwd=root)
  log.write(f'{budget}\n{run.stderr}\n');log.flush()
  if run.returncode: raise RuntimeError(run.stderr)
  data=[json.loads(line) for line in run.stdout.splitlines()]
  for d in data:raw.write(json.dumps(dict(d,budget_mib=budget))+'\n')
  revisit=data[60:]
  rows.append(dict(budget_mib=budget,revisit_frames=len(revisit),native_copy_frames=sum(d['native_copy_bytes']>0 for d in revisit),wall_p50=statistics.median(d['wall_ms'] for d in revisit),wall_p95=sorted(d['wall_ms'] for d in revisit)[math.ceil(.95*len(revisit))-1],peak_mib=max(d['peak_bytes'] for d in data)/2**20))
  print(rows[-1],flush=True)
(out/'budgets.json').write_text(json.dumps(rows,indent=2)+'\n')
# Both distinct clips, including the two-software-lane branch, against CPU.
with open(out/'verification.log','w') as log:
 for backend,source,second,order in [('software',hevc,prores,'forward'),('cuda-native',hevc,prores,'forward'),('cuda-native',prores,None,'reverse'),('cuda-native',hevc,None,'random')]:
  verify=env.copy();verify.update(FOLD_VIDEO_DECODER=backend,FOLD_PROFILE_VERIFY='1')
  if second:verify['FOLD_PROFILE_SECOND']=second
  run=subprocess.run([exe,source,'4','8','full',order],env=verify,text=True,capture_output=True,cwd=root)
  log.write(f'{backend} {source} second={second} order={order}\n{run.stderr}\n');log.flush()
  if run.returncode:raise RuntimeError(run.stderr)
  for line in run.stdout.splitlines():
   d=json.loads(line)
   assert d['readback_bytes']==0
  print('verified',backend,source,order,flush=True)
