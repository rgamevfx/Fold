struct Steps { count: vec4<u32>, values: array<Params,16>, }
@group(0) @binding(0) var input: texture_2d<f32>;
@group(0) @binding(1) var output: texture_storage_2d<rgba32float,write>;
@group(0) @binding(2) var<uniform> steps: Steps;
@group(0) @binding(3) var<storage,read_write> invalid: atomic<u32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= textureDimensions(output)) { return; }
    var value = textureLoad(input, vec2<i32>(id.xy), 0);
    for (var i = 0u; i < steps.count.x; i += 1u) {
        value = pointwise(value, steps.values[i]);
        // Preserve validation of every logical intermediate, even if a later
        // clamp would conceal overflow or invalid alpha.
        let exponent = bitcast<vec4<u32>>(value) & vec4<u32>(0x7f800000u);
        if any(exponent == vec4<u32>(0x7f800000u)) || value.a < 0.0 || value.a > 1.0 {
            atomicOr(&invalid, 1u);
        }
    }
    textureStore(output, vec2<i32>(id.xy), value);
}
