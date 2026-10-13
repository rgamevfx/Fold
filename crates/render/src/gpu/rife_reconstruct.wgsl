struct Params {size:vec4<u32>,control:vec4<f32>};
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var first:texture_2d<f32>;
@group(0) @binding(2) var second:texture_2d<f32>;
@group(0) @binding(3) var<storage,read> flow:array<f32>;
@group(0) @binding(4) var<storage,read> mask:array<f32>;
@group(0) @binding(5) var output:texture_storage_2d<rgba32float,write>;
@group(0) @binding(6) var<storage,read_write> status:atomic<u32>;
fn vector(q:vec2<i32>)->vec4<f32>{let c=vec2<u32>(clamp(q,vec2<i32>(0),vec2<i32>((p.size.xy+u32(p.control.z)-1)/u32(p.control.z))-1));let i=c.y*p.size.z+c.x;let plane=p.size.z*p.size.w;return vec4<f32>(flow[i],flow[plane+i],flow[2*plane+i],flow[3*plane+i]);}
fn weight(q:vec2<i32>)->f32{let c=vec2<u32>(clamp(q,vec2<i32>(0),vec2<i32>((p.size.xy+u32(p.control.z)-1)/u32(p.control.z))-1));return mask[c.y*p.size.z+c.x];}
fn load(tex:texture_2d<f32>,q:vec2<i32>)->vec4<f32>{return textureLoad(tex,clamp(q,vec2<i32>(0),vec2<i32>(p.size.xy)-1),0);}
fn sample(tex:texture_2d<f32>,q:vec2<f32>)->vec4<f32>{let base=vec2<i32>(floor(q));let f=fract(q);return mix(mix(load(tex,base),load(tex,base+vec2<i32>(1,0)),f.x),mix(load(tex,base+vec2<i32>(0,1)),load(tex,base+vec2<i32>(1,1)),f.x),f.y);}
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3<u32>){
 if(any(id.xy>=p.size.xy)){return;}
 if(p.control.w>0.5){var value=load(first,vec2<i32>(id.xy));if(p.control.x>=0.5){value=load(second,vec2<i32>(id.xy));}textureStore(output,vec2<i32>(id.xy),value);return;}
 let q=(vec2<f32>(id.xy)+0.5)/p.control.z-0.5;let base=vec2<i32>(floor(q));let f=fract(q);
 let motion=mix(mix(vector(base),vector(base+vec2<i32>(1,0)),f.x),mix(vector(base+vec2<i32>(0,1)),vector(base+vec2<i32>(1,1)),f.x),f.y)*p.control.z;
 let m=clamp(mix(mix(weight(base),weight(base+vec2<i32>(1,0)),f.x),mix(weight(base+vec2<i32>(0,1)),weight(base+vec2<i32>(1,1)),f.x),f.y),0.,1.);
 if(any(motion!=motion)||any(abs(motion)>vec4<f32>(1e8))||m!=m){atomicOr(&status,1u);textureStore(output,vec2<i32>(id.xy),vec4<f32>(0.));return;}
 let a=sample(first,vec2<f32>(id.xy)+motion.xy);let b=sample(second,vec2<f32>(id.xy)+motion.zw);
 textureStore(output,vec2<i32>(id.xy),mix(b,a,m));
}
