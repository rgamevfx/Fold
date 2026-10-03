@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var output: texture_storage_2d<FORMAT, write>;
@group(0) @binding(2) var<storage, read_write> invalid: atomic<u32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(output)) { return; }
    let p = textureLoad(source, vec2<i32>(id.xy), 0);
    let exponent = bitcast<vec4<u32>>(p) & vec4<u32>(0x7f800000u);
    if any(exponent == vec4<u32>(0x7f800000u)) || p.a < 0.0 || p.a > 1.0 { atomicOr(&invalid, 1u); }
    textureStore(output, vec2<i32>(id.xy), p);
}
