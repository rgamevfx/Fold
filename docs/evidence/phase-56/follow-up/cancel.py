"""Public cancellation against a live rustc child; wrapper only records PID."""
import json, os, shutil, sys, tempfile, time, uuid
from pathlib import Path
ROOT = Path(__file__).resolve().parents[4]
sys.path.insert(0, str(ROOT/'skills/fold/scripts'))
from live_badge import Client
out = Path(__file__).parent
with tempfile.TemporaryDirectory(prefix='fold-compiler-cancel-') as tmp:
    marker = Path(tmp)/'pid'
    wrapper = Path(tmp)/'rustc'
    real = shutil.which('rustc')
    wrapper.write_text('#!/usr/bin/env python3\nimport os,sys\nfrom pathlib import Path\nPath('+repr(str(marker))+').write_text(str(os.getpid()))\nos.execv('+repr(real)+', ['+repr(real)+']+sys.argv[1:])\n')
    wrapper.chmod(0o700)
    os.environ['FOLD_RUSTC'] = str(wrapper)
    c = Client(ROOT/'target/release/fold-cli')
    log=[]
    def call(method, **args):
        start=time.monotonic(); r=c.call(method, **args)
        log.append({'method':method,'arguments':args,'result':r,'elapsed':time.monotonic()-start})
        return r
    try:
        before=call('state')
        source='let mut a=Asset::new("Cancellation probe"); let c=a.circle("dot",17.); a.output(&c); a'
        batch=dict(request_id=str(uuid.uuid4()),revision=before['revision'],project_epoch=before['project_epoch'],label='Cancellation probe',ids=['composition','graph','definition','instance','layer'],operations=[
            {'op':'create','id':{'$ref':'composition'},'kind':'fold.composition.document','name':'Probe'},
            {'op':'command','id':'fold.app.create-rust-asset','arguments':{**{k:{'$ref':k} for k in ['composition','graph','definition','instance','layer']},'source':source}}])
        job=call('batch',**batch)
        deadline=time.monotonic()+10
        while not marker.exists() and time.monotonic()<deadline: time.sleep(.005)
        assert marker.exists(), 'Compiler did not start'
        pid=int(marker.read_text())
        # Wait until exec has replaced the observation wrapper with actual rustc.
        while time.monotonic()<deadline:
            executable=os.readlink(f'/proc/{pid}/exe')
            if Path(executable).name=='rustc':break
            time.sleep(.001)
        assert Path(executable).name=='rustc', executable
        responsive=call('state')
        assert responsive['revision']==before['revision']
        call('cancel',job=job['job'])
        result=call('jobs',job=job['job'],wait_ms=25000)
        assert result['status']=='complete' and not result['outcome']['ok'], result
        after=call('state')
        assert after['documents']==before['documents'] and after['revision']==before['revision']
        assert not Path(f'/proc/{pid}').exists(), 'Compiler child still alive'
        batch['request_id']=str(uuid.uuid4())
        accepted=call('batch',**batch)
        recovery=call('jobs',job=accepted['job'],wait_ms=25000)
        assert recovery['status']=='complete' and recovery['outcome']['ok'], recovery
        (out/'cancel-result.json').write_text(json.dumps({'compiler_executable':executable,'cancelled':result,'unchanged_documents':True,'child_reaped':True,'subsequent_compile_succeeded':True,'calls':log},indent=2))
        print('Live rustc cancellation, writer responsiveness, child reap, atomicity and subsequent compile passed')
    finally:c.close()
