#!/usr/bin/env python3
"""Exercise the shipped MCP + native SDK candidate workflow in an isolated headless session.

Requires a built fold-cli. Never attaches to an artist's session.
"""
import argparse
import hashlib
import json
import os
from pathlib import Path
import subprocess
import tempfile
import time

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument('cli', type=Path)
args = parser.parse_args()
cli = str(args.cli.resolve())
with tempfile.TemporaryDirectory(prefix='fold-agent-contract-') as work:
    environment = dict(os.environ, XDG_RUNTIME_DIR=work)
    server = subprocess.Popen([cli, 'agent', 'serve'], env=environment, stdout=subprocess.DEVNULL)
    mcp = subprocess.Popen([cli, 'agent', 'mcp'], env=environment, stdin=subprocess.PIPE, stdout=subprocess.PIPE, text=True)
    serial = 0
    last_images = []
    def rpc(method, params):
        global serial
        serial += 1
        mcp.stdin.write(json.dumps(dict(jsonrpc='2.0', id=serial, method=method, params=params))+'\n')
        mcp.stdin.flush()
        response = json.loads(mcp.stdout.readline())
        assert 'error' not in response, response
        return response['result']
    def tool(method, arguments):
        global last_images
        response = rpc('tools/call', {'name':'fold_'+method,'arguments':arguments})
        last_images = [hashlib.sha256(c['data'].encode()).hexdigest() for c in response['content'] if c['type']=='image']
        result = json.loads(response['content'][0]['text'])
        assert result['ok'], result
        return result['result']
    try:
        initialized = rpc('initialize', {'protocolVersion':'2025-11-25'})
        assert 'fold_candidate' in initialized['instructions']
        names = {t['name'] for t in rpc('tools/list', {})['tools']}
        assert {'fold_candidate','fold_observe','fold_author'} <= names
        deadline = time.monotonic()+15
        while True:
            sessions = tool('sessions', {})
            if sessions:
                session = sessions[0]['session']
                break
            assert server.poll() is None, 'isolated server exited'
            assert time.monotonic()<deadline, 'session startup timeout'
            time.sleep(.02)
        def call(method, **arguments):
            return tool(method,dict(session=session,**arguments))
        def job(accepted):
            deadline = time.monotonic()+30
            while True:
                response = call('jobs',job=accepted['job'])
                if response['status']=='complete':
                    assert response['outcome']['ok'], response
                    return response['outcome']['result']
                assert time.monotonic()<deadline, 'job timeout'
                time.sleep(.02)
        before=call('state')
        epoch=before['project_epoch']
        base=before['revision']
        sdk=call('catalog',sdk=True)
        assert 'ProjectOperation' in sdk['operation_declarations']
        source_code = "plan.document('title').info({width:320,height:180,frames:192,rate:[24,1]}).text('label','i love katie',{size:24});"
        private=call('candidate',action='begin',request_id='begin',revision=base,project_epoch=epoch)
        candidate=private['candidate']
        edited=job(call('author',request_id='edit',candidate=candidate,generation=0,project_epoch=epoch,label='SDK title',source=source_code))
        assert call('state')['revision']==base
        source={'document':edited['ids']['title'],'output':'video'}
        observation=job(call('observe',source=source,times=[{'numerator':0,'denominator':1},{'numerator':1,'denominator':2}],dimensions=[160,90],candidate=candidate,generation=1,project_epoch=epoch,queries=[{'mode':'evaluated','fields':['counts']}]))
        private_images=last_images.copy()
        assert len(private_images)==1, 'expected MCP image content'
        metadata=observation['metadata']
        assert metadata['generation']==1 and metadata['candidate']==candidate
        assert metadata['frames'][0]['measurements'][0]['data']['counts']['glyphs']>0
        committed=call('candidate',action='commit',request_id='commit',candidate=candidate,generation=1,revision=base,project_epoch=epoch,label='SDK title')
        assert committed['committed']
        job(call('observe',source=source,times=[{'numerator':0,'denominator':1},{'numerator':1,'denominator':2}],dimensions=[160,90]))
        assert last_images==private_images, 'candidate and committed frames differ'
        after=call('state')
        assert after['document_count']==before['document_count']+1
        call('undo',request_id='undo',revision=after['revision'],project_epoch=epoch)
        assert call('state')['document_count']==before['document_count']
        print(json.dumps({'passed':True,'workflow':'MCP discover, SDK, candidate edit, pinned strip and measurements, atomic commit, undo','reference_benchmark':'not run'}))
    finally:
        for process in (mcp,server):
            process.terminate()
            try:
                process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                process.kill()
                process.wait()
