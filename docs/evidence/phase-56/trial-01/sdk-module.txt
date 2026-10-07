/** Shared SDK for bounded Fold author requests and external native batch clients. */
export function datum(value) {
  if (typeof value === 'string') return {Text: value};
  if (typeof value === 'boolean') return {Bool: value};
  if (typeof value === 'number' && Number.isFinite(value)) return {Scalar: value};
  if (Array.isArray(value) && [2,4].includes(value.length) && value.every(v => typeof v === 'number' && Number.isFinite(v)))
    return {[value.length === 2 ? 'Vector' : 'Color']: [...value]};
  throw new Error('Expected finite scalar, boolean, text, vector[2] or color[4]');
}
export class FoldPlan {
  constructor({revision, project_epoch, handles = {}, bindings = {}, namespace = 'default'}) {
    this.revision = revision;
    this.project_epoch = project_epoch;
    this.bindings = bindings;
    this.namespace = namespace;
    this.remembered = new Map();
    this.handles = new Map(Object.entries(bindings).map(([k,v])=>[k,v.id]));
    for (const [key,id] of Object.entries(handles)) {
      if (this.handles.has(key) && this.handles.get(key)!==id) throw Error(`Handle collision: ${key}`);
      this.handles.set(key,id);
    }
    this.ids = new Set();
    this.operations = [];
    this.declarations = new Map();
  }
  identity(key) {
    if (typeof key !== 'string' || key.length === 0 || key.length > 80) throw new Error('Identity key must be 1..80 characters');
    if (this.handles.has(key)) return this.handles.get(key);
    this.ids.add(key);
    return {$ref: key};
  }
  declare(key, signature) {
    const previous = this.declarations.get(key) ?? this.bindings[key]?.signature;
    if (previous && previous !== signature) throw new Error(`Conflicting declaration: ${key}`);
    this.declarations.set(key, signature);
    return !previous && !this.handles.has(key);
  }
  document(key, kind = 'fold.motion.document', name = key) {
    const id = this.identity(key);
    const fresh=this.declare(key, `document:${kind}`);
    if (fresh) this.operation({op:'create', id, kind, name});
    this.remember(id,key,`document:${kind}`,id);
    return new Document(this, id, key, fresh);
  }
  remember(document,key,signature,id) {
    const owner=JSON.stringify(document);
    if (!this.remembered.has(owner)) this.remembered.set(owner,{document,bindings:{}});
    this.remembered.get(owner).bindings[key]={id,signature};
  }
  composite(key,source,info,name=key) {
    const doc=this.document(key,'fold.compositor.composite',name);
    doc.initialize(d=>{
      d.info(info);
      const read=this.identity(`${key}/read`),out=this.identity(`${key}/output`);
      d.edit({op:'add',id:read,parameters:{Read:{source:{Document:{source:{document:source.id,output:'video'},info}},start:{numerator:0,denominator:1},source_start:{numerator:0,denominator:1},duration:{numerator:info.frames*info.rate[1],denominator:info.rate[0]}}}},
        {op:'add',id:out,parameters:'Output'}, {op:'connect',source:read,node:out,input:'image'}, {op:'output',node:out},{op:'layout'});
      this.remember(d.id,`${key}/read`,'compositor:Read',read);this.remember(d.id,`${key}/output`,'compositor:Output',out);
    });return doc;
  }
  sequence(key,source,name=key) {
    const doc=this.document(key,'fold.timeline.sequence',name);
    doc.initialize(d=>this.operation({op:'place',source:source.id,target:d.id,at:{numerator:0,denominator:1}}));
    return doc;
  }
  existingDocument(id) { return new Document(this, id, id); }
  operation(operation) { this.operations.push(structuredClone(operation)); return this; }
  batch(request_id, label) {
    const operations=[...this.operations,...[...this.remembered.values()].map(v=>({op:'remember',namespace:this.namespace,...v}))];
    if (!operations.length || operations.length > 128 || this.ids.size > 512) throw new Error('Batch requires 1..128 operations and at most 512 allocated identities');
    return structuredClone({request_id, label, revision:this.revision, project_epoch:this.project_epoch, ids:[...this.ids], operations});
  }
}
export class Document {
  constructor(plan,id,key,fresh=false) { this.plan=plan; this.id=id; this.key=key; this.fresh=fresh; }
  initialize(build) { if (this.fresh) build(this); return this; }
  edit(...edits) {
    // Adjacent edits share one package transaction, retaining forward-link support.
    const last=this.plan.operations.at(-1);
    if (last?.op==='edit' && JSON.stringify(last.document)===JSON.stringify(this.id)) last.edits.push(...structuredClone(edits));
    else this.plan.operation({op:'edit',document:this.id,edits});
    return this;
  }
  node(key,kind,{settings={},inputs={}}={}) {
    const identity=`${this.key}/${key}`;
    const id=this.plan.identity(identity);
    const fresh=this.plan.declare(identity,`node:${kind}`);
    if (fresh) this.edit({op:'create_node',id,kind,settings,inputs});
    this.plan.remember(this.id,identity,`node:${kind}`,id);
    return new Node(this,id,fresh);
  }
  object(key,kind,name=key) {
    const identity=`${this.key}/${key}`;
    const id=this.plan.identity(identity);
    const fresh=this.plan.declare(identity,`object:${kind}`);
    if (fresh) this.edit({op:'object',id,kind,name});
    this.plan.remember(this.id,identity,`object:${kind}`,id);
    return new Node(this,id,fresh);
  }
  existingNode(id) { return new Node(this,id); }
  text(key,text,options={}) {
    const node=this.object(key,'fold.motion.text',options.name??key);
    node.initialize(n=>{ n.set('text',text);
      if ('size' in options) n.set('size',options.size);
      if ('position' in options) n.role('transform',{translation:{Value:datum(options.position)}});
    });
    return node;
  }
  expression(key,expression,inputs={}) { return this.node(key,'fold.motion.expression',{settings:{expression,inputs}}); }
  control(key,name,value) {
    const tagged=datum(value),identity=`${this.key}/${key}/control`,id=this.plan.identity(identity);
    const node=this.node(key,'fold.motion.published_control',{settings:{id,name,value_type:Object.keys(tagged)[0]},inputs:{default:{Value:tagged}}});
    this.plan.remember(this.id,identity,'published-control',id);
    return node;
  }

  tool(key,kind,operation) {
    const identity=`${this.key}/${key}`, id=this.plan.identity(identity);
    const fresh=this.plan.declare(identity,`tool:${kind}`);
    if(fresh) this.edit({...operation,id});
    this.plan.remember(this.id,identity,`tool:${kind}`,id);
    return new Node(this,id,fresh);
  }
  badge(key,{name=key,text='Hi Fold',size=24,padding=[24,12],radius=24,position}={}) {
    const textKey=`${this.key}/${key}/text`, bgKey=`${this.key}/${key}/background`;
    const textId=this.plan.identity(textKey), bgId=this.plan.identity(bgKey);
    const badge=this.tool(key,'badge',{op:'badge',text:textId,background:bgId,name});
    this.plan.remember(this.id,textKey,'object:fold.motion.text',textId);
    this.plan.remember(this.id,bgKey,'object:fold.motion.fit_background',bgId);
    badge.text=new Node(this,textId,badge.fresh); badge.background=new Node(this,bgId,badge.fresh);
    badge.initialize(n=>{
      n.text.set('text',text).set('size',size);
      n.background.set('padding',padding).set('radius',radius);
      if(position) n.role('transform',{translation:{Value:datum(position)}});
    });
    return badge;
  }
  duplicate(key,source,{name=key,...inputs}={}) {
    return this.tool(key,'duplicate',{op:'duplicator',source:source.id,name})
      .initialize(n=>{for(const [k,v] of Object.entries(inputs)) n.set(k,v);});
  }
  group(key,name=key) { return this.tool(key,'group',{op:'scene_group',name}); }
  oscillator(key,target,input,defaults={}) {
    return this.tool(key,'oscillator',{op:'driver',target:target.id,input,kind:'oscillator'})
      .initialize(n=>{for(const [k,v] of Object.entries(defaults)) n.set(k,v);});
  }
  colorRamp(key,target,input='color',stops=[[0,[0,0,0,1]],[1,[1,1,1,1]]],interpolation='Linear') {
    return this.tool(key,'color_ramp',{op:'driver',target:target.id,input,kind:'color_ramp'})
      .initialize(n=>n.ramp(stops,interpolation));
  }
  style(key,object,kind,defaults={}) {
    return this.tool(key,`style:${kind}`,{op:'style',object:object.id,kind})
      .initialize(n=>{for(const [k,v] of Object.entries(defaults)) n.set(k,v);});
  }
  info(value) { return this.edit({op:'info',value}); }
}
export class Node {
  constructor(document,id,fresh=false) { this.document=document; this.id=id; this.fresh=fresh; }
  initialize(build) { if(this.fresh) build(this); return this; }
  parent(group,time={numerator:0,denominator:1}) {this.document.edit({op:'parent',object:this.id,parent:group?.id??null,time});return this;}
  reference(input,object,local=true) {this.document.edit({op:'reference',node:this.id,input,object:object.id,local});return this;}
  ramp(stops,interpolation='Linear') {this.document.edit({op:'ramp',node:this.id,settings:{stops:stops.map(([position,color])=>({position,value:datum(color)})),interpolation}});return this;}
  set(input,value) { this.document.edit({op:'set',node:this.id,input,value:datum(value)}); return this; }
  connect(input,source,output='value') { this.document.edit({op:'connect',node:this.id,input,source:{node:source.id,socket:output}}); return this; }
  role(role,inputs) { this.document.edit({op:'object_inputs',object:this.id,role,inputs});return this; }
  properties(value) { this.document.edit({op:'object_properties',object:this.id,value});return this; }
  settings(settings) { this.document.edit({op:'node',node:this.id,value:{settings}}); return this; }
  link(socket='value') { return {Link:{node:this.id,socket}}; }
}

export const expr = {
  value:value=>({op:'literal',value:datum(value)}),
  input:name=>({op:'input',name}),
  time:{op:'time'}, index:{op:'index'}, count:{op:'count'}, position:{op:'position'},
  unary:(operation,value)=>({op:'unary',operation,value}),
  binary:(operation,a,b)=>({op:'binary',operation,a,b}),
  vector:(x,y)=>({op:'vector',x,y}),
};
