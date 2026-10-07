"""Release public-MCP probe; isolated server and disposable export destination."""
import json, sys, tempfile, uuid
from pathlib import Path
ROOT=Path(__file__).resolve().parents[4]
sys.path.insert(0,str(ROOT/'skills/fold/scripts'))
from live_badge import Client
out=Path(__file__).parent
c=Client(ROOT/'target/release/fold-cli')
log=[]
def call(tool_name,**args):
    r=c.call(tool_name,**args);log.append({'method':tool_name,'arguments':args,'result':r});return r
def job(method,**args):
    accepted=call(method,**args)
    while True:
        r=call('jobs',job=accepted['job'],wait_ms=25000)
        if r['status']=='complete':
            assert r['outcome']['ok'],r
            return r['outcome']['result']
def identity():
    s=call('state');return dict(revision=s['revision'],project_epoch=s['project_epoch'],request_id=str(uuid.uuid4()))
source='''let mut a=Asset::new("Desktop timing probe");
let radius=a.scalar_range("radius","Radius",12.,"px",[1.,30.]);
a.describe("radius","Geometry",false,"Size of each circle");
let c=a.circle("circle",radius);
let c=a.fill("paint",&c,[0.8,0.1,0.4,1.]);
let p=a.grid("grid",50.,100.,[35.,35.],[0.,0.]);
let c=a.copies("copies",&c,&p);
let c=a.fit("fit",&c,[100.,50.],[300.,750.]);
let c=a.translate("move",&c,vec2(phase(2.)*100.,0.));
a.output(&c);a'''
try:
    assert call('catalog',method='asset')
    result=job('batch',**identity(),label='Public SDK completion probe',ids=['composition','graph','definition','instance','layer'],operations=[
        {'op':'create','id':{'$ref':'composition'},'kind':'fold.composition.document','name':'Desktop probe'},
        {'op':'edit','document':{'$ref':'composition'},'edits':[{'op':'format','width':500,'height':888,'rate':[25,1],'frames':150}]},
        {'op':'command','id':'fold.app.create-rust-asset','arguments':{**{k:{'$ref':k} for k in ['composition','graph','definition','instance','layer']},'source':source}}])
    ids=result['ids']
    q={'mode':'verify','check':'control_response','control':ids['instance'],'input':'radius','values':[{'Scalar':8.},{'Scalar':12.}],'target':{'node':ids['instance'],'output':'content'},'times':[{'numerator':0,'denominator':1},{'numerator':1,'denominator':1}]}
    evidence=job('inspect',document=ids['graph'],query=q)['data']
    assert evidence['status']=='pass',evidence
    fp=evidence['samples'][0]['variants'][0]['fingerprint']
    assert fp['complete'] and fp['elements']>5000
    with tempfile.TemporaryDirectory(prefix='fold-bundle-release-') as temp:
        path=str(Path(temp)/'probe.foldasset')
        job('asset',**identity(),action='export',document=ids['graph'],definition=ids['definition'],path=path)
        new={key:str(uuid.uuid4()) for key in ['graph','instance','layer']}
        before=call('state')
        job('asset',**identity(),action='import',composition=ids['composition'],path=path,**new)
        call('undo',**identity())
        assert call('state')['documents']==before['documents']
    # Keep a small source-edit fixture; the large construction above remains in evidence.
    small=source.replace('50.,100.,[35.,35.],[0.,0.]','4.,8.,[35.,35.],[0.,0.]')
    job('batch',**identity(),label='Desktop fixture',operations=[{'op':'edit','document':ids['graph'],'edits':[{'op':'rust_asset','definition':ids['definition'],'source':small}]}])
    dest=Path('/tmp/fold-phase56-desktop/probe.fold')
    job('save',**identity(),path=str(dest))
    (out/'public-result.json').write_text(json.dumps({'large_asset_fingerprint':fp,'control_response':'pass','bundle_import_undo':'pass','desktop_fixture':str(dest),'ids':ids},indent=2))
    print(json.dumps({'large_asset':fp,'bundle_import_undo':'pass'}))
finally:
    (out/'public-transcript.json').write_text(json.dumps(log,indent=2));c.close()
