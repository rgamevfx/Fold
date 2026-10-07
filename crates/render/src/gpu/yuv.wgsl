// Verified 8-bit limited BT.709 4:2:0, left/center chroma. Preserve signed
// encoded RGB for the following OCIO input transform; no display transfer here.
struct Params { size: vec4<u32>, offsets: vec4<u32>, format: vec4<u32>, flags: vec4<u32>, chroma_size: vec4<u32> }
@group(0) @binding(0) var<storage, read> data: array<u32>;
@group(0) @binding(1) var output: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var<uniform> p: Params;
fn byte_at(i: u32) -> f32 { return f32((data[i / 4u] >> ((i % 4u)*8u)) & 255u); }
fn sample_at(i: u32) -> f32 {
    if p.format.x == 1u { return byte_at(i); }
    return f32(u32(byte_at(i) + 256.0*byte_at(i+1u)) >> p.chroma_size.z);
}
fn nearest(q: vec2<i32>, from_size: vec2<u32>, to_size: vec2<u32>) -> vec2<u32> {
    let v = clamp(q, vec2<i32>(0), vec2<i32>(to_size)-vec2<i32>(1));
    // Exact rational center-nearest mapping, without large source-size products.
    let whole = from_size / to_size;
    let rem = from_size % to_size;
    return vec2<u32>(v)*whole + whole/2u + ((2u*vec2<u32>(v)+vec2<u32>(1))*rem + (whole%2u)*to_size)/(2u*to_size);
}
fn chroma(q: vec2<i32>, domain: vec2<u32>, offset: u32) -> f32 {
    let actual = nearest(q, p.chroma_size.xy, domain);
    return sample_at(offset + actual.y*p.size.z + actual.x*p.size.w);
}
fn reconstruct(c: vec2<f32>, domain: vec2<u32>, offset: u32) -> f32 {
    let base = vec2<i32>(floor(c));
    let f = fract(c);
    return mix(mix(chroma(base,domain,offset), chroma(base+vec2<i32>(1,0),domain,offset),f.x),
               mix(chroma(base+vec2<i32>(0,1),domain,offset), chroma(base+vec2<i32>(1,1),domain,offset),f.x), f.y);
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output);
    if any(id.xy >= size) { return; }
    // Match the reference: even destinations resize YUV first; odd destinations
    // reconstruct at source resolution before nearest RGB resampling.
    var pixel = id.xy;
    var domain = size;
    if any((size % 2u) != vec2<u32>(0)) {
        pixel = nearest(vec2<i32>(id.xy), p.size.xy, size);
        domain = p.size.xy;
    }
    let src = nearest(vec2<i32>(pixel), p.size.xy, domain);
    let scale = f32(1u << (p.format.y-8u));
    let maximum = f32((1u << p.format.y)-1u);
    if p.chroma_size.w != 0u {
        let offset = src.y*p.offsets.z+src.x*p.format.x;
        var alpha = 1.0;
        if p.flags.x != 0u { alpha=clamp(sample_at(p.offsets.w+offset)/maximum,0.0,1.0); }
        let rgb=vec3<f32>(sample_at(p.offsets.y+offset),sample_at(offset),sample_at(p.offsets.x+offset))/maximum;
        textureStore(output,vec2<i32>(id.xy),vec4<f32>(rgb*alpha,alpha));
        return;
    }
    let raw = sample_at(src.y*p.offsets.z+src.x*p.format.x);
    var y = (raw-16.0*scale)/(219.0*scale);
    var crange = 224.0*scale;
    if p.flags.y != 0u { y = raw/maximum; crange = maximum; }
    let c = (vec2<f32>(pixel)-vec2<f32>(p.flags.zw)*0.5)/vec2<f32>(p.format.zw);
    let cs = (domain+p.format.zw-vec2<u32>(1))/p.format.zw;
    let u = (reconstruct(c,cs,p.offsets.x)-128.0*scale)/crange;
    let v = (reconstruct(c,cs,p.offsets.y)-128.0*scale)/crange;
    var alpha = 1.0;
    if p.flags.x != 0u { alpha = clamp(sample_at(p.offsets.w+src.y*p.offsets.z+src.x*p.format.x)/maximum,0.0,1.0); }
    textureStore(output, vec2<i32>(id.xy), vec4<f32>(vec3<f32>(y+1.5748*v, y-0.187324273*u-0.468124273*v, y+1.8556*u)*alpha, alpha));
}
