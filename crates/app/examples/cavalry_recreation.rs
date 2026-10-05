//! Editable portrait study of the Cavalry reference, using public graph operations.
//! Usage: cavalry_recreation NEW.fold [NEW-preview-directory] [FONT.ttf]
use fold_foundation::{ObjectId, Time};
use fold_motion::{
    Motion,
    animation::{Interpolation, Key, Track},
    authoring::{self as a, node, scene},
    fields::Datum,
    graph::Node,
};
#[path = "support/parade_project.rs"]
mod parade_project;
fn object(m: &mut Motion, id: ObjectId) -> &mut fold_motion::scene::Object {
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap()
}
fn keys(
    m: &mut Motion,
    target: ObjectId,
    input: &str,
    values: &[(i64, Datum)],
) -> Result<(), String> {
    let mut n = Node::new("fold.motion.keyframes")?;
    n.settings = serde_json::to_value(Track {
        channels: vec![],
        keys: values
            .iter()
            .map(|(frame, value)| Key {
                time: Time::new(*frame, 25).unwrap(),
                value: value.clone(),
                interpolation: Interpolation::Ease,
            })
            .collect(),
    })
    .unwrap();
    node(m, target)?.connect(input, n.id, "value");
    m.graph.nodes.push(n);
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<_> = std::env::args().skip(1).collect();
    let output = args
        .first()
        .ok_or("NEW.fold [NEW-preview-directory] [FONT.ttf]")?;
    if std::path::Path::new(output).exists() {
        return Err("Output already exists".into());
    }
    let mut m = Motion::new_scene();
    m.info.width = 360;
    m.info.height = 640;
    m.info.rate = [25, 1];
    m.info.frames = 268;
    let bg = scene::create_object(&mut m, "fold.motion.rectangle")?;
    node(&mut m, bg)?.set("width", Datum::Scalar(360.));
    node(&mut m, bg)?.set("height", Datum::Scalar(640.));
    let o = object(&mut m, bg).clone();
    object(&mut m, bg).name = "Paper".into();
    node(&mut m, o.transform.unwrap())?.set("translation", Datum::Vector([0., 0.]));
    node(&mut m, o.appearance.unwrap())?.set("color", Datum::Color([1.; 4]));
    let text = scene::create_object(&mut m, "fold.motion.text")?;
    if let Some(path) = args.get(2) {
        let font = ObjectId::new();
        m.fonts.insert(
            font,
            fold_motion::document::Font {
                name: "Embedded reference substitute".into(),
                face_index: 0,
                source: fold_motion::document::FontSource::Embedded(std::fs::read(path)?),
            },
        );
        node(&mut m, text)?.settings = serde_json::json!({"font":font});
    }
    node(&mut m, text)?.set(
        "text",
        Datum::Text("Become\nObvious\nFirst\nChoice.".into()),
    );
    node(&mut m, text)?.set("size", Datum::Scalar(48.));
    node(&mut m, text)?.set("line_spacing", Datum::Scalar(1.02));
    let o = object(&mut m, text).clone();
    object(&mut m, text).name = "Become Obvious First Choice".into();
    node(&mut m, o.transform.unwrap())?.set("translation", Datum::Vector([27., 390.]));
    node(&mut m, o.appearance.unwrap())?.set("color", Datum::Color([0.006, 0.013, 0.016, 1.]));
    let grid = a::add_node(&mut m, "fold.motion.shape_grid")?;
    a::insert_after(&mut m, text, grid, "content")?;
    keys(
        &mut m,
        grid,
        "cell_size",
        &[
            (0, Datum::Scalar(6.)),
            (10, Datum::Scalar(6.)),
            (36, Datum::Scalar(0.)),
        ],
    )?;
    let delay = a::add_node(&mut m, "fold.motion.index")?;
    let time = a::add_node(&mut m, "fold.motion.time")?;
    let progress = a::add_node(&mut m, "fold.motion.math")?;
    node(&mut m, progress)?.settings = serde_json::json!({"operation":"Multiply"});
    node(&mut m, progress)?.connect("a", delay, "value");
    node(&mut m, progress)?.set("b", Datum::Scalar(0.07));
    let phase = a::add_node(&mut m, "fold.motion.math")?;
    node(&mut m, phase)?.connect("a", time, "value");
    node(&mut m, phase)?.connect("b", progress, "value");
    let wave = a::add_node(&mut m, "fold.motion.wave")?;
    node(&mut m, wave)?.connect("coordinate", phase, "value");
    node(&mut m, wave)?.set("frequency", Datum::Scalar(1.2));
    keys(
        &mut m,
        wave,
        "amplitude",
        &[
            (0, Datum::Scalar(0.)),
            (12, Datum::Scalar(19.)),
            (40, Datum::Scalar(0.)),
        ],
    )?;
    let xy = a::add_node(&mut m, "fold.motion.combine_xy")?;
    node(&mut m, xy)?.connect("y", wave, "value");
    let move_node = a::add_node(&mut m, "fold.motion.set_position")?;
    node(&mut m, move_node)?.settings = serde_json::json!({"domain":"Glyphs","combine":"Add"});
    node(&mut m, move_node)?.connect("value", xy, "value");
    a::insert_after(&mut m, grid, move_node, "content")?;
    let mark = scene::create_object(&mut m, "fold.motion.text")?;
    node(&mut m, mark)?.set("text", Datum::Text("16ᵗʰ FORM".into()));
    node(&mut m, mark)?.set("size", Datum::Scalar(14.));
    let o = object(&mut m, mark).clone();
    object(&mut m, mark).name = "Signature".into();
    node(&mut m, o.transform.unwrap())?.set("translation", Datum::Vector([27., 16.]));
    node(&mut m, o.appearance.unwrap())?.set("color", Datum::Color([0.006, 0.013, 0.016, 1.]));
    let square = scene::create_object(&mut m, "fold.motion.rectangle")?;
    node(&mut m, square)?.set("width", Datum::Scalar(16.));
    node(&mut m, square)?.set("height", Datum::Scalar(22.));
    let o = object(&mut m, square).clone();
    object(&mut m, square).name = "Orange profile — editable source".into();
    object(&mut m, square).visible = false;
    node(&mut m, o.transform.unwrap())?.set("translation", Datum::Vector([0., 0.]));
    node(&mut m, o.appearance.unwrap())?.set("color", Datum::Color([1., 0.12, 0.035, 0.8]));
    let owner = a::drift::from_object(&mut m, square)?;
    let transform = object(&mut m, owner).transform.unwrap();
    node(&mut m, transform)?.set("translation", Datum::Vector([0., 0.]));
    node(&mut m, owner)?.set("count", Datum::Scalar(24.));
    node(&mut m, owner)?.set("speed", Datum::Scalar(85.));
    node(&mut m, owner)?.set("sweep_enabled", Datum::Bool(true));
    node(&mut m, owner)?.set("stretch", Datum::Scalar(0.3));
    node(&mut m, owner)?.set("trail_samples", Datum::Scalar(4.));
    keys(
        &mut m,
        owner,
        "angle_step",
        &[
            (0, Datum::Scalar(90.)),
            (70, Datum::Scalar(90.)),
            (90, Datum::Scalar(45.)),
            (205, Datum::Scalar(45.)),
            (235, Datum::Scalar(90.)),
        ],
    )?;
    for group in m.groups.values_mut() {
        for (i, n) in group.graph.nodes.iter_mut().enumerate() {
            n.position = [(i % 5) as f32 * 250., (i / 5) as f32 * 170.];
        }
    }
    for (i, n) in m.graph.nodes.iter_mut().enumerate() {
        n.position = [(i % 6) as f32 * 250., (i / 6) as f32 * 170.];
    }
    m.validate()?;
    parade_project::save(
        m,
        output.clone(),
        args.get(1).cloned(),
        [
            "Typography Study — Motion",
            "Typography Study — Composite",
            "Typography Study — Sequence",
        ],
    )
}
