// Reject a lossy cache entry if any display channel differs by >24 SDR codes.
// Original RGBA8 stays retained; this is a cache policy, never a render failure.
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var compressed: texture_2d<f32>;
@group(0) @binding(2) var<storage, read_write> invalid: atomic<u32>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(source)) { return; }
    let reference = textureLoad(source, id.xy, 0);
    let actual = textureLoad(compressed, id.xy, 0);
    if any(abs(reference - actual) > vec4<f32>(24.01 / 255.0)) || actual.a != 1.0 {
        atomicStore(&invalid, 1u);
    }
}
