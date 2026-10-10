// Shared analytic area contract for instanced and clipped fallback rendering.
struct Triangle { a: vec2<f32>, b: vec2<f32>, c: vec2<f32>, padding: vec2<f32>, weights: vec4<f32> }
fn coverage(t:Triangle,p:vec2<f32>)->f32 {
    if all(t.weights.xyz==vec3<f32>(1.0)) {return 1.0;}
    let b=t.b-t.a;let c=t.c-t.a;let v=p-t.a;let det=cross(b,c);
    if det==0.0 {return 0.0;}
    let u=cross(v,c)/det;let w=cross(b,v)/det;
    return clamp((1.0-u-w)*t.weights.x+u*t.weights.y+w*t.weights.z,-1.0,1.0);
}
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
    if inside {return coverage(triangle,pixel+vec2<f32>(0.5));}
    p=clip(p,0u,0.0,true);p=clip(p,0u,1.0,false);p=clip(p,1u,0.0,true);p=clip(p,1u,1.0,false);
    var sum=0.0;var moment=vec2<f32>(0.0);
    for(var i=0u;i<p.count;i++) {let a=p.points[i];let b=p.points[(i+1u)%p.count];let w=cross(a,b);sum+=w;moment+=(a+b)*w;}
    if sum==0.0 {return 0.0;}
    return min(abs(sum)*0.5,1.0)*coverage(triangle,pixel+moment/(3.0*sum));
}
