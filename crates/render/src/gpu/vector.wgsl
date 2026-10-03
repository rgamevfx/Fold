// Analytic triangle/pixel intersection, not CPU coverage or fixed-grid MSAA.
struct Triangle { a: vec2<f32>, b: vec2<f32>, c: vec2<f32> }
struct Paint { rect: vec4<u32>, color: vec4<f32> }
struct Params { size: vec4<u32> }
struct Polygon { points: array<vec2<f32>,8>, count: u32 }
@group(0) @binding(0) var<storage,read> triangles: array<Triangle>;
@group(0) @binding(1) var<storage,read> paints: array<Paint>;
@group(0) @binding(2) var<storage,read> tiles: array<u32>;
@group(0) @binding(3) var<uniform> params: Params;
@group(0) @binding(4) var output: texture_storage_2d<rgba32float,write>;
@group(0) @binding(5) var<storage,read_write> invalid: atomic<u32>;
fn clip(p:Polygon,axis:u32,bound:f32,greater:bool)->Polygon {
    var result:Polygon;
    if p.count==0u {return result;}
    var a=p.points[p.count-1u]; var ain=(a[axis]>=bound)==greater;
    for(var i=0u;i<p.count;i++) {
        let b=p.points[i]; let bin=(b[axis]>=bound)==greater;
        if ain!=bin {
            let t=(bound-a[axis])/(b[axis]-a[axis]);
            result.points[result.count]=a+t*(b-a);result.count++;
        }
        if bin {result.points[result.count]=b;result.count++;}
        a=b;ain=bin;
    }
    return result;
}
fn cross(a:vec2<f32>,b:vec2<f32>)->f32 {return a.x*b.y-a.y*b.x;}
fn area(triangle:Triangle,pixel:vec2<f32>)->f32 {
    var p:Polygon;p.points[0]=triangle.a-pixel;p.points[1]=triangle.b-pixel;p.points[2]=triangle.c-pixel;p.count=3u;
    let orientation=cross(p.points[1]-p.points[0],p.points[2]-p.points[0]);
    if orientation==0.0 {return 0.0;}
    // Classify the whole pixel first. Interior pixels avoid polygon clipping.
    var inside=true;
    for(var i=0u;i<3u;i++) {
        let a=p.points[i];let edge=p.points[(i+1u)%3u]-a;
        let base=cross(edge,-a)*sign(orientation);
        let dx=-edge.y*sign(orientation);let dy=edge.x*sign(orientation);
        if base+max(dx,0.0)+max(dy,0.0)<0.0 {return 0.0;}
        if base+min(dx,0.0)+min(dy,0.0)<0.0 {inside=false;}
    }
    if inside {return 1.0;}
    p=clip(p,0u,0.0,true);p=clip(p,0u,1.0,false);p=clip(p,1u,0.0,true);p=clip(p,1u,1.0,false);
    var sum=0.0;
    for(var i=0u;i<p.count;i++) {sum+=cross(p.points[i],p.points[(i+1u)%p.count]);}
    return min(abs(sum)*0.5,1.0);
}
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
