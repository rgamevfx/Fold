fn fetch(p0:vec2<f32>)->vec4<f32>{
 let size=vec2<f32>(textureDimensions(fold_image_0));var p=p0;
 if(fold_params.values[2].w>0.5){p=clamp(p,vec2<f32>(0.5),size-0.5);}
 return source(p/size);
}
fn sample_linear(uv:vec2<f32>)->vec4<f32>{
 let size=vec2<f32>(textureDimensions(fold_image_0));let p=uv*size-0.5;let b=floor(p)+0.5;let f=fract(p);
 return mix(mix(fetch(b),fetch(b+vec2<f32>(1.,0.)),f.x),mix(fetch(b+vec2<f32>(0.,1.)),fetch(b+vec2<f32>(1.,1.)),f.x),f.y);
}
fn straight(p:vec4<f32>)->vec3<f32>{return p.rgb/max(p.a,0.000001);}
fn effect(uv:vec2<f32>)->vec4<f32>{
 let base=source(uv);let control=fold_params.values[0];let size=fold_params.size.xy;
 let d=(uv-fold_params.values[1].xy)*size;let radius=length(d)/(0.5*length(size));let direction=d/max(length(d),0.00001);
 let radial=direction*pow(radius,fold_params.values[1].z)*control.x;
 let offsets=fold_params.values[3];let offsets_b=fold_params.values[4].xy;
 let r=sample_linear(uv-(radial*control.y+offsets.xy*control.x)/size);
 let g=sample_linear(uv-(radial*control.z+offsets.zw*control.x)/size);
 let b=sample_linear(uv-(radial*control.w+offsets_b*control.x)/size);
 return vec4<f32>(vec3<f32>(straight(r).r,straight(g).g,straight(b).b)-straight(base),1.);
}
