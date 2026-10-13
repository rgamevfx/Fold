fn effect(uv:vec2<f32>)->vec4<f32>{
 let p=source(uv);if(p.a<=0.){return p;}
 let v=fold_params.values[0];let center=fold_params.values[1].xy;let size=fold_params.size.xy;
 let radius=length((uv-center)*size)/(0.5*length(size));
 let protect=select(smoothstep(v.y,min(1.,v.y+0.2),radius),1.,v.y<=0.);
 let delta=glow(uv).rgb;let y=dot(delta,fold_params.values[2].rgb);
 let colored=mix(vec3<f32>(y),delta,v.x);
 return vec4<f32>(p.rgb+colored*p.a*protect,p.a);
}
