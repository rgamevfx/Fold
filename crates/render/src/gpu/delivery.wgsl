// Straight RGBA16 little-endian delivery. Validate before clipping/quantization.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> packed: array<vec2<u32>>;
@group(0) @binding(2) var<storage, read_write> invalid: atomic<u32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(source);
    if any(id.xy >= size) { return; }
    let p = textureLoad(source, vec2<i32>(id.xy), 0);
    var v = vec4<f32>(0.0, 0.0, 0.0, p.a);
    if p.a > 0.0 { v = vec4<f32>(p.rgb / p.a, p.a); }
    let exponent = bitcast<vec4<u32>>(v) & vec4<u32>(0x7f800000u);
    if any(exponent == vec4<u32>(0x7f800000u)) || p.a < 0.0 || p.a > 1.0 {
        atomicOr(&invalid, 1u);
    }
    // Rust f32::round rounds nonnegative halfway values upward (WGSL round is even).
    let codes = vec4<u32>(floor(clamp(v, vec4<f32>(0.0), vec4<f32>(1.0)) * 65535.0 + 0.5));
    packed[id.y * size.x + id.x] = vec2<u32>(codes.r | (codes.g << 16u), codes.b | (codes.a << 16u));
}
