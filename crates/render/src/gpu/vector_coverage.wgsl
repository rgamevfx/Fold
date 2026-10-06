// Shared analytic area contract for instanced and clipped fallback rendering.
struct Triangle { a: vec2<f32>, b: vec2<f32>, c: vec2<f32> }
struct Polygon { points: array<vec2<f32>,8>, count: u32 }
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
