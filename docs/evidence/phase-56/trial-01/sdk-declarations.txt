export type Identity = string | {$ref: string};
export type Literal = number | string | boolean | [number,number] | [number,number,number,number];
export type Datum = {Scalar:number} | {Text:string} | {Bool:boolean} | {Vector:[number,number]} | {Color:[number,number,number,number]};
export type Binding = {Value:Datum} | {Link:{node:Identity,socket:string}};
export type Operation = Record<string,unknown>;
export type Batch = {request_id:string,label:string,revision:number,project_epoch:number,ids:string[],operations:Operation[]};
export function datum(value:Literal):Datum;
export class FoldPlan {
  constructor(state:{revision:number,project_epoch:number,handles?:Record<string,string>,namespace?:string,bindings?:Record<string,{id:string,signature:string}>});
  identity(key:string):Identity;
  document(key:string,kind?:string,name?:string):Document;
  composite(key:string,source:Document,info:{width:number,height:number,frames:number,rate:[number,number]},name?:string):Document;
  sequence(key:string,source:Document,name?:string):Document;
  existingDocument(id:string):Document;
  operation(operation:Operation):this;
  batch(request_id:string,label:string):Batch;
}
export class Document {
  readonly id:Identity;
  initialize(build:(document:Document)=>void):this;
  badge(key:string,options?:{name?:string,text?:string,size?:number,padding?:[number,number],radius?:number,position?:[number,number]}):Node & {text:Node,background:Node};
  duplicate(key:string,source:Node,defaults?:Record<string,Literal>):Node;
  group(key:string,name?:string):Node;
  oscillator(key:string,target:Node,input:string,defaults?:Record<string,Literal>):Node;
  colorRamp(key:string,target:Node,input?:string,stops?:[number,[number,number,number,number]][],interpolation?:"Linear"|"Smooth"|"Stepped"):Node;
  style(key:string,object:Node,kind:string,defaults?:Record<string,Literal>):Node;
  edit(...edits:Operation[]):this;
  node(key:string,kind:string,options?:{settings?:Record<string,unknown>,inputs?:Record<string,Binding>}):Node;
  object(key:string,kind:string,name?:string):Node;
  existingNode(id:string):Node;
  text(key:string,text:string,options?:{name?:string,size?:number,position?:[number,number]}):Node;
  expression(key:string,expression:Expression,inputs?:Record<string,Datum>):Node;
  control(key:string,name:string,value:Literal):Node;
  info(value:Record<string,unknown>):this;
}
export class Node {
  readonly id:Identity;
  initialize(build:(node:Node)=>void):this;
  parent(group:Node|null,time?:{numerator:number,denominator:number}):this;
  reference(input:string,object:Node,local?:boolean):this;
  ramp(stops:[number,[number,number,number,number]][],interpolation?:"Linear"|"Smooth"|"Stepped"):this;
  set(input:string,value:Literal):this;
  connect(input:string,source:Node,output?:string):this;
  role(role:'source'|'transform'|'appearance',inputs:Record<string,Binding>):this;
  properties(value:Record<string,unknown>):this;
  settings(settings:Record<string,unknown>):this;
  link(socket?:string):Binding;
}

export type Unary = 'sin'|'cos'|'floor'|'abs'|'negate';
export type Binary = 'add'|'subtract'|'multiply'|'divide'|'modulo'|'min'|'max';
export type Expression = {op:'literal',value:Datum}|{op:'input',name:string}|{op:'time'|'index'|'count'|'position'}|{op:'unary',operation:Unary,value:Expression}|{op:'binary',operation:Binary,a:Expression,b:Expression}|{op:'vector',x:Expression,y:Expression};
export const expr: {value(value:Literal):Expression;input(name:string):Expression;time:Expression;index:Expression;count:Expression;position:Expression;unary(operation:Unary,value:Expression):Expression;binary(operation:Binary,a:Expression,b:Expression):Expression;vector(x:Expression,y:Expression):Expression};
