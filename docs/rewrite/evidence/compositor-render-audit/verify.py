import os,subprocess
from pathlib import Path
root=Path('/tmp/fold-render-audit')
env=dict(os.environ,FOLD_COLOR_ROOT=str(Path('target').resolve()),FOLD_VIDEO_HELPER=str(root/'video-runtime/bin/fold-video-helper'),FOLD_VIDEO_DECODER='cuda-native',FOLD_PROFILE_VERIFY='1')
for clip,name in [('hevc','Create-one-continuous-feeling-15-second.mp4'),('prores','Cutdown_02_VFX_v01.mov')]:
 for stage,count,order in [('source',1,'forward'),('shared',1,'forward'),('full',360,'forward'),('shared',90,'reverse'),('shared',90,'random'),('shared',30,'held')]:
  if stage == 'full' and clip == 'hevc': count=361
  stem=f'verify-{clip}-{stage}-{order}'
  with (root/f'{stem}.jsonl').open('w') as out,(root/f'{stem}.log').open('w') as err:
   subprocess.run([str(root/'candidate-final'),str(Path('/home/rgame/Videos')/name),str(count),'8',stage,order],env=env,stdout=out,stderr=err,check=True,timeout=180)
  print(stem,'passed',flush=True)
