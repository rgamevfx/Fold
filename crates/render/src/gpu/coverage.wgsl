struct Params { outputRect: vec4<i32>, firstRect: vec4<i32>, secondRect: vec4<i32>, values: vec4<f32>, mode: vec4<u32> }
@group(0) @binding(0) var first: texture_2d<f32>;
@group(0) @binding(1) var second: texture_2d<f32>;
@group(0) @binding(2) var output: texture_storage_2d<r32float,write>;
@group(0) @binding(3) var<uniform> p: Params;
@group(0) @binding(4) var<storage,read_write> invalid: atomic<u32>;
fn sampleTile(image:texture_2d<f32>,rect:vec4<i32>,at:vec2<i32>,outside:f32,present:u32)->f32 {
    let q=at-rect.xy;
    if present==0u || any(q<vec2<i32>(0)) || any(q>=rect.zw) { return outside; }
    return textureLoad(image,q,0).x;
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=vec2<u32>(p.outputRect.zw)) {return;}
    let at=vec2<i32>(id.xy)+p.outputRect.xy;
    let a=sampleTile(first,p.firstRect,at,p.values.x,p.mode.y);
    let b=sampleTile(second,p.secondRect,at,p.values.y,p.mode.z);
    var v=a;
    switch p.mode.x {
        case 1u: {v=a+b*(1.0-a);}
        case 2u: {v=a+b;}
        case 3u: {v=b*(1.0-a);}
        case 4u: {v=a*b;}
        case 5u: {v=a;}
        case 6u: {v=select(a,1.0-a,p.values.w!=0.0)*p.values.z;}
        case 7u: {v=a*(1.0-p.values.z)+b*p.values.z;}
        default: {}
    }
    if p.mode.x>=1u && p.mode.x<=5u {v=clamp(v,0.0,1.0);}
    if v!=v || abs(v)>3.402823e38 {atomicStore(&invalid,1u);}
    textureStore(output,vec2<i32>(id.xy),vec4<f32>(v,0.0,0.0,1.0));
}
