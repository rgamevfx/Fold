//! Inspectable acceptance fixture built exclusively with public graph-authoring tools.
//! Usage: cargo run -p fold-motion --example title -- /absolute/new-title.fold
use fold_foundation::{DocumentId, Time};
use fold_motion::{
    Motion,
    animation::{Interpolation, Key, Track},
    authoring as a,
    fields::Datum,
    geometry::Domain,
};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args_os()
        .nth(1)
        .ok_or("supply a new .fold path")?;
    let path = std::path::Path::new(&path);
    if path.exists() {
        return Err("fixture destination already exists".into());
    }
    let mut m = Motion::empty();
    m.info.width = 320;
    m.info.height = 180;
    m.info.frames = 72;
    let text = a::add_content(&mut m, "fold.motion.text")?;
    a::node(&mut m, text)?.set("size", Datum::Scalar(48.));
    let response = a::wave_position(&mut m, text, Domain::Glyphs)?;
    a::radial_influence(&mut m, response)?;
    let wave = m
        .graph
        .nodes
        .iter()
        .find(|n| n.kind == "fold.motion.wave")
        .unwrap()
        .id;
    a::expose_input(&mut m, wave, "amplitude")?;
    let transform = m
        .graph
        .nodes
        .iter()
        .find(|n| n.kind == "fold.motion.transform")
        .unwrap()
        .id;
    let keys = a::keyframe_input(&mut m, transform, "translation", Time::ZERO)?;
    a::node(&mut m, keys)?.settings = serde_json::to_value(Track {
        keys: vec![
            Key {
                time: Time::ZERO,
                value: Datum::Vector([20., 60.]),
                interpolation: Interpolation::Ease,
            },
            Key {
                time: Time::new(1, 1)?,
                value: Datum::Vector([70., 60.]),
                interpolation: Interpolation::Hold,
            },
        ],
    })?;
    let mut project = fold_project::Project::new(32);
    project.commit(fold_project::EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![fold_project::Mutation::PutDocument(
            m.document(DocumentId::new())?,
        )],
    })?;
    fold_project::save(&project.snapshot(), path)?;
    println!("Saved editable kinetic title to {}", path.display());
    Ok(())
}
