let mut a = Asset::new("LIFT / bead conveyor");
let text = a.text_control("text", "Word", "GLOW");
let size = a.scalar("size", "Type size", 160., "px");
let spacing = a.scalar("spacing", "Cell spacing", 11., "px");
let radius = a.scalar("radius", "Bead radius", 6.5, "px");
let speed = a.scalar("speed", "Rise speed", 80., "px/s");
let row = a.scalar("row", "Row spacing", 300., "px");
let coral = a.color("coral", "Coral", [1.,0.12,0.24,1.]);
let cyan = a.color("cyan", "Cyan", [0.02,0.72,1.,1.]);
let citrus = a.color("citrus", "Citrus", [0.76,1.,0.06,1.]);
let bg = a.rectangle("background", 500., 888.);
let bg = a.fill("background_ink", &bg, [0.004,0.008,0.018,1.]);
let dot = a.circle("dot", 1.4);
let dot = a.fill("dot_ink", &dot, [0.045,0.10,0.14,1.]);
let field = a.grid("field", 25., 45., [20.,20.], [10.,4.]);
let field = a.copies("field_dots", &dot, &field);
let mut scene = a.merge("base", &bg, &field);
let type_shape = a.text("word", text, size);
let type_shape = a.align("word_origin", &type_shape, [250.,0.]);
let points = a.shape_points("letter_cells", &type_shape, spacing);

let shadow = a.circle("shadow", radius.clone()+1.);
let shadow = a.fill("shadow_ink", &shadow, [0.025,0.045,0.06,1.]);
let shadow = a.translate("shadow_offset", &shadow, [3.,5.]);
let shadows = a.copies("shadows", &shadow, &points);
let bead = a.circle("bead", radius.clone());
let bead = a.fill("bead_ink", &bead, [1.,1.,1.,1.]);
let beads = a.copies("beads", &bead, &points);
let beads = a.wrangle("bead_palette", &beads, "Instances", "let k = index % 3; let c = if k == 0 { chc(\"coral\") } else if k == 1 { chc(\"cyan\") } else { chc(\"citrus\") }; color = [c[0]*0.45,c[1]*0.45,c[2]*0.45,c[3]];", &[("coral",coral.clone()),("cyan",cyan.clone()),("citrus",citrus.clone())]);
let face = a.circle("face", radius.clone()*0.82);
let face = a.fill("face_ink", &face, [1.,1.,1.,1.]);
let face = a.translate("face_offset", &face, [-1.2,-1.6]);
let faces = a.copies("faces", &face, &points);
let faces = a.wrangle("face_palette", &faces, "Instances", "let k = index % 3; color = if k == 0 { chc(\"coral\") } else if k == 1 { chc(\"cyan\") } else { chc(\"citrus\") };", &[("coral",coral.clone()),("cyan",cyan.clone()),("citrus",citrus.clone())]);
let beads = a.merge("bead_faces", &beads, &faces);
let glint = a.circle("glint", radius*0.23);
let glint = a.fill("glint_ink", &glint, [0.75,0.90,1.,0.6]);
let glint = a.translate("glint_offset", &glint, [-2.5,-3.]);
let glints = a.copies("glints", &glint, &points);
let word = a.merge("word_depth", &shadows, &beads);
let word = a.merge("word_glints", &word, &glints);
let phase = (time()*speed).modulo(row.clone());
for i in 0..6 {
 let y = row.clone() * (i as f64-1.) - phase.clone() + 145.;
 let moving = a.translate(&key("row",i), &word, vec2(0.,y));
 scene = a.merge(&key("stack",i), &scene, &moving);
}
a.output(&scene);
a
