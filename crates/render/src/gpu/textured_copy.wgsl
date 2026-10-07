@group(0) @binding(0) var source:texture_2d<f32>;
@group(0) @binding(1) var output_image:texture_storage_2d<rgba32float,write>;
@group(0) @binding(2) var<storage,read_write> status:atomic<u32>;
@compute @workgroup_size(8,8) fn main(@builtin(global_invocation_id) id:vec3<u32>) {
 let dims=textureDimensions(output_image); if any(id.xy>=dims) {return;}
 let c=textureLoad(source,vec2<i32>(id.xy),0);
 if any(abs(c)>vec4(3.402823e38)) || any(c!=c) || c.a<0. || c.a>1. {atomicStore(&status,1u);}
 textureStore(output_image,vec2<i32>(id.xy),c);
}
