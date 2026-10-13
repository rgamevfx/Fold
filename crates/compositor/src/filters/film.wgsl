fn effect(uv:vec2<f32>) -> vec4<f32> {
    let p=source(uv);
    if(p.a<=0.){return p;}
    let a=fold_params.values[0]; // strength exposure contrast saturation
    let b=fold_params.values[1]; // rolloff shadow warmth highlight warmth halation
    let stock=fold_params.values[2]; // contrast saturation lift shoulder
    let tint=fold_params.values[3].xyz;
    let luma=fold_params.values[4].xyz;
    let original=p.rgb/p.a;
    let exposed=original*exp2(a.y);
    let lum=max(dot(exposed,luma),0.);
    let shadows=1.-smoothstep(0.,0.3,lum);
    let highlights=smoothstep(0.3,2.,lum);
    let balance=tint+vec3<f32>(0.09,0.,-0.09)*(b.y*shadows+b.z*highlights);
    let balanced=exposed*exp2(balance);
    // Signed scene-linear contrast, anchored at middle gray. No display encoding or clamp.
    let curved=sign(balanced)*0.18*pow(abs(balanced)/0.18,vec3<f32>(a.z*stock.x));
    let density=curved/(vec3<f32>(1.)+(b.x+stock.w)*max(curved,vec3<f32>(0.))/(vec3<f32>(1.)+max(curved,vec3<f32>(0.))));
    let y=dot(density,luma);
    var look=mix(vec3<f32>(y),density,a.w*stock.y)+vec3<f32>(stock.z*shadows);
    let halo=glow(uv).rgb;
    // Keep coverage unchanged; don't invent RGB outside the source matte.
    look+=halo*vec3<f32>(1.,0.23,0.055)*b.w;
    return vec4<f32>(mix(original,look,a.x)*p.a,p.a);
}
