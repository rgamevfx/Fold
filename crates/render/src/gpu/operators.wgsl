// Explicit RGBA32F reference-quality GPU operators. Pixel centers, nearest
// neighbor, transparent borders. No sRGB texture formats or implicit transfers.
struct Params {
    header: vec4<u32>, // operation, width, height, ACES flag
    color: vec4<f32>,
    matrix: vec4<f32>, // inverse a,b,c,d
    offset: vec4<f32>,
    rect: vec4<u32>,
    grade: array<vec4<f32>, 7>,
}
@group(0) @binding(0) var first: texture_2d<f32>;
@group(0) @binding(1) var second: texture_2d<f32>;
@group(0) @binding(5) var mask_image: texture_2d<f32>;
@group(0) @binding(2) var result: texture_storage_2d<rgba32float, write>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var<storage, read_write> invalid: atomic<u32>;
fn sample_first(p: vec2<i32>) -> vec4<f32> {
    if any(p < vec2<i32>(0)) || any(p >= vec2<i32>(params.header.yz)) { return vec4<f32>(0.0); }
    return textureLoad(first, p, 0);
}
fn cubic(value: f32) -> f32 {
    let x=abs(value);
    if x<1.0 { return (1.5*x-2.5)*x*x+1.0; }
    if x<2.0 { return ((-0.5*x+2.5)*x-4.0)*x+2.0; }
    return 0.0;
}
fn validate(value: vec4<f32>) {
    let exponent = bitcast<vec4<u32>>(value) & vec4<u32>(0x7f800000u);
    if any(exponent == vec4<u32>(0x7f800000u)) || value.a < 0.0 || value.a > 1.0 {
        atomicOr(&invalid, 1u);
    }
}
@compute @workgroup_size(8, 8)
fn main(@builtin(global_invocation_id) id: vec3<u32>) {
    if any(id.xy >= params.header.yz) { return; }
    let p = vec2<i32>(id.xy);
    var value = vec4<f32>(0.0);
    switch params.header.x {
        case 17u: {
            value=textureLoad(first,p,0); var rgb=value.rgb;
            switch params.rect.x {
                case 0u: { rgb*=params.color.x; }
                case 1u: { rgb=vec3<f32>(1.0)-rgb; }
                case 2u: { rgb=clamp(rgb,vec3<f32>(params.color.x),vec3<f32>(params.color.y)); }
                case 3u: { rgb*=value.a; }
                default: { if value.a==0.0 {rgb=vec3<f32>(0.0);} else {rgb/=value.a;} }
            }
            value=vec4<f32>(rgb,value.a);
        }
        case 14u: {
            let q=vec2<f32>(id.xy)+vec2<f32>(0.5); let m=params.matrix;
            let s=vec2<f32>(m.x*q.x+m.z*q.y,m.y*q.x+m.w*q.y)+params.offset.xy;
            if all(s>=vec2<f32>(-2.0)) && all(s<=vec2<f32>(params.header.yz)+vec2<f32>(2.0)) {
                if params.rect.x==0u { value=sample_first(vec2<i32>(floor(s))); }
                else {
                    let base=floor(s-vec2<f32>(0.5)); let fraction=s-vec2<f32>(0.5)-base;
                    var start=0; var end=1;
                    if params.rect.x==2u { start=-1; end=2; }
                    for (var y=start;y<=end;y+=1) { for (var x=start;x<=end;x+=1) {
                        var weight=0.0;
                        if params.rect.x==2u { weight=cubic(f32(x)-fraction.x)*cubic(f32(y)-fraction.y); }
                        else { weight=select(1.0-fraction.x,fraction.x,x==1)*select(1.0-fraction.y,fraction.y,y==1); }
                        value+=sample_first(vec2<i32>(base)+vec2<i32>(x,y))*weight;
                    } }
                    value.a=clamp(value.a,0.0,1.0);
                }
            }
        }
        case 12u: {
            let a = textureLoad(first,p,0); let b = textureLoad(second,p,0);
            switch params.rect.x {
                case 0u: { value = a+b*(1.0-a.a); }
                case 1u: { value = a+b; }
                case 2u: { value = a*b; }
                case 3u: { value = a+b-a*b; }
                case 4u: { value = abs(a-b); }
                case 5u: { value = min(a,b); }
                case 6u: { value = max(a,b); }
                case 7u: { value = a*b.a; }
                case 8u: { value = a*(1.0-b.a); }
                case 9u: { value = a*b.a+b*(1.0-a.a); }
                case 10u: { value = a*(1.0-b.a)+b*(1.0-a.a); }
                default: { value = a; }
            }
            value.a = clamp(value.a,0.0,1.0);
        }
        case 13u: {
            value = textureLoad(first,p,0);
            if params.rect.y == 0u {
                if params.rect.x != 0u && value.a == 0.0 { value = vec4<f32>(0.0); }
                else {
                    var rgb = value.rgb;
                    if params.rect.x != 0u { rgb /= value.a; }
                    rgb = ((rgb-params.grade[0].rgb)/(params.grade[1].rgb-params.grade[0].rgb)*(params.grade[3].rgb-params.grade[2].rgb)+params.grade[2].rgb)*params.grade[4].rgb+params.grade[5].rgb;
                    rgb = sign(rgb)*pow(abs(rgb),vec3<f32>(1.0)/params.grade[6].rgb);
                    if params.rect.x != 0u { rgb *= value.a; }
                    value = vec4<f32>(rgb,value.a);
                }
            }
        }
        case 10u: {
            let a = textureLoad(first, p, 0);
            let b = textureLoad(second, p, 0);
            for (var c = 0u; c < 4u; c += 1u) {
                let source = params.rect[c];
                if source < 4u { value[c] = a[source]; }
                else if source < 8u { value[c] = b[source - 4u]; }
                else if source == 9u { value[c] = 1.0; }
            }
        }
        case 11u: {
            let a = textureLoad(first, p, 0);
            let b = textureLoad(second, p, 0);
            var coverage = 1.0;
            if params.rect.x != 0u {
                coverage = clamp(textureLoad(mask_image, p, 0)[params.rect.y], 0.0, 1.0);
                if params.rect.z != 0u { coverage = 1.0 - coverage; }
            }
            coverage *= params.color.x;
            for (var c = 0u; c < 4u; c += 1u) {
                if params.matrix[c] == 0.0 || coverage == 0.0 { value[c] = a[c]; }
                else if coverage == 1.0 { value[c] = b[c]; }
                else { value[c] = a[c] * (1.0 - coverage) + b[c] * coverage; }
            }
        }
        case 0u: { value = params.color; }
        case 1u: { value = textureLoad(first, p, 0) * params.color.x; }
        case 2u: {
            let fg = textureLoad(first, p, 0);
            value = fg + textureLoad(second, p, 0) * (1.0 - fg.a);
        }
        case 9u: {
            let fg = textureLoad(first, p, 0) * params.color.x;
            validate(fg);
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
    validate(value);
    textureStore(result, p, value);
}
