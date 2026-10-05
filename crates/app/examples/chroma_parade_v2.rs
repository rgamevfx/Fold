//! A source badge + editable path + shipped Along Path asset. No graph wiring
//! beyond what the same artist-facing creation commands perform.
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
        .ok_or("Usage: chroma_parade_v2 NEW.fold [NEW-preview-directory]")?;
    if std::path::Path::new(&output).exists() {
        return Err("Output already exists".into());
    }
    let mut m = Motion::new_scene();
    m.info.width = 960;
    m.info.height = 540;
    m.info.rate = [24, 1];
    m.info.frames = 240;
    let badge = assets::badge(&mut m)?;
    let path = scene::create_object(&mut m, "fold.motion.path")?;
    node(&mut m, path)?.settings = serde_json::json!({"segments":[
        {"Move":[155.,300.]},{"Cubic":[[155.,120.],[365.,140.],[480.,300.]]},
        {"Cubic":[[625.,475.],[805.,450.],[805.,290.]]},
        {"Cubic":[[805.,115.],[615.,145.],[480.,300.]]},
        {"Cubic":[[325.,455.],[155.,455.],[155.,300.]]},"Close"],
        "attributes":{"color":[
            {"position":0.,"value":{"Color":[0.03,0.85,1.,1.]}},
            {"position":0.25,"value":{"Color":[0.45,0.18,1.,1.]}},
            {"position":0.5,"value":{"Color":[1.,0.12,0.3,1.]}},
            {"position":0.75,"value":{"Color":[1.,0.75,0.1,1.]}},
            {"position":1.,"value":{"Color":[0.03,0.85,1.,1.]}}]}});
    // Style the actual path, rather than making a second procedural guide object.
    let object = m
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == path)
        .unwrap();
    object.name = "Spectrum".into();
    let fill = object.appearance.unwrap();
    let tr = object.transform.unwrap();
    node(&mut m, fill)?.set("color", Datum::Color([0.; 4]));
    let mut stroke = fold_motion::graph::Node::new("fold.motion.stroke")?;
    stroke.connect("content", fill, "content");
    stroke.set("width", Datum::Scalar(2.));
    stroke.set("color", Datum::Color([0.08, 0.13, 0.22, 1.]));
    node(&mut m, tr)?.connect("content", stroke.id, "content");
    m.graph.nodes.push(stroke);
    let parade = assets::along_path(&mut m, badge, path)?;
    node(&mut m, parade)?.set("travel", Datum::Bool(true));
    node(&mut m, parade)?.set("enabled", Datum::Bool(true));
    node(&mut m, parade)?.set("orient", Datum::Bool(false));
    node(&mut m, parade)?.set("wave", Datum::Scalar(7.));
    node(&mut m, parade)?.set("pulse", Datum::Scalar(0.04));
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == parade)
        .unwrap()
        .name = "Parade".into();
    for (text, size, xy, color) in [
        ("CHROMA / PARADE", 34., [56., 38.], [0.86, 0.92, 1., 1.]),
        (
            "ONE BADGE. ONE PATH. YOUR DESIGN.",
            12.,
            [58., 82.],
            [0.35, 0.48, 0.65, 1.],
        ),
    ] {
        let id = scene::create_object(&mut m, "fold.motion.text")?;
        node(&mut m, id)?.set("text", Datum::Text(text.into()));
        node(&mut m, id)?.set("size", Datum::Scalar(size));
        let object = m
            .scene
            .as_mut()
            .unwrap()
            .objects
            .iter_mut()
            .find(|o| o.id == id)
            .unwrap();
        object.name = text.into();
        let tr = object.transform.unwrap();
        let fill = object.appearance.unwrap();
        node(&mut m, tr)?.set("translation", Datum::Vector(xy));
        node(&mut m, fill)?.set("color", Datum::Color(color));
    }
    m.validate()?;
    parade_project::save(
        m,
        output,
        std::env::args().nth(2),
        [
            "Chroma Parade v2 — MoGraph",
            "Chroma Parade v2 — Composite",
            "Chroma Parade v2 — Sequence",
        ],
    )
}
