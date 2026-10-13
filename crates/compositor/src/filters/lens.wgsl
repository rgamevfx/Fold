fn warp(p: vec2<f32>) -> vec2<f32> {
    let k = fold_params.values[0];
    let t = fold_params.values[1].xy;
    let r = dot(p,p);
    let radial = 1. + r*(k.x+r*(k.y+r*k.z));
    return p*radial + vec2<f32>(2.*t.x*p.x*p.y+t.y*(r+2.*p.x*p.x), t.x*(r+2.*p.y*p.y)+2.*t.y*p.x*p.y);
}
fn solve_step(p: vec2<f32>, destination: vec2<f32>) -> vec2<f32> {
    let k = fold_params.values[0];
    let t = fold_params.values[1].xy;
    let r = dot(p,p);
    let radial = 1.+r*(k.x+r*(k.y+r*k.z));
    let d = 2.*(k.x+2.*k.y*r+3.*k.z*r*r);
    let a = radial+p.x*p.x*d+2.*t.x*p.y+6.*t.y*p.x;
    let b = p.x*p.y*d+2.*t.x*p.x+2.*t.y*p.y;
    let c = radial+p.y*p.y*d+6.*t.x*p.y+2.*t.y*p.x;
    let det = a*c-b*b;
    if(abs(det)<0.000001){return p;}
    let e = warp(p)-destination;
    return p-clamp(vec2<f32>(c*e.x-b*e.y,a*e.y-b*e.x)/det,vec2<f32>(-0.5),vec2<f32>(0.5));
}
fn tap(p: vec2<f32>) -> vec4<f32> {
    let size = vec2<f32>(textureDimensions(fold_image_0));
    if(fold_params.values[2].y>0.5){return source(clamp(p,vec2<f32>(0.5),size-vec2<f32>(0.5))/size);}
    return source(p/size);
}
fn cubic(x0: f32) -> f32 {
    let x=abs(x0);
    if(x<1.){return (1.5*x-2.5)*x*x+1.;}
    if(x<2.){return ((-0.5*x+2.5)*x-4.)*x+2.;}
    return 0.;
}
fn sample_image(uv: vec2<f32>) -> vec4<f32> {
    let size=vec2<f32>(textureDimensions(fold_image_0));
    let p=uv*size;
    if(fold_params.values[2].x<0.5){return tap(floor(p)+vec2<f32>(0.5));}
    let base=floor(p-vec2<f32>(0.5))+vec2<f32>(0.5);
    let f=p-base;
    if(fold_params.values[2].x<1.5){
        return mix(mix(tap(base),tap(base+vec2<f32>(1.,0.)),f.x),mix(tap(base+vec2<f32>(0.,1.)),tap(base+vec2<f32>(1.,1.)),f.x),f.y);
    }
    // CUBIC_TAPS
}
fn effect(uv: vec2<f32>) -> vec4<f32> {
    let center=fold_params.values[1].zw;
    let metric=vec2<f32>(fold_params.size.x/fold_params.size.y/fold_params.values[2].z,1.)*2.;
    let p=(uv-center)*metric/fold_params.values[2].w;
    var q=p;
    if(fold_params.values[0].w>0.5){q=warp(p);}else{
        // INVERSE_STEPS
        if(length(warp(q)-p)>0.001){return vec4<f32>(0.);}
    }
    let color=sample_image(q/metric+center);
    return vec4<f32>(color.rgb,clamp(color.a,0.,1.));
}
