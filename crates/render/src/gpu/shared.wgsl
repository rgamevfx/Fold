@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var<storage, read_write> status: atomic<u32>;

@compute @workgroup_size(8, 8)
fn validate(@builtin(global_invocation_id) at: vec3<u32>) {
    if any(at.xy >= textureDimensions(image)) { return; }
    let pixel = textureLoad(image, vec2<i32>(at.xy), 0);
    let exponent = bitcast<vec4<u32>>(pixel) & vec4<u32>(0x7f800000u);
    if any(exponent == vec4<u32>(0x7f800000u)) || pixel.a < 0.0 || pixel.a > 1.0 {
        atomicOr(&status, 1u);
    }
}
