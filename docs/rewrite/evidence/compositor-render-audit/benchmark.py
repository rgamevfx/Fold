import json, os, subprocess, time
from pathlib import Path
root = Path('/tmp/fold-render-audit')
env = dict(os.environ, FOLD_COLOR_ROOT=str(Path('target').resolve()), FOLD_VIDEO_HELPER=str(root/'video-runtime/bin/fold-video-helper'), FOLD_VIDEO_DECODER='cuda-native')
clips = [('hevc','Create-one-continuous-feeling-15-second.mp4'),('prores','Cutdown_02_VFX_v01.mov')]
for trial in range(3):
    for clip, name in clips:
        for stage in ['source','shared','blur','full']:
            # Alternate process ordering to reduce systematic clock/cache bias.
            for version in (['before','after'] if trial % 2 == 0 else ['after','before']):
                binary = root/('baseline-shared' if version == 'before' else 'candidate')
                stem = f'{version}-{clip}-{stage}-{trial}'
                with (root/f'{stem}.jsonl').open('w') as out, (root/f'{stem}.log').open('w') as err:
                    subprocess.run([str(binary),str(Path('/home/rgame/Videos')/name),'90','8',stage],env=env,stdout=out,stderr=err,check=True,timeout=120)
            print(trial,clip,stage,'done',flush=True)
