fn hash(p:vec2<i32>,salt:u32)->f32 {
    var h=bitcast<u32>(p.x)*1664525u+bitcast<u32>(p.y)*1013904223u+salt*747796405u;
    h=(h^(h>>16u))*2246822519u;h=(h^(h>>13u))*3266489917u;
    return (f32(h^(h>>16u))+1.)/4294967296.;
}
fn gaussian(p:vec2<i32>,salt:u32)->f32 {
    return sqrt(-2.*log(max(hash(p,salt),0.000001)))*cos(6.2831853*hash(p,salt+5237u));
}
fn texture_noise(p:vec2<f32>,salt:u32)->f32 {
    let cell=vec2<i32>(floor(p));let f=fract(p);
    let w=mix(f,f*f*(3.-2.*f),fold_params.values[3].x);
    let n=mix(mix(gaussian(cell,salt),gaussian(cell+vec2<i32>(1,0),salt),w.x),mix(gaussian(cell+vec2<i32>(0,1),salt),gaussian(cell+vec2<i32>(1,1),salt),w.x),w.y);
    // Normalize interpolation variance so size/softness do not change intensity.
    return n*inverseSqrt(max(0.25,((1.-w.x)*(1.-w.x)+w.x*w.x)*((1.-w.y)*(1.-w.y)+w.y*w.y)));
}
fn noise(p:vec2<f32>,salt:u32)->f32 {
    let c=fold_params.values[3].y;
    return (texture_noise(p,salt)+c*texture_noise(p/3.1,salt+9151u))*inverseSqrt(1.+c*c);
}
fn effect(uv:vec2<f32>)->vec4<f32> {
    let p=source(uv);if(p.a<=0.){return p;}
    let a=fold_params.values[0]; // amount size color mode
    let tone=fold_params.values[1].xyz;
    let rgb=fold_params.values[2].xyz;
    let timing=fold_params.values[3];
    let sensor=fold_params.values[4].xyz;
    let original=p.rgb/p.a;
    let point=uv*fold_params.size.xy/a.y;
    let salt=u32(timing.z);
    let mono=noise(point,salt);
    let chroma=vec3<f32>(noise(point,salt+101u),noise(point,salt+307u),noise(point,salt+701u));
    let random=(vec3<f32>(mono)*(1.-a.z)+chroma*a.z)*inverseSqrt((1.-a.z)*(1.-a.z)+a.z*a.z);
    let lum=max(dot(original,fold_params.values[5].xyz),0.);
    let shadow=1.-smoothstep(0.,0.18,lum);
    let highlight=smoothstep(0.18,1.,lum);
    let response=tone.x*shadow+tone.y*(1.-shadow)*(1.-highlight)+tone.z*highlight;
    let pixels=vec2<f32>(textureDimensions(fold_image_0));
    let footprint=max(fold_params.size.x/pixels.x,fold_params.size.y/pixels.y)/a.y;
    let attenuation=min(1.,1./max(footprint,0.0001));
    if(a.w<0.5){
        // Film density varies in exposure space. Correct the log-normal mean so
        // adding grain does not brighten shadows or shift the underlying grade.
        let sigma=rgb*(response*a.x*3.*attenuation);
        let density=exp2(random*sigma-0.34657359*sigma*sigma);
        return vec4<f32>(original*density*p.a,p.a);
    }
    let amplitude=vec3<f32>(response)*sqrt(max(original,vec3<f32>(0.))*sensor.x*sensor.x+vec3<f32>(sensor.y*sensor.y));
    let fixed_seed=u32(timing.w);
    let fixed=(gaussian(vec2<i32>(i32(floor(point.x)),0),fixed_seed)+gaussian(vec2<i32>(0,i32(floor(point.y))),fixed_seed+99u))*0.7071068;
    let pattern=select(0.,fixed*sensor.z,a.w>0.5);
    return vec4<f32>((original+(random*amplitude+vec3<f32>(pattern))*rgb*a.x*attenuation)*p.a,p.a);
}
