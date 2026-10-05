//! A source badge, one Duplicator, and a background. All controls stay editable.
use fold_motion::{
    Motion,
    authoring::{
        node,
        scene::{self, assets},
    },
    fields::Datum,
};
#[path = "support/parade_project.rs"]
mod parade_project;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("Usage: badge_wave NEW.fold [NEW-preview-directory]")?;
    if std::path::Path::new(&output).exists() {
        return Err("Output already exists".into());
    }
    let mut m = Motion::new_scene();
    m.info.width = 960;
    m.info.height = 540;
    m.info.rate = [24, 1];
    m.info.frames = 240;
    let bg = scene::create_object(&mut m, "fold.motion.rectangle")?;
    node(&mut m, bg)?.set("width", Datum::Scalar(960.));
    node(&mut m, bg)?.set("height", Datum::Scalar(540.));
    let o = m
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == bg)
        .unwrap();
    o.name = "Background".into();
    let tr = o.transform.unwrap();
    let fill = o.appearance.unwrap();
    node(&mut m, tr)?.set("translation", Datum::Vector([0., 0.]));
    node(&mut m, fill)?.set("color", Datum::Color([0.25, 0.48, 0.27, 1.]));
    let badge = assets::badge(&mut m)?;
    let children: Vec<_> = m
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .filter(|o| o.parent == Some(badge))
        .cloned()
        .collect();
    for o in children {
        if m.graph.node(o.id)?.kind == "fold.motion.text" {
            node(&mut m, o.id)?.set("text", Datum::Text("Hi Fold".into()));
            node(&mut m, o.id)?.set("size", Datum::Scalar(24.));
        } else {
            node(&mut m, o.id)?.set("radius", Datum::Scalar(24.));
            node(&mut m, o.id)?.set("padding", Datum::Vector([24., 12.]));
            let style = assets::apply_style(&mut m, o.id, "fold.motion.stroke")?;
            node(&mut m, style)?.set("width", Datum::Scalar(1.5));
            node(&mut m, style)?.set("color", Datum::Color([0.018, 0.012, 0.065, 1.]));
        }
    }
    let tr = m
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| o.id == badge)
        .unwrap()
        .transform
        .unwrap();
    node(&mut m, tr)?.set("translation", Datum::Vector([715., 270.]));
    let copies = assets::duplicate(&mut m, badge)?;
    let wave = assets::attach_oscillator(&mut m, copies, "offset_y")?;
    let ramp = assets::attach_color_ramp(&mut m, copies, "color")?;
    node(&mut m, wave)?.set("minimum", Datum::Scalar(-60.));
    node(&mut m, wave)?.set("maximum", Datum::Scalar(60.));
    node(&mut m, wave)?.set("duration", Datum::Scalar(1.25));
    assets::set_color_ramp(
        &mut m,
        ramp,
        fold_motion::nodes::color_ramp::Settings {
            stops: vec![
                fold_motion::nodes::color_ramp::stop(0., [0.38, 0.025, 0.12, 1.]),
                fold_motion::nodes::color_ramp::stop(1., [0.1, 0.12, 0.85, 1.]),
            ],
            ..Default::default()
        },
    )?;
    for (key, value) in [
        ("count", Datum::Scalar(52.)),
        ("spacing", Datum::Vector([-9., 0.])),
        ("stagger", Datum::Scalar(0.024)),
    ] {
        node(&mut m, copies)?.set(key, value);
    }
    m.validate()?;
    parade_project::save(
        m,
        output,
        std::env::args().nth(2),
        [
            "Badge Wave — MoGraph",
            "Badge Wave — Composite",
            "Badge Wave — Sequence",
        ],
    )
}
