"""Author the M5a mixed scene/compositor acceptance fixture through Fold v2.
Requires an empty running session with GPU and color runtime, plus Pillow.
Creates a foreground title scene, an explicit-input compositor, and a background 3D scene.
Never reads implementation code or edits a project payload directly.
"""
import json,subprocess,time,uuid
from pathlib import Path
import sys
if len(sys.argv) != 5:
 raise SystemExit("Usage: m5a-mixed-scene.py <fold-cli> <socket> <new-project.fold> <new-report.json>")
CLI, SOCKET, PROJECT, REPORT = sys.argv[1:]
PROJECT, REPORT = str(Path(PROJECT).absolute()), str(Path(REPORT).absolute())
if Path(PROJECT).exists() or Path(REPORT).exists():
 raise SystemExit("Project and report destinations must be unused")
inspections=[]
def request(command):
 deadline=time.monotonic()+120
 while True:
  p=subprocess.run([CLI,'protocol',SOCKET],input=json.dumps({'version':2,'command':command}),text=True,capture_output=True,check=True,timeout=30)
  v=json.loads(p.stdout)
  if 'error' in v: raise RuntimeError(v['error'])
  if v.get('result',{}).get('state')!='running': return v
  if time.monotonic()>deadline: raise TimeoutError('Public job deadline exceeded')
  command={'method':'job_status','handle':v['result']['job'],'wait_ms':1000}
def discover():return request({'method':'discover'})
def guard(scope):
 v=discover();return {'epoch':v['epoch'],'request_id':str(uuid.uuid4()),'base':v['result']['revision'],'scope':scope}
def inspect(document,object=None):
 v=discover();return request({'method':'inspect_document','epoch':v['epoch'],'revision':v['result']['revision'],'scope':{'document':document,'definition':None,'object':object}})['result']['source']
def edit(document,edits):
 scope={'document':document,'definition':None};g=guard(scope)
 proposal=request({'method':'propose','guard':g,'edit':{'kind':'project','edits':edits}})
 handle=proposal['result']['handle'];candidate=request({'method':'inspect_candidate','handle':handle})
 inspections.append(candidate)
 # Scope and complete semantic diff must be present before Apply.
 assert candidate['result']['scope']['document']==document and candidate['result']['changes']
 request({'method':'apply','guard':guard(scope),'handle':handle,'receipt':candidate['result']['receipt']})
 return candidate

import re
assert discover()['result']['documents']==[], 'Start a fresh empty project session'
c=str(uuid.uuid4())
edit(c,[{'op':'create','document':c,'type_id':'fold.composition.document','name':'M5a mixed scene acceptance'}])
record=json.loads(inspect(c))
edit(c,[{'op':'author','document':c,'expected':record,'edits':[{'op':'format','width':1920,'height':1080,'rate':[30,1],'frames':120}]},*[{'op':'add_layer','composition':c,'kind':kind} for kind in ['scene','image','scene']]])
record=json.loads(inspect(c)); all_layers=record['native']['layers']
scene_documents={d['id'] for d in discover()['result']['documents'] if d['type']=='fold.motion.native.scene'}
layers=[layer for layer in all_layers if layer['source']['Output']['document'] in scene_documents]
assert len(layers)==2 and len(all_layers)==3
catalog={(op['key'].split('.')[-1],op['mode']):op for op in next(p["requests"]["catalog"] for p in discover()['result']["scoped_proposals"] if p["type"] == "fold.motion.construction")['operations']}
dump=lambda v:json.dumps(v,separators=(',',':'))
def lit(kind,value,**kw):return {'kind':kind,'value':value,**kw}
def val(v):return 'value(literal('+dump(v)+'))'
def vector(v,unit='meters',space='world'):return lit('vector',v,unit=unit,space=space)
def col(v):return lit('color',v,encoding='linear_rec709',alpha='straight')
class Graph:
 def __init__(self,original,object):
  self.original=original;self.object=object;self.nodes=[];self.positions={};self.serial=0
 def uid(self,key):return str(uuid.uuid5(uuid.UUID(self.object),key))
 def meta(self,key):return '@'+dump({'id':self.uid(key)})
 def link(self,name):
  self.serial+=1;return 'link($'+dump(name)+' '+self.meta('edge/'+str(self.serial))+')'
 def add(self,name,key,mode,**values):
  op=catalog[key,mode];params=[]
  for key,spec in op['parameters'].items():
   v=values.get(key)
   if v is None:
    default=spec['default'];assert default and default['kind']=='literal',(name,key,default)
    v=val(default['value'])
   params.append('    '+dump(key)+' '+self.meta('parameter/'+name+'/'+key)+' = '+v+';')
  self.positions[self.uid('node/'+name)]=[len(self.nodes)*320,0]
  self.nodes.append('  let '+dump(name)+' '+self.meta('node/'+name)+' = '+dump(op['key'])+'.'+dump(mode)+' @'+str(op['version'])+' (\n'+'\n'.join(params)+'\n  );')
  return name
 def source(self,output,ty):
  original=self.original
  header=original[:original.index('asset ')]
  asset=re.search(r'^asset (.*) \{$',original,re.M)[1]
  out=re.search(r'^  output "main" (@\{[^\n]+?\}) :',original,re.M)[1]
  root=original[original.index('\nroot '):original.index('\npresentation ')]
  return header+'asset '+asset+' {\n'+'\n'.join(self.nodes)+'\n  output "main" '+out+' : '+dump({'kind':ty})+' = $'+dump(output)+' '+self.meta('output')+';\n}\n'+root+'\npresentation '+dump({'positions':self.positions,'groups':[],'annotations':{}})+';\nextensions {};\n'
edits=[];renames=[]
for i,(layer,name) in enumerate(zip(layers,['Title reveal','Background spheres'])):
 scene=layer['source']['Output']['document'];state=json.loads(inspect(scene))
 obj=next(o for o in state['native']['objects'] if o['name'] in ['Box',name]);oid=obj['id'];original=state['object_sources'][oid];g=Graph(original,oid)
 if i==0:
  text=g.add('Make it move','text','packaged',text=val(lit('text','Make it move')),size=val(lit('distance',210,unit='canvas')))
  outline=g.add('Outline','outline_text','curves',text=g.link(text))
  solid=g.add('Extrude','extrude','straight_text',profile=g.link(outline),depth=val(lit('distance',.04,unit='meters')),canvas_unit=val(lit('distance',.004,unit='meters')))
  ink=g.add('Ivory','material','unlit',geometry=g.link(solid),color=val(col([.92,.86,.72,1])))
  output=g.add('Title position','place','world',geometry=g.link(ink),position=val(vector([-2.65,1.45,0])));ty='placement'
 elif i==1:
  sphere=g.add('Sphere','mesh','sphere',radius=val(lit('distance',.18,unit='meters')))
  grid=g.add('Points','distribute','grid',columns=val(lit('integer',11)),rows=val(lit('integer',4)),spacing=val(vector([.57,.53])),center=val(vector([0,-.9,0])))
  copies=g.add('Copy to Points','cloner','points',prototype=g.link(sphere),points=g.link(grid))
  random=g.add('Random colors','random','uniform')
  colors=[[1,.07,.025,1],[1,.42,.035,1],[.05,.55,.36,1],[.06,.16,.95,1],[.42,.06,.9,1],[.96,.19,.5,1]]
  ramp={'id':g.uid('ramp'),'ty':{'kind':'color','encoding':'linear_rec709','alpha':'straight'},'stops':[{'id':g.uid('stop/'+str(j)),'position':j/(len(colors)-1),'value':col(color)} for j,color in enumerate(colors)]}
  palette=g.add('Palette','ramp','color_linear',position=g.link(random),stops='ramp('+dump(ramp)+')')
  colored=g.add('Color copies','animator','color_replace',geometry=g.link(copies),color=g.link(palette))
  output=g.add('Soft material','material','lit',geometry=g.link(colored),roughness=val(lit('scalar',.3)));ty='geometry'
  # The three independent branches converge at Copy to Points and Color copies.
  for n,xy in [('Sphere',[0,0]),('Points',[0,270]),('Copy to Points',[340,0]),('Random colors',[0,540]),('Palette',[340,540]),('Color copies',[680,0]),('Soft material',[1020,0])]:g.positions[g.uid('node/'+n)]=xy
 else:
  shape=g.add('Backdrop','mesh','box',size=val(vector([9,6,.1])))
  output=g.add('Midnight','material','unlit',geometry=g.link(shape),color=val(col([.018,.013,.035,1])));ty='geometry'
 source=g.source(output,ty)
 edits.append({'op':'object_source','document':scene,'object':oid,'expected':original,'source':source})
 camera=next(o for o in state['native']['objects'] if o['name']=='Camera');old=state['object_sources'][camera['id']];cg=Graph(old,camera['id'])
 camera_node=cg.add('Camera','camera','orthographic',position=val(vector([0,0,6])),vertical_size=val(lit('distance',4.2,unit='meters')))
 edits.append({'op':'object_source','document':scene,'object':camera['id'],'expected':old,'source':cg.source(camera_node,'camera')})
 # Renaming is a separate candidate because it uses the complete expected scene.
 renames.append((scene,oid,name))
edit(c,edits)
# A separate plate behind the spheres makes depth and object ownership visible.
background_scene=layers[1]['source']['Output']['document']
state=json.loads(inspect(background_scene));before_objects={o['id'] for o in state['native']['objects']}
edit(background_scene,[{'op':'hierarchy','document':background_scene,'expected':state,'edits':[{'op':'create','kind':'box'}]}])
state=json.loads(inspect(background_scene));plate=next(o for o in state['native']['objects'] if o['id'] not in before_objects)
original=state['object_sources'][plate['id']];g=Graph(original,plate['id'])
shape=g.add('Background plate','mesh','box',size=val(vector([9,6,.1])))
ink=g.add('Midnight','material','unlit',geometry=g.link(shape),color=val(col([.018,.013,.035,1])))
output=g.add('Behind spheres','place','world',geometry=g.link(ink),position=val(vector([0,0,-1])))
edit(background_scene,[{'op':'object_source','document':background_scene,'object':plate['id'],'expected':original,'source':g.source(output,'placement')}])
renames.append((background_scene,plate['id'],'Background plate'))
for scene,oid,name in renames:
 edit(scene,[{'op':'hierarchy','document':scene,'expected':json.loads(inspect(scene)),'edits':[{'op':'rename','object':oid,'name':name}]}])
record=json.loads(inspect(c));ops=[{'op':'layer','layer':layer['id'],'name':name} for layer,name in zip(layers,['Title reveal','Background spheres'])]
for numerator,denominator,opacity,y in [(0,1,0,65),(4,5,1,0)]:
 for channel,value in [('opacity',opacity),('position_y',y)]:ops.append({'op':'keyframe','layer':layers[0]['id'],'channel':channel,'time':{'numerator':numerator,'denominator':denominator},'value':value})
edit(c,[{'op':'author','document':c,'expected':record,'edits':ops}])
# Bind the middle Compositor explicitly to the background Scene output.
image_layer=next(layer for layer in all_layers if layer not in layers)
image_document=image_layer['source']['Output']['document']
image_state=json.loads(inspect(image_document))
port=image_state['image_inputs'][0]['id']
edit(c,[{'op':'connect_layers','composition':c,'source':layers[1]['id'],'target':image_layer['id'],'port':port}])
image_state=json.loads(inspect(image_document))
provider=next(p for p in discover()['result']['project_authoring']['providers'] if p['type']=='fold.compositor.composite')
grade=next(op['record'] for op in provider['catalog']['operators'] if op['id']=='Grade' and op['version']==3)
grade['id']=str(uuid.uuid4());grade['name']='Cool background'
for parameter in grade['parameters'].values():parameter['id']=str(uuid.uuid4())
grade['parameters']['settings']['value']['value']['gain']=[.7,.9,1.1]
edit(image_document,[{'op':'author','document':image_document,'expected':image_state,'edits':[
 {'op':'create_node','id':grade['id'],'record':grade},
 {'op':'insert','node':image_state['native']['output'],'input':'image','modifier':grade['id'],'modifier_input':'image'}]}])
record=json.loads(inspect(c))
edit(c,[{'op':'author','document':c,'expected':record,'edits':[{'op':'layer','layer':image_layer['id'],'name':'Explicit background grade'}]}])
# Round-trip and edit a native scoped source through the guarded project protocol.
image_state=json.loads(inspect(image_document));source_nodes=json.loads(image_state['graph_source'])
selected=next(node for node in source_nodes if node['operation']['id']==grade['id'])
# Canonical scoped text is the selected-node projection of the complete source.
expected=json.dumps([selected],indent=2)
selected['operation']['name']='Background grade'
edit(image_document,[{'op':'compositor_source','document':image_document,'nodes':[grade['id']],'expected':expected,'source':json.dumps([selected],indent=2)}])
# Check retained hierarchy and actual explicit inputs before observing/saving.
image_state=json.loads(inspect(image_document))
assert image_state['image_inputs'][0]['source']['Output']['document']==layers[1]['source']['Output']['document']
for layer in layers:
 state=json.loads(inspect(layer['source']['Output']['document']))
 assert len(state['native']['objects'])>=3 and len(state['object_sources'])>=3
v=discover(); observations=[]
from PIL import Image
for num,den,name in [(0,1,'start'),(2,5,'reveal'),(1,1,'held')]:
 result=request({'method':'observe_frame','target':{'kind':'committed','epoch':v['epoch'],'revision':v['result']['revision'],'document':c},'output':'video','time':{'numerator':num,'denominator':den},'dimensions':[960,540]})['result']
 image=result.pop('image'); path=REPORT+'.'+name+'.png'
 Image.frombytes('RGBA',tuple(image['dimensions']),bytes.fromhex(image['data'])).save(path)
 observations.append({'file':path,**result})
start_pixels=Image.open(REPORT+'.start.png').convert('RGBA').crop((0,230,960,540)).tobytes()
held_pixels=Image.open(REPORT+'.held.png').convert('RGBA').crop((0,230,960,540)).tobytes()
assert start_pixels==held_pixels,'Static background changed under the title reveal'
saved=request({'method':'save_project_copy','guard':{'epoch':v['epoch'],'request_id':str(uuid.uuid4()),'base':v['result']['revision']},'path':PROJECT})
Path(REPORT).write_text(json.dumps({'composition':c,'inspections':inspections,'observations':observations,'saved':saved},indent=2))
print('Saved',PROJECT,'at revision',v['result']['revision'])
