import os,subprocess,json,statistics,time,pathlib,gzip,hashlib,datetime
from contextlib import contextmanager
root=pathlib.Path('/home/rgame/.codex/worktrees/efc8/Fold')
out=root/'docs/rewrite/evidence/compositor-render-audit/improvements'
out.mkdir(exist_ok=True)
env=os.environ.copy();env.update(FOLD_COLOR_ROOT=str(root/'target'),FOLD_VIDEO_HELPER='/tmp/fold-render-audit/video-runtime/bin/fold-video-helper',FOLD_VIDEO_DECODER='cuda-native')
bins={'before':'/tmp/fold-render-audit/candidate-final','after':str(root/'target/release/examples/render_profile')}
clips={'hevc':'/home/rgame/Videos/Create-one-continuous-feeling-15-second.mp4','prores':'/home/rgame/Videos/Cutdown_02_VFX_v01.mov'}
rows=[]
(out/'provenance.json').write_text(json.dumps(dict(started_utc=datetime.datetime.now(datetime.timezone.utc).isoformat(),
    binaries={key:dict(path=path,sha256=hashlib.sha256(pathlib.Path(path).read_bytes()).hexdigest()) for key,path in bins.items()},
    frames_per_run=90,warmup_frames=10,trials=3),indent=2)+'\n')
@contextmanager
def clocks():
 with open(out/'clocks.csv','w') as log:
  process=subprocess.Popen(['nvidia-smi','--query-gpu=timestamp,clocks.gr,clocks.mem,utilization.gpu,memory.used,temperature.gpu','--format=csv','-lms','1000'],stdout=log)
  try: yield
  finally:
   process.terminate()
   process.wait(timeout=10)
with clocks(), gzip.open(out/'frames.jsonl.gz','wt') as raw,open(out/'runs.log','w') as log:
 for trial in range(3):
  for clip,path in clips.items():
   for stage in ['source','full']:
    for build in (['before','after'] if trial%2==0 else ['after','before']):
     cmd=[bins[build],path,'90','8',stage,'forward']
     run=subprocess.run(cmd,env=env,cwd=root,text=True,capture_output=True)
     log.write(f'{trial} {clip} {stage} {build}\n{run.stderr}\n');log.flush()
     if run.returncode: raise RuntimeError(run.stderr)
     data=[json.loads(s) for s in run.stdout.splitlines()]
     for d in data: raw.write(json.dumps(dict(d,trial=trial,clip=clip,build=build))+'\n')
     warm=data[10:]
     row=dict(trial=trial,clip=clip,stage=stage,build=build,**{k:statistics.median(d[k] for d in warm) for k in ['wall_ms','gpu_ms','decode_ms','peak_bytes','passes']})
     rows.append(row);print(json.dumps(row),flush=True)
(out/'summary.json').write_text(json.dumps(rows,indent=2)+'\n')
