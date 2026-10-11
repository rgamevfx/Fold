import json, math, statistics
from pathlib import Path
root=Path('/tmp/fold-render-audit')
def pct(v,q):
    v=sorted(v);return round(v[math.ceil(q*len(v))-1],4)
results=[]
for clip in ['hevc','prores']:
 for stage in ['source','shared','blur','full']:
  for version in ['before','after']:
   trials=[]
   for trial in range(3):
    p=root/f'{version}-{clip}-{stage}-{trial}.jsonl'
    if not p.exists():continue
    rows=[json.loads(x) for x in p.read_text().splitlines()]
    if len(rows)!=90:continue
    trials.append(rows)
   if len(trials)!=3:continue
   warm=[r for t in trials for r in t[10:]]
   result=dict(clip=clip,stage=stage,version=version,trials=3,warm_samples=len(warm),cold_wall_ms=[t[0]['wall_ms'] for t in trials])
   for key in ['wall_ms','gpu_ms','scene_gpu_ms','compile_ms','decode_ms','evaluate_cpu_ms','output_cpu_ms','queue_submit_ms']:
    result[key]={p:pct([r[key] for r in warm],q) for p,q in [('p50',.5),('p95',.95),('p99',.99)]}
   for key in ['passes','upload_bytes','native_copy_bytes','pipe_bytes','readback_bytes','status_readback_bytes','peak_bytes']:
    result[key]=max(r[key] for t in trials for r in t)
   results.append(result)
(root/'summary.json').write_text(json.dumps(results,indent=2)+'\n')
for r in results: print(r['clip'],r['stage'],r['version'],'wall',r['wall_ms'],'gpu',r['gpu_ms'],'memory',r['peak_bytes'],'passes',r['passes'])
