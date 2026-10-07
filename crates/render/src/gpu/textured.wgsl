struct Params { matrix: vec4<f32>, translate: vec4<f32>, response: vec4<f32>, mapping: vec4<f32>, atlas: vec4<f32>, mode: vec4<u32> }
@group(0) @binding(0) var source: texture_2d_array<f32>;
@group(0) @binding(1) var driver: texture_2d_array<f32>;
struct Instance { matrix:vec4<f32>, offset:vec4<f32> }
@group(0) @binding(3) var<storage,read> instances:array<Instance>;
@group(0) @binding(2) var<uniform> p: Params;
@group(0) @binding(4) var<storage,read_write> status:atomic<u32>;
fn texel(tex: texture_2d_array<f32>, at:vec2<i32>, layer:i32, level:i32, encoded:bool) -> vec4<f32> {
    let c=textureLoad(tex,at,layer,level);
    if encoded {
        let rgb=select(c.rgb/12.92,pow((c.rgb+vec3(0.055))/1.055,vec3(2.4)),c.rgb>vec3(0.04045));
        return vec4(rgb,c.a);
    }
    return c;
}
fn bilinear(tex: texture_2d_array<f32>, uv: vec2<f32>, layer:i32, level:i32, encoded:bool) -> vec4<f32> {
    let dims=vec2<i32>(textureDimensions(tex,level));
    let q=clamp(uv,vec2(0.),vec2(1.))*vec2<f32>(dims)-vec2(0.5);
    let lo=vec2<i32>(floor(q)); let f=fract(q); let end=dims-vec2(1);
    let a=texel(tex,clamp(lo,vec2(0),end),layer,level,encoded);
    let b=texel(tex,clamp(lo+vec2(1,0),vec2(0),end),layer,level,encoded);
    let c=texel(tex,clamp(lo+vec2(0,1),vec2(0),end),layer,level,encoded);
    let d=texel(tex,clamp(lo+vec2(1,1),vec2(0),end),layer,level,encoded);
    return mix(mix(a,b,f.x),mix(c,d,f.x),f.y);
}
struct VertexOut { @builtin(position) position:vec4<f32>, @location(0) uv:vec2<f32>, @location(1) tint:vec4<f32>, @location(2) raw_position:vec2<f32>, @location(3) @interpolate(flat) layer:i32 }
struct InputVertex {position:vec2<f32>,uv:vec2<f32>,sample_uv:vec2<f32>,center:vec2<f32>}
@group(0) @binding(5) var<storage,read> vertices:array<InputVertex>;
fn evaluate(instance_index:u32,position:vec2<f32>,texcoord:vec2<f32>,sample_uv:vec2<f32>,center:vec2<f32>)->VertexOut {
    let instance=instances[instance_index];
    var sample_position=sample_uv;
    if p.mode.z!=0u {
        let local=vec2(dot(instance.matrix.xz,center),dot(instance.matrix.yw,center))+instance.offset.xy;
        sample_position=(vec2(dot(p.matrix.xz,local),dot(p.matrix.yw,local))+p.translate.xy)/p.translate.zw;
    }
    let layer=i32(instance.offset.w);
    let c=bilinear(driver,sample_position,min(layer,i32(textureNumLayers(driver))-1),0,(p.mode.w&2u)!=0u);
    var rgb=vec3(0.); if c.a>1e-8 {rgb=c.rgb/c.a;}
    var weights=vec3(0.2126,0.7152,0.0722); if p.mode.y!=0u {weights=vec3(0.27222872,0.67408174,0.053689517);}
    var value=dot(rgb,weights);
    switch p.mode.x {case 1u:{value=rgb.r;} case 2u:{value=rgb.g;} case 3u:{value=rgb.b;} case 4u:{value=c.a;} default:{}}
    value=value*p.mapping.x+p.mapping.y;
    let delta=(position-center)*(1.+value*p.response.z);
    let angle=value*p.atlas.w*0.017453292519943295;
    let before=center+vec2(cos(angle)*delta.x-sin(angle)*delta.y,sin(angle)*delta.x+cos(angle)*delta.y)+value*p.response.xy;
    let local=vec2(dot(instance.matrix.xz,before),dot(instance.matrix.yw,before))+instance.offset.xy;
    let raw_pos=vec2(dot(p.matrix.xz,local),dot(p.matrix.yw,local))+p.translate.xy;
    let pos=floor(raw_pos*16.+vec2(0.5))/16.;
    var out:VertexOut; out.position=vec4(pos.x/p.translate.z*2.-1.,1.-pos.y/p.translate.w*2.,0.,1.);
    out.raw_position=raw_pos;out.layer=layer;
    out.uv=texcoord;
    if p.atlas.x>0. {let glyph=floor(clamp(value,0.,1.)*(p.atlas.x-1.)+0.5);out.uv=vec2((glyph+texcoord.x)*p.atlas.y,texcoord.y*p.atlas.z);}
    out.tint=vec4(mix(vec3(1.),rgb,p.response.w),mix(1.,clamp(value,0.,1.),p.mapping.z)*p.mapping.w*instance.offset.z);
    return out;
}
@fragment fn fragment(v:VertexOut) -> @location(0) vec4<f32> {
    let dims=vec2<f32>(textureDimensions(source,0));
    let lod=clamp(log2(max(max(length(dpdx(v.uv)*dims),length(dpdy(v.uv)*dims)),1.)),0.,f32(textureNumLevels(source)-1u));
    let c=mix(bilinear(source,v.uv,v.layer,i32(floor(lod)),(p.mode.w&1u)!=0u),bilinear(source,v.uv,v.layer,i32(ceil(lod)),(p.mode.w&1u)!=0u),fract(lod));
    return vec4(c.rgb*v.tint.rgb,c.a)*v.tint.a;
}

@vertex fn vertex(@builtin(instance_index) instance_index:u32,@location(0) position:vec2<f32>,@location(1) uv:vec2<f32>,@location(2) sample_uv:vec2<f32>,@location(3) center:vec2<f32>)->VertexOut{
    return evaluate(instance_index,position,uv,sample_uv,center);
}
@compute @workgroup_size(64) fn validate(@builtin(global_invocation_id) id:vec3<u32>){
    let count=arrayLength(&vertices);if id.x>=count*arrayLength(&instances){return;}
    let v=vertices[id.x%count];let result=evaluate(id.x/count,v.position,v.uv,v.sample_uv,v.center);
    if any(result.raw_position!=result.raw_position)||any(abs(result.raw_position)>vec2(1e9)){atomicStore(&status,1u);}
}
