// Exact separable finite Gaussian kernel, shared with the CPU reference.
// Each 64-pixel stripe cooperatively loads its overlapping samples in chunks.
struct Params {
    dimensions: vec2<u32>,
    axis: u32,
    edges: u32,
    radius: u32,
    divisor: f32,
    padding: vec2<u32>,
}
@group(0) @binding(0) var source: texture_2d<f32>;
@group(0) @binding(1) var result: texture_storage_2d<rgba32float, write>;
@group(0) @binding(2) var<uniform> params: Params;
@group(0) @binding(3) var<storage, read> weights: array<f32>;
@group(0) @binding(4) var<storage, read_write> invalid: atomic<u32>;
var<workgroup> samples: array<vec4<f32>, 128>;
fn coordinate(along: i32, across: i32) -> vec2<i32> {
    if params.axis == 0u { return vec2<i32>(along, across); }
    return vec2<i32>(across, along);
}
fn load(p: vec2<i32>) -> vec4<f32> {
    if params.edges == 1u {
        return textureLoad(source, clamp(p, vec2<i32>(0), vec2<i32>(params.dimensions)-vec2<i32>(1)), 0);
    }
    if any(p < vec2<i32>(0)) || any(p >= vec2<i32>(params.dimensions)) { return vec4<f32>(0.0); }
    return textureLoad(source, p, 0);
}
@compute @workgroup_size(64)
fn main(@builtin(workgroup_id) group: vec3<u32>, @builtin(local_invocation_index) lane: u32) {
    let origin = i32(group.x * 64u) - i32(params.radius);
    let count = 2u * params.radius + 1u;
    var value = vec4<f32>(0.0);
    for (var chunk = 0u; chunk < count; chunk += 64u) {
        samples[lane] = load(coordinate(origin + i32(chunk + lane), i32(group.y)));
        samples[lane + 64u] = load(coordinate(origin + i32(chunk + lane + 64u), i32(group.y)));
        workgroupBarrier();
        for (var tap = 0u; tap < min(64u, count - chunk); tap += 1u) {
            value += samples[lane + tap] * weights[chunk + tap];
        }
        workgroupBarrier();
    }
    // Partial stripes participate in every barrier before discarding extra lanes.
    let p = coordinate(i32(group.x * 64u + lane), i32(group.y));
    if any(p >= vec2<i32>(params.dimensions)) { return; }
    value /= params.divisor;
    value.a = clamp(value.a, 0.0, 1.0);
    let exponent = bitcast<vec4<u32>>(value) & vec4<u32>(0x7f800000u);
    if any(exponent == vec4<u32>(0x7f800000u)) || value.a < 0.0 || value.a > 1.0 {
        atomicOr(&invalid, 1u);
    }
    textureStore(result, p, value);
}
