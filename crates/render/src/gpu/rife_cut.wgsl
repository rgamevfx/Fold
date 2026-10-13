struct Params {size:vec4<u32>,control:vec4<f32>};
@group(0) @binding(0) var<uniform> p:Params;
@group(0) @binding(1) var first:texture_2d<f32>;
@group(0) @binding(2) var second:texture_2d<f32>;
@group(0) @binding(3) var<storage,read_write> difference:atomic<u32>;
fn load(tex:texture_2d<f32>,q:vec2<i32>)->vec3<f32>{return textureLoad(tex,clamp(q,vec2<i32>(0),vec2<i32>(p.size.xy)-1),0).rgb;}
fn guide(tex:texture_2d<f32>,q:vec2<f32>)->vec3<f32>{
 let base=vec2<i32>(floor(q));let f=fract(q);
 var c=mix(mix(load(tex,base),load(tex,base+vec2<i32>(1,0)),f.x),mix(load(tex,base+vec2<i32>(0,1)),load(tex,base+vec2<i32>(1,1)),f.x),f.y);
 if(p.control.y>0.5){c=vec3<f32>(dot(c,vec3<f32>(1.7050509453,-0.6217921376,-0.0832588747)),dot(c,vec3<f32>(-0.1302564144,1.1408047676,-0.0105483187)),dot(c,vec3<f32>(-0.0240033567,-0.128968969,1.1529723406)));}
 c=clamp(c,vec3<f32>(0.),vec3<f32>(1.));return select(c*12.92,1.055*pow(c,vec3<f32>(1./2.4))-0.055,c>vec3<f32>(0.0031308));
}
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3<u32>){
 if(id.x>=32u||id.y>=18u){return;}
 let q=(vec2<f32>(id.xy)+0.5)/vec2<f32>(32.,18.)*vec2<f32>(p.size.xy)-0.5;
 let delta=dot(abs(guide(first,q)-guide(second,q)),vec3<f32>(1./3.));
 atomicAdd(&difference,u32(round(delta*65535.)));
}
