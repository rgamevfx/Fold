// Explicit RGBA32F reference-quality GPU operators. Pixel centers, nearest
// neighbor, transparent borders. No sRGB texture formats or implicit transfers.
struct Params {
    header: vec4<u32>, // operation, width, height, ACES flag
    color: vec4<f32>,
    matrix: vec4<f32>, // inverse a,b,c,d
    offset: vec4<f32>,
    rect: vec4<u32>,
}
@group(0) @binding(0) var first: texture_2d<f32>;
@group(0) @binding(1) var second: texture_2d<f32>;
@group(0) @binding(2) var result: texture_storage_2d<rgba32float, write>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var<storage, read_write> invalid: atomic<u32>;
fn sample_first(p: vec2<i32>) -> vec4<f32> {
    if any(p < vec2<i32>(0)) || any(p >= vec2<i32>(params.header.yz)) { return vec4<f32>(0.0); }
    return textureLoad(first, p, 0);
}
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.header.yz) { return; }
    let p = vec2<i32>(id.xy);
    var value = vec4<f32>(0.0);
    switch params.header.x {
        case 0u: { value = params.color; }
        case 1u: { value = textureLoad(first, p, 0) * params.color.x; }
        case 2u: {
            let fg = textureLoad(first, p, 0);
            value = fg + textureLoad(second, p, 0) * (1.0 - fg.a);
        }
        case 3u: {
            if all(id.xy >= params.rect.xy) && all(id.xy < params.rect.zw) { value = textureLoad(first, p, 0); }
        }
        case 4u: {
            value = textureLoad(first, p, 0);
            var rgb = value.rgb * params.color.rgb;
            if params.header.w == 0u { rgb = min(rgb, vec3<f32>(value.a)); }
            value = vec4<f32>(rgb, value.a);
        }
        case 5u: { value = textureLoad(first, p, 0) * textureLoad(second, p, 0).a; }
        case 6u: {
            let q = vec2<f32>(id.xy) + vec2<f32>(0.5);
            let m = params.matrix;
            let s = floor(vec2<f32>(m.x*q.x + m.z*q.y, m.y*q.x + m.w*q.y) + params.offset.xy);
            // Check in float before conversion so huge finite transforms cannot
            // saturate an integer conversion into an apparently valid coordinate.
            if all(s >= vec2<f32>(0.0)) && all(s < vec2<f32>(params.header.yz)) {
                value = textureLoad(first, vec2<i32>(s), 0);
            }
        }
        case 7u, 8u: {
            let radius = i32(params.rect.x);
            for (var k = -radius; k <= radius; k += 1) {
                var delta = vec2<i32>(k, 0);
                if params.header.x == 8u { delta = vec2<i32>(0, k); }
                value += sample_first(p + delta);
            }
            value /= f32(2 * radius + 1);
            if params.header.x == 8u {
                if params.header.w == 0u { value = clamp(value, vec4<f32>(0.0), vec4<f32>(1.0)); }
                else { value.a = clamp(value.a, 0.0, 1.0); }
            }
        }
        default: {}
    }
    let exponent = bitcast<vec4<u32>>(value) & vec4<u32>(0x7f800000u);
    if any(exponent == vec4<u32>(0x7f800000u)) || value.a < 0.0 || value.a > 1.0 {
        atomicOr(&invalid, 1u);
    }
    textureStore(result, p, value);
}
