fn linear709(c:vec3<f32>)->vec3<f32>{
 if(fold_params.values[5].w<0.5){return c;}
 return vec3<f32>(dot(c,vec3<f32>(1.7050509453,-0.6217921376,-0.0832588747)),dot(c,vec3<f32>(-0.1302564144,1.1408047676,-0.0105483187)),dot(c,vec3<f32>(-0.0240033567,-0.128968969,1.1529723406)));
}
fn working(c:vec3<f32>)->vec3<f32>{
 if(fold_params.values[5].w<0.5){return c;}
 return vec3<f32>(dot(c,vec3<f32>(0.6130974293,0.3395231366,0.0473794527)),dot(c,vec3<f32>(0.0701937228,0.9163538814,0.0134523986)),dot(c,vec3<f32>(0.0206155926,0.1095697731,0.8698146343)));
}
fn hsv(c0:vec3<f32>)->vec3<f32>{
 let c=max(c0,vec3<f32>(0.));let hi=max(c.r,max(c.g,c.b));let lo=min(c.r,min(c.g,c.b));let d=hi-lo;
 var h=0.;if(d>0.000001){if(hi==c.r){h=(c.g-c.b)/d;}else if(hi==c.g){h=2.+(c.b-c.r)/d;}else{h=4.+(c.r-c.g)/d;}}
 return vec3<f32>(fract(h/6.+1.),d/max(hi,0.000001),hi);
}
fn rgb(h:vec3<f32>)->vec3<f32>{let p=abs(fract(vec3<f32>(h.x)+vec3<f32>(0.,2./3.,1./3.))*6.-3.);return h.z*mix(vec3<f32>(1.),clamp(p-1.,vec3<f32>(0.),vec3<f32>(1.)),h.y);}
fn qualify(c:vec3<f32>)->f32 {
 let picked=hsv(linear709(fold_params.values[0].rgb));let q=hsv(linear709(c));let r=fold_params.values[2];let tol=fold_params.values[3];
 let hue_distance=min(abs(q.x-picked.x),1.-abs(q.x-picked.x))*360.;
 let h=select(hue_distance/max(tol.x*r.x,0.001),0.,picked.y<0.03);
 let s=abs(q.y-picked.y)/max(tol.y*r.x,0.00001);
 let v=abs(log2(max(q.z,0.000001)/max(picked.z,0.000001)))/max(tol.z*r.x,0.00001);
 let distance=max(h,max(s,v));
 return 1.-smoothstep(max(0.,1.-r.y),1.+max(r.y,0.00001),distance);
}
fn effect(uv:vec2<f32>)->vec4<f32>{
 let p=source(uv);if(p.a<=0.){return p;}
 let original=p.rgb/p.a;let weight=qualify(original);
 if(fold_params.values[5].x>0.5){return vec4<f32>(vec3<f32>(weight*p.a),1.);}
 let a=fold_params.values[4];
 if(fold_params.values[2].z<0.5 && a.x==0. && a.y==1.){return vec4<f32>(mix(original,original*exp2(a.z),weight)*p.a,p.a);}
 var h=hsv(linear709(original));
 if(fold_params.values[2].z>0.5){let t=hsv(linear709(fold_params.values[1].rgb));h=vec3<f32>(t.xy,h.z);}
 h.x=fract(h.x+a.x/360.+1.);h.y=clamp(h.y*a.y,0.,1.);
 var changed=working(rgb(h));
 let luma=fold_params.values[6].rgb;
 let y=dot(original,luma);let after=dot(changed,luma);
 if(after>0.000001){changed*=max(y,0.)/after;}
 changed*=exp2(a.z);
 return vec4<f32>(mix(original,changed,weight)*p.a,p.a);
}
