// Analytic triangle/pixel intersection, not CPU coverage or fixed-grid MSAA.
struct Paint { rect: vec4<u32>, color: vec4<f32> }
struct Params { size: vec4<u32> }
@group(0) @binding(0) var<storage,read> triangles: array<Triangle>;
@group(0) @binding(1) var<storage,read> paints: array<Paint>;
@group(0) @binding(2) var<storage,read> tiles: array<u32>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var output: texture_storage_2d<rgba32float,write>;
@group(0) @binding(5) var<storage,read_write> invalid: atomic<u32>;
@compute @workgroup_size(8,8)
fn main(@builtin(global_invocation_id) id:vec3<u32>) {
    if any(id.xy>=params.size.xy) {return;}
    let tile=(id.y/16u)*params.size.z+id.x/16u;
    var color=vec4<f32>(0.0);
    var i=tiles[2u*tile];
    while i<tiles[2u*tile+1u] {
        let paint=paints[tiles[i]];
        let start=i+2u;let end=start+tiles[i+1u];i=end;
        if any(id.xy<paint.rect.xy) || any(id.xy>=paint.rect.xy+paint.rect.zw) {continue;}
        var coverage=0.0;
        for(var t=start;t<end;t++) {coverage+=area(triangles[tiles[t]],vec2<f32>(id.xy));}
        let alpha=clamp(coverage,0.0,1.0)*paint.color.a;
        color=vec4<f32>(paint.color.rgb*alpha,alpha)+color*(1.0-alpha);
    }
    if any(color!=color) || any(abs(color)>vec4<f32>(3.402823e38)) {atomicStore(&invalid,1u);}
    textureStore(output,vec2<i32>(id.xy),color);
}
