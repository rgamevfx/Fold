// The pinned opaque encoder may emit BC7 mode 6, whose shared endpoint
// p-bits may round opaque alpha to 254. Other selected modes are opaque. Display images are black-matted
// and MUST stay opaque when sampled by ImGui's alpha-blending pipeline.
// Set both alpha endpoints to 127 and both p-bits to 1. RGB endpoints change
// by at most one SDR code; include this in the measured compression tolerance.
@group(0) @binding(0) var<storage, read_write> blocks: array<vec4<u32>>;
@compute @workgroup_size(64)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if id.x >= arrayLength(&blocks) { return; }
    if (blocks[id.x].x & 127u) != 64u { return; }
    blocks[id.x].y |= 0xfffe0000u;
    blocks[id.x].z |= 1u;
}
