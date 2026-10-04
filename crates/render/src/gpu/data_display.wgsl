@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var output: texture_storage_2d<rgba8unorm, write>;
@group(0) @binding(2) var<uniform> range: vec4<f32>;
@group(0) @binding(3) var<storage, read_write> invalid: atomic<u32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(output)) { return; }
    let sample = textureLoad(source, vec2<i32>(id.xy), 0).r;
    if (bitcast<u32>(sample) & 0x7f800000u) == 0x7f800000u { atomicOr(&invalid, 1u); }
    let value = clamp((sample - range.x) / (range.y - range.x), 0.0, 1.0);
    textureStore(output, vec2<i32>(id.xy), vec4<f32>(value, value, value, 1.0));
}
