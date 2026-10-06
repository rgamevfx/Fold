#!/usr/bin/env python3
"""Reproduce and verify Badge through public MCP in an isolated Fold session.
Usage: live_badge.py FOLD_CLI NEW.fold [--baseline ORIGINAL.fold]
No running artist session is modified. Requires only Python's standard library.
"""
import argparse, base64, json, os, subprocess, tempfile, time, uuid
from pathlib import Path

class Client:
    def __init__(self, cli, project=None):
        self.runtime=tempfile.TemporaryDirectory(prefix='fold-badge-mcp-')
        env=dict(os.environ,XDG_RUNTIME_DIR=self.runtime.name)
        self.server=subprocess.Popen([str(cli),'agent','serve',*([str(project)] if project else [])],env=env,stdout=subprocess.DEVNULL)
        self.mcp=subprocess.Popen([str(cli),'agent','mcp'],env=env,stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
        self.serial=0
        self.rpc('initialize',{'protocolVersion':'2025-11-25'})
        deadline=time.monotonic()+20
        while True:
            sessions=self.tool('sessions',{})
            if sessions:
                self.session=sessions[0]['session'];break
            if time.monotonic()>deadline or self.server.poll() is not None: raise RuntimeError('Session startup failed')
            time.sleep(.05)
    def rpc(self,method,params):
        self.serial+=1
        self.mcp.stdin.write(json.dumps(dict(jsonrpc='2.0',id=self.serial,method=method,params=params))+'\n');self.mcp.stdin.flush()
        response=json.loads(self.mcp.stdout.readline())
        if 'error' in response: raise RuntimeError(response['error'])
        return response['result']
    def tool(self,method,args):
        result=self.rpc('tools/call',{'name':'fold_'+method,'arguments':args})
        reply=json.loads(result['content'][0]['text'])
        if not reply['ok']:raise RuntimeError(reply['error'])
        value=reply['result']
        images=[c for c in result['content'] if c['type']=='image']
        if images and isinstance(value,dict):value['_images']=images
        return value
    def call(self,method,**args):return self.tool(method,dict(session=self.session,**args))
    def job(self,accepted):
        deadline=time.monotonic()+120
        while time.monotonic()<deadline:
            r=self.call('jobs',job=accepted['job'])
            if r['status']=='complete':
                if not r['outcome']['ok']:raise RuntimeError(r['outcome']['error'])
                value=r['outcome']['result']
                if r.get('_images'):value['image']=r['_images'][0]
                return value
            time.sleep(.05)
        raise TimeoutError(accepted)
    def build(self,source):
        s=self.call('state');epoch=s['project_epoch']
        c=self.call('candidate',action='begin',request_id=str(uuid.uuid4()),revision=s['revision'],project_epoch=epoch)
        result=self.job(self.call('author',request_id=str(uuid.uuid4()),namespace='badge-demo',candidate=c['candidate'],generation=0,project_epoch=epoch,label='Badge workflow',source=source))
        self.call('candidate',action='commit',request_id=str(uuid.uuid4()),candidate=c['candidate'],generation=result['generation'],revision=s['revision'],project_epoch=epoch,label='Badge workflow')
        return result
    def render(self,document,numerator,denominator=1):
        return self.job(self.call('render',source={'document':document,'output':'video'},time={'numerator':numerator,'denominator':denominator},dimensions=[960,540]))['image']['data']
    def close(self):
        for p in [self.mcp,self.server]:
            p.terminate()
            try:p.wait(timeout=5)
            except subprocess.TimeoutExpired:p.kill();p.wait()
        self.runtime.cleanup()

def main():
    parser=argparse.ArgumentParser(description=__doc__);parser.add_argument('cli',type=Path);parser.add_argument('output',type=Path);parser.add_argument('--baseline',type=Path);a=parser.parse_args()
    a.cli=a.cli.resolve();a.output=a.output.resolve()
    if a.output.exists():raise FileExistsError(a.output)
    source=(Path(__file__).resolve().parents[3]/'demos/badge-live.js').read_text()
    client=Client(a.cli,a.baseline.resolve() if a.baseline else None);baseline=None
    try:
        if a.baseline:
            # Preserve the reference project's color policy, remove all artwork through public transactions.
            state=client.call('state')
            client.job(client.call('batch',request_id=str(uuid.uuid4()),revision=state['revision'],project_epoch=state['project_epoch'],label='Empty reference artwork',operations=[{'op':'remove','document':d['id']} for d in sorted(state['documents'],key=lambda d:{'fold.timeline.sequence':0,'fold.compositor.composite':1,'fold.motion.document':2}[d['type']])]))
        assert client.call('catalog',sdk=True)['version']==2
        assert client.call('catalog',guide='live-tools')
        result=client.build(source);handles=result['handles']
        frames=[client.render(handles['composite'],n,d) for n,d in [(0,1),(3,8),(3,4),(5,4),(5,1)]]
        assert len(set(frames))>1,'Animation must move'
        assert client.render(handles['sequence'],3,4)==frames[2],'Sequence must retain the composite output'
        if a.baseline:
            baseline=Client(a.cli,a.baseline.resolve())
            docs=baseline.call('state')['documents']
            composite=next(d['id'] for d in docs if d['type']=='fold.compositor.composite')
            reference=[baseline.render(composite,n,d) for n,d in [(0,1),(3,8),(3,4),(5,4),(5,1)]]
            if frames!=reference:
                debug=Path(tempfile.mkdtemp(prefix='fold-badge-parity-'))
                for i,(actual,expected) in enumerate(zip(frames,reference)):
                    (debug/f'actual-{i}.png').write_bytes(base64.b64decode(actual))
                    (debug/f'expected-{i}.png').write_bytes(base64.b64decode(expected))
                for label,c in [('actual',client),('expected',baseline)]:
                    for doc in c.call('state')['documents']:
                        if doc['type']=='fold.motion.document':
                            (debug/f'{label}.json').write_text(json.dumps(c.job(c.call('inspect',document=doc['id'])),indent=2))
                raise AssertionError(f'Badge differs from baseline frames; inspect {debug}')
        repeated=client.build(source);assert repeated['handles']==handles
        assert client.render(handles['composite'],3,4)==frames[2]
        state=client.call('state')
        client.job(client.call('save',request_id=str(uuid.uuid4()),revision=state['revision'],project_epoch=state['project_epoch'],path=str(a.output)))
        a.output.with_suffix('.png').write_bytes(base64.b64decode(frames[2]))
        client.close();client=Client(a.cli,a.output)
        repeated=client.build(source);assert repeated['handles']==handles
        assert client.render(handles['composite'],3,4)==frames[2]
        client.build("const d=plan.document('motion');d.badge('badge').text.set('text','Artist text');d.badge('badge').role('transform',{translation:{Value:datum([620,290])}}).properties({name:'My badge'});")
        client.build(source)
        client.build("const d=plan.document('motion');d.oscillator('bounce',d.duplicate('copies',d.badge('badge')),'offset_y').set('minimum',-20).set('maximum',20);")
        document=client.job(client.call('inspect',document=handles['motion']))['data']
        text=next(n for n in document['graph']['nodes'] if n['id']==handles['motion/badge/text'])
        assert text['inputs']['text']['Value']['Text']=='Artist text'
        badge=next(o for o in document['scene']['objects'] if o['id']==handles['motion/badge'])
        assert badge['name']=='My badge'
        transform=next(n for n in document['graph']['nodes'] if n['id']==badge['transform'])
        assert transform['inputs']['translation']['Value']['Vector']==[620,290]
        print(json.dumps({'passed':True,'saved':str(a.output),'frames':5,'baseline_pixel_match':bool(a.baseline),'reopen_and_repeat':True,'artist_edits_preserved':True,'calls':client.serial,'note':'Follow-up test edits stayed in an isolated unsaved session.'}))
    finally:
        client.close()
        if baseline:baseline.close()
if __name__=='__main__':main()
