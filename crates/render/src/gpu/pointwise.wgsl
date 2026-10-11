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

fn pointwise(input: vec4<f32>, params: Params) -> vec4<f32> {
    var value = input;
    switch params.header.x {
        case 1u: { value *= params.color.x; }
        case 17u: {
            var rgb=value.rgb;
            switch params.rect.x {
                case 0u: { rgb*=params.color.x; }
                case 1u: { rgb=vec3<f32>(1.0)-rgb; }
                case 2u: { rgb=clamp(rgb,vec3<f32>(params.color.x),vec3<f32>(params.color.y)); }
                case 3u: { rgb*=value.a; }
                default: { if value.a==0.0 {rgb=vec3<f32>(0.0);} else {rgb/=value.a;} }
            }
            value=vec4<f32>(rgb,value.a);
        }
        case 13u: {
            if params.rect.x != 0u && value.a == 0.0 { value = vec4<f32>(0.0); }
            else {
                var rgb=value.rgb;
                if params.rect.x != 0u { rgb/=value.a; }
                let slope=(params.grade[3].rgb-params.grade[2].rgb)*params.grade[4].rgb/(params.grade[1].rgb-params.grade[0].rgb);
                let intercept=params.grade[5].rgb+params.grade[2].rgb*params.grade[4].rgb-slope*params.grade[0].rgb;
                if params.rect.y != 0u { rgb=(sign(rgb)*pow(abs(rgb),params.grade[6].rgb)-intercept)/slope; }
                else { rgb=rgb*slope+intercept; rgb=sign(rgb)*pow(abs(rgb),vec3<f32>(1.0)/params.grade[6].rgb); }
                if params.rect.z != 0u { rgb=max(rgb,vec3<f32>(0.0)); }
                if params.rect.w != 0u { rgb=min(rgb,vec3<f32>(1.0)); }
                if params.rect.x != 0u { rgb*=value.a; }
                value=vec4<f32>(rgb,value.a);
            }
        }


        case 4u: {
            var rgb = value.rgb * params.color.rgb;
            if params.header.w == 0u { rgb = min(rgb, vec3<f32>(value.a)); }
            value = vec4<f32>(rgb, value.a);
        }
        default: {}
    }
    return value;
}
