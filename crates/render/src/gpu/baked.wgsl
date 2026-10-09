// Decode before filtering: cached BC7 RGB uses the sRGB transfer curve over
// premultiplied linear working values. Float16 is already linear.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var output: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var<uniform> params: vec4<u32>;
@group(0) @binding(3) var<storage, read_write> status: atomic<u32>;
fn texel(at: vec2<i32>, level: i32) -> vec4<f32> {
    let c = textureLoad(source, at, level);
    if params.x != 0u {
        let rgb = select(c.rgb / 12.92, pow((c.rgb + vec3(0.055)) / 1.055, vec3(2.4)), c.rgb > vec3(0.04045));
        return vec4(rgb, c.a);
    }
    return c;
}
fn bilinear(uv: vec2<f32>, level: i32) -> vec4<f32> {
    let dims = vec2<i32>(textureDimensions(source, level));
    let q = uv * vec2<f32>(dims) - vec2(0.5);
    let lo = vec2<i32>(floor(q));
    let f = fract(q);
    let end = dims - vec2(1);
    let a = texel(clamp(lo, vec2(0), end), level);
    let b = texel(clamp(lo + vec2(1,0), vec2(0), end), level);
    let c = texel(clamp(lo + vec2(0,1), vec2(0), end), level);
    let d = texel(clamp(lo + vec2(1,1), vec2(0), end), level);
    return mix(mix(a,b,f.x),mix(c,d,f.x),f.y);
}
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output);
    if any(id.xy >= size) { return; }
    let ratio = vec2<f32>(textureDimensions(source, 0)) / vec2<f32>(size);
    let lod = clamp(log2(max(max(ratio.x, ratio.y), 1.)), 0., f32(textureNumLevels(source)-1u));
    let uv = (vec2<f32>(id.xy)+vec2(0.5))/vec2<f32>(size);
    let color = mix(bilinear(uv, i32(floor(lod))), bilinear(uv, i32(ceil(lod))), fract(lod));
    if any(color != color) || any(abs(color) > vec4(3.402823e38)) || color.a < 0. || color.a > 1.000001 {
        atomicStore(&status, 1u);
    }
    textureStore(output, vec2<i32>(id.xy), vec4(color.rgb, clamp(color.a, 0., 1.)));
}
