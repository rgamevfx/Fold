#!/usr/bin/env python3
"""Capture a saved reference trial at every encoded GIF timestamp via public Fold CLI.

Read-only: uses an explicitly supplied session/document and never seeks the viewer.
Requires Pillow. Produces evidence for independent review, not an automatic aesthetic pass.
"""
import argparse
import base64
import hashlib
import json
from pathlib import Path
import subprocess
import time
from PIL import Image, ImageDraw


def main():
    p = argparse.ArgumentParser(description=__doc__)
    p.add_argument('--cli', required=True, type=Path)
    p.add_argument('--session', required=True)
    p.add_argument('--document', required=True)
    p.add_argument('--reference', required=True, type=Path)
    p.add_argument('--output', required=True, type=Path)
    a = p.parse_args()
    a.output.mkdir(parents=True, exist_ok=True)
    transcript = a.output / 'capture-transcript.jsonl'

    def call(method, args):
        begin = time.monotonic()
        proc = subprocess.run([str(a.cli.resolve()), 'agent', 'call', a.session, method, '-'],
                              input=json.dumps(args), text=True, capture_output=True, timeout=90)
        response = json.loads(proc.stdout)
        with transcript.open('a') as log:
            log.write(json.dumps({'method': method, 'args': args, 'response': response,
                                  'elapsed_seconds': time.monotonic()-begin})+'\n')
        if proc.returncode or not response.get('ok'):
            raise RuntimeError(response)
        return response['result']

    def complete(job):
        deadline = time.monotonic()+90
        while time.monotonic() < deadline:
            result = call('jobs', {'job': job})
            if result['status'] == 'complete':
                if not result['outcome']['ok']:
                    raise RuntimeError(result['outcome'])
                return result['outcome']['result']
            time.sleep(0.1)
        raise TimeoutError(f'job {job}')

    before = call('state', {})
    reference = Image.open(a.reference)
    frames, evidence, durations = [], [], []
    ms = 0
    for index in range(reference.n_frames):
        reference.seek(index)
        duration = reference.info['duration']
        durations.append(duration)
        requested = {'numerator': ms, 'denominator': 1000}
        job = call('observe', {'source': {'document': a.document, 'output': 'video'},
                              'times': [requested], 'dimensions': list(reference.size), 'queries': []})
        result = complete(job['job'])
        raw = base64.b64decode(result['image']['data'])
        path = a.output / f'frame-{index:02d}.png'
        path.write_bytes(raw)
        frames.append(Image.open(path).convert('RGB'))
        evidence.append({'index': index, 'reference_time_ms': ms, 'duration_ms': duration,
                         'sha256': hashlib.sha256(raw).hexdigest(), 'metadata': result['metadata']})
        ms += duration
        print(f'{index+1}/{reference.n_frames}', flush=True)
    after = call('state', {})
    if (before['revision'], before['project_epoch']) != (after['revision'], after['project_epoch']):
        raise RuntimeError('project changed during capture; evidence spans different roots')
    frames[0].save(a.output/'preview.gif', save_all=True, append_images=frames[1:],
                   duration=durations, loop=0)
    sheet = Image.new('RGB', (6*180, ((len(frames)+5)//6)*202), '#242424')
    draw = ImageDraw.Draw(sheet)
    for i, frame in enumerate(frames):
        x,y=(i%6)*180,(i//6)*202
        sheet.paste(frame.resize((180,180)),(x,y))
        draw.text((x+4,y+183), f'{i:02d}  {evidence[i]["reference_time_ms"]} ms',fill='white')
    sheet.save(a.output/'contact-sheet.jpg', quality=95)
    (a.output/'evidence.json').write_text(json.dumps({
        'reference_sha256': hashlib.sha256(a.reference.read_bytes()).hexdigest(),
        'revision':before['revision'],'project_epoch':before['project_epoch'],
        'frames':evidence,'duration_ms':ms,
        'verdict':'ungraded; full-cycle image evidence only; controls, persistence and native editing need separate checks'
    }, indent=2)+'\n')


if __name__ == '__main__':
    main()
