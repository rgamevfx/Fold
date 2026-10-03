// Existing decoder-native full-resolution GBR float planes. BT.709 YUV range
// expansion/chroma reconstruction remains the explicitly measured FFmpeg adapter;
// packing and the following input OCIO operation are GPU-resident.
@group(0) @binding(0) var<storage, read> planes: array<f32>;
@group(0) @binding(1) var output: texture_storage_2d<rgba32float, write>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    let size = textureDimensions(output);
    if any(id.xy >= size) { return; }
    let i = id.y * size.x + id.x;
    let count = size.x * size.y;
    textureStore(output, vec2<i32>(id.xy), vec4<f32>(planes[2u*count+i], planes[i], planes[count+i], 1.0));
}
