let mut a = Asset::new("RAY / Pearl Conveyor");
let text = a.text_control("text", "Letters (Unicode characters)", "NOV");
let speed = a.scalar("speed", "Conveyor speed", 0.5, "letters/s");
let spacing = a.scalar("spacing", "Cell spacing", 24., "px");
let radius = a.scalar("radius", "Cell radius / breathing track", 14., "px");
let pitch = a.scalar("pitch", "Letter separation", 500., "px");
let size = a.scalar("size", "Letter size", 460., "px");
let pink = a.color("pink", "Candy pink", [1.,0.09,0.48,1.]);
let green = a.color("green", "Acid mint", [0.12,0.9,0.18,1.]);
let blue = a.color("blue", "Electric blue", [0.04,0.3,1.,1.]);
let bg = a.rectangle("background",500.,888.);
let bg = a.fill("night",&bg,[0.002,0.004,0.009,1.]);
let mut out = bg;
let paints = [pink.clone(),green.clone(),blue.clone(),Signal::from([1.,1.,1.,1.]),Signal::from([0.4,0.43,0.48,1.])];
let mut cells = Vec::new();
for j in 0..5 {
 let c = a.circle(&key("cell",j),radius.clone());
 let c = a.fill(&key("cell_base",j),&c,paints[j].clone());
 let shadow = a.circle(&key("shadow",j),radius.clone());
 let shadow = a.fill(&key("shadow_fill",j),&shadow,paints[j].clone()*[0.18,0.18,0.18,1.]);
 let shadow = a.translate(&key("shadow_shift",j),&shadow,[3.,4.]);
 let cell = a.merge(&key("cell_depth",j),&shadow,&c);
 let shine = a.circle(&key("shine",j),radius.clone()*0.34);
 let shine = a.fill(&key("shine_fill",j),&shine,[1.,1.,1.,0.5]);
 let shine = a.translate(&key("shine_shift",j),&shine,vec2(radius.clone()*-0.32,radius.clone()*-0.32));
 cells.push(a.merge(&key("cell_finish",j),&cell,&shine));
}
let dot = a.circle("dot",2.2);
let dots = a.grid("dust_grid",21.,38.,[25.,24.],[0.,0.]);
let dots = a.copies("dust_copies",&dot,&dots);
let dots = a.wrangle("dust_palette",&dots,"Instances","let n = (index * 17 + 3) % 5; color = if n==0 {chc(\"pink\")} else if n==1 {chc(\"green\")} else if n==2 {chc(\"blue\")} else {[0.65,0.7,0.8,1.]}; set_attrv(\"scale\", [0.5 + ((index*7)%5) as f64*0.25;2]); opacity=0.2 + 0.45*(time.seconds*1.0471975512 + index as f64*0.23).sin().abs();",&[("pink",pink.clone()),("green",green.clone()),("blue",blue.clone())]);
out = a.merge("dust_over_night",&out,&dots);
for i in 0..3 {
 let select = a.add(Node::new(&key("select",i),"fold.motion.text_select").value("text",text.clone()).value("characters",true).value("index",i as f64)).socket("value");
 let letter = a.add(Node::new(&key("letter",i),"fold.motion.text").settings("{\"font\":\"01bd2cff-6ef3-4c79-9845-f8d35f3bf508\"}").link("text",&select).value("size",size.clone()));
 let letter = a.align(&key("letter_center",i),&letter,[0.,0.]);
 let points = a.shape_points(&key("letter_cells",i),&letter,spacing.clone());
 let phase = (time()*speed.clone() + i as f64).modulo(3.);
 let y = (phase - 1.5) * -pitch.clone() + 444.;
 for j in 0..5 {
  let k = i*5+j;
  let copies = a.copies(&key("letter_copies",k),&cells[j],&points);
  let copies = a.wrangle(&key("letter_palette",k),&copies,"Instances","let n = (index*13 + (position[0]/24.).abs() as usize) % 5; opacity = if n as f64 == chf(\"slot\") {1.} else {0.};",&[("slot",Signal::from(j as f64))]);
  let copies = a.translate(&key("conveyor",k),&copies,vec2(250.,y.clone()));
  out = a.merge(&key("stack",k),&out,&copies);
 }
}
a.output(&out);
a
