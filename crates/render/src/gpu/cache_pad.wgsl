@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var destination: texture_storage_2d<rgba8unorm, write>;
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(destination)) { return; }
    let point = min(id.xy, textureDimensions(source) - vec2<u32>(1));
    textureStore(destination, id.xy, textureLoad(source, point, 0));
}
