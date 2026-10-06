#!/usr/bin/env python3
"""Create and verify a reusable Rust orbit tool through a live Fold session.
Usage: wrangle.py /path/to/fold-cli SESSION /new/project.fold
Adds one Motion document; saves a new project copy and PNG previews. No archive edits.
"""
import argparse
import base64
import json
from pathlib import Path
import subprocess
import time
import uuid

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('cli')
parser.add_argument('session')
parser.add_argument('save', type=Path)
args = parser.parse_args()
args.save = args.save.resolve()
if args.save.exists():
    raise FileExistsError(args.save)


def call(method, params):
    reply = subprocess.run([args.cli, 'agent', 'call', args.session, method, '-'],
                           input=json.dumps(params), capture_output=True, text=True)
    if not reply.stdout:
        raise RuntimeError(reply.stderr)
    data = json.loads(reply.stdout)
    if not data['ok']:
        raise RuntimeError(data['error'])
    return data['result']


def wait(job, success=True):
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        result = call('jobs', {'job': job})
        if result['status'] == 'complete':
            outcome = result['outcome']
            if outcome['ok'] != success:
                raise RuntimeError(outcome)
            return outcome.get('result') if success else outcome['error']
        time.sleep(.05)
    raise TimeoutError(job)


def batch(operations, ids=(), success=True, label='Rust orbit tool'):
    state = call('state', {})
    return wait(call('batch', {'request_id': str(uuid.uuid4()),
        'revision': state['revision'], 'project_epoch': state['project_epoch'],
        'label': label, 'ids': list(ids), 'operations': operations})['job'], success)


def ref(name):
    return {'$ref': name}


def literal(kind, value):
    return {'Value': {kind: value}}


def link(node, socket):
    return {'Link': {'node': node, 'socket': socket}}


# Check the running contract before making changes.
catalog = call('catalog', {'type': 'fold.motion.document', 'query': 'wrangle'})
if 'fold.motion.wrangle' not in json.dumps(catalog):
    raise RuntimeError('This desktop does not expose Rust Wrangle')
source = '''let phase = tau * index as f64 / count as f64;
let angle = phase + tau * time.seconds * chf("speed");
let radius = chf("radius") * (1.0 + 0.15 * (phase * 3.0).sin());
position = [radius * angle.cos(), radius * angle.sin()];
set_attrf("phase", phase);'''
start = time.monotonic()
created = batch([
    {'op': 'create', 'id': ref('motion'), 'kind': 'fold.motion.document', 'name': 'Rust Orbit'},
    {'op': 'edit', 'document': ref('motion'), 'edits': [
        {'op': 'info', 'value': {'width': 640, 'height': 360, 'rate': [24, 1], 'frames': 120}},
        {'op': 'object', 'id': ref('dot'), 'kind': 'fold.motion.ellipse', 'name': 'Orbit dots'},
        {'op': 'set', 'node': ref('dot'), 'input': 'width', 'value': {'Scalar': 9}},
        {'op': 'set', 'node': ref('dot'), 'input': 'height', 'value': {'Scalar': 9}},
        {'op': 'create_node', 'id': ref('points'), 'kind': 'fold.motion.linear_points',
         'inputs': {'count': literal('Scalar', 40)}},
        {'op': 'create_node', 'id': ref('wrangle'), 'kind': 'fold.motion.wrangle',
         'settings': {'source': source}, 'inputs': {
             'content': link(ref('points'), 'points'), 'radius': literal('Scalar', 120),
             'speed': literal('Scalar', .1)}},
    ]}
], ['motion', 'dot', 'points', 'wrangle'])
cold_batch_ms = (time.monotonic() - start) * 1000
ids = created['ids']
roles = created['receipts'][0]['changes']['objects'][0]
# Use returned roles; no full graph inspection or ID reconstruction.
edits = [
    {'op': 'set', 'node': roles['appearance'], 'input': 'color', 'value': {'Color': [.07, .5, .8, 1]}},
    {'op': 'create_node', 'id': ref('copies'), 'kind': 'fold.motion.instance_on_points',
     'inputs': {'source': link(roles['appearance'], 'content'), 'points': link(ids['wrangle'], 'content')}},
    {'op': 'connect', 'node': roles['transform'], 'input': 'content',
     'source': {'node': ref('copies'), 'socket': 'content'}},
    {'op': 'set', 'node': roles['transform'], 'input': 'translation', 'value': {'Vector': [320, 180]}},
    {'op': 'group_nodes', 'nodes': [ids['wrangle']], 'id': ref('tool'),
     'definition': ref('definition'), 'name': 'Rust Orbit'},
    {'op': 'publish', 'group': ref('definition'), 'node': ids['wrangle'],
     'input': 'radius', 'port': 'radius', 'name': 'Radius', 'section': 'Orbit'},
    {'op': 'publish', 'group': ref('definition'), 'node': ids['wrangle'],
     'input': 'speed', 'port': 'speed', 'name': 'Speed', 'section': 'Orbit'},
    {'op': 'port', 'group': ref('definition'), 'port': 'input_0', 'name': 'Points'},
    {'op': 'layout', 'nodes': [ids['points'], ref('tool'), ref('copies')], 'origin': [100, 100]},
]
setup = batch([{'op': 'edit', 'document': ids['motion'], 'edits': edits}], ['copies', 'tool', 'definition'])
ids.update(setup['ids'])


def edit(edits, **kwargs):
    return batch([{'op': 'edit', 'document': ids['motion'], 'edits': edits}], **kwargs)


def render(n, d=1):
    return wait(call('render', {'source': {'document': ids['motion'], 'output': 'video'},
        'time': {'numerator': n, 'denominator': d}, 'dimensions': [640, 360]})['job'])


frames = []
render_times = []
for n, d in [(0, 1), (1, 2), (2, 1), (0, 1)]:
    start = time.monotonic()
    frames.append(render(n, d)['image']['data'])
    render_times.append((time.monotonic() - start) * 1000)
assert frames[0] == frames[-1], 'Out-of-order rendering changed the initial frame'
assert frames[0] != frames[1], 'Time did not animate the orbit'
revision = call('state', {})['revision']
error = edit([{'op': 'wrangle', 'group': ids['definition'], 'node': ids['wrangle'],
               'source': 'position[0] = missing_value;'}], success=False)
assert call('state', {})['revision'] == revision, 'Invalid Rust mutated the project'
edit([{'op': 'set', 'node': ids['tool'], 'input': 'radius', 'value': {'Scalar': 80}}], label='Verify radius control')
assert render(0)['image']['data'] != frames[0], 'Published radius has no effect'
state = call('state', {})
assert state['undo_label'] == 'Verify radius control'
call('undo', {'request_id': str(uuid.uuid4()), 'revision': state['revision'], 'project_epoch': state['project_epoch']})
assert render(0)['image']['data'] == frames[0], 'Undo did not restore the tool'
# Compile a source edit, render it, then restore the authored source.
edit([{'op': 'wrangle', 'group': ids['definition'], 'node': ids['wrangle'],
       'source': source.replace('0.15', '0.30')}])
assert render(0)['image']['data'] != frames[0], 'Hot source edit has no effect'
edit([{'op': 'wrangle', 'group': ids['definition'], 'node': ids['wrangle'], 'source': source}])
state = call('state', {})
wait(call('save', {'request_id': str(uuid.uuid4()), 'revision': state['revision'],
                  'project_epoch': state['project_epoch'], 'path': str(args.save)})['job'])
for i, frame in enumerate(frames[:3]):
    args.save.with_suffix(f'.{i}.png').write_bytes(base64.b64decode(frame))
print(json.dumps({'ids': ids, 'cold_batch_ms': cold_batch_ms,
    'render_roundtrip_ms': render_times, 'invalid_source': error, 'save': str(args.save)}, indent=2))
