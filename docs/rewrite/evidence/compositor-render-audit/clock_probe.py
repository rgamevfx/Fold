import os,subprocess,time
from pathlib import Path
r=Path('/tmp/fold-render-audit')
e=dict(os.environ,FOLD_COLOR_ROOT=str(Path('target').resolve()),FOLD_VIDEO_HELPER=str(r/'video-runtime/bin/fold-video-helper'),FOLD_VIDEO_DECODER='cuda-native')
with (r/'gpu-clocks.csv').open('w') as clocks:
 monitor=subprocess.Popen(['nvidia-smi','--query-gpu=timestamp,clocks.gr,clocks.mem,temperature.gpu,power.draw,utilization.gpu,pstate','--format=csv','-lms','100'],stdout=clocks,stderr=subprocess.DEVNULL)
 try:
  for trial,version in enumerate(['before','after','after','before']):
   with (r/f'clock-{version}-{trial}.jsonl').open('w') as out,(r/f'clock-{version}-{trial}.log').open('w') as err:
    print(time.time(),version,'start',flush=True)
    subprocess.run([str(r/('baseline-shared' if version=='before' else 'candidate-final')),'/home/rgame/Videos/Cutdown_02_VFX_v01.mov','180','8','source'],env=e,stdout=out,stderr=err,check=True,timeout=180)
    print(time.time(),version,'end',flush=True)
 finally:
  monitor.terminate();monitor.wait()
