//! A single transaction creates a procedural source and its owning Composition layer.
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_project::{DocumentRef, EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

pub const CREATE: &str = "fold.app.create-rust-asset";
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Args {
    pub composition: DocumentId,
    pub graph: DocumentId,
    pub definition: ObjectId,
    pub instance: ObjectId,
    pub layer: ObjectId,
    pub source: String,
}
pub fn create(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    let args: Args = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if snapshot.state().documents.contains_key(&args.graph) {
        return Err("Choose a new source graph identity".into());
    }
    let original = snapshot
        .state()
        .documents
        .get(&args.composition)
        .ok_or("Missing Composition")?;
    let composition = fold_composition::Composition::from_document(original)?;
    if composition.layers.iter().any(|l| l.id == args.layer) {
        return Err("Layer identity already exists".into());
    }
    let mut motion = fold_motion::Motion::new_scene();
    motion.info = composition.info.clone();
    fold_motion::program::define(&mut motion, args.definition, &args.source, cancel)?;
    place(snapshot, &args, motion, cancel)
}
pub fn place(
    snapshot: &Snapshot,
    args: &Args,
    mut motion: fold_motion::Motion,
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    if snapshot.state().documents.contains_key(&args.graph) {
        return Err("Choose a new source graph identity".into());
    }
    let original = snapshot
        .state()
        .documents
        .get(&args.composition)
        .ok_or("Missing Composition")?;
    let mut composition = fold_composition::Composition::from_document(original)?;
    if composition.layers.iter().any(|l| l.id == args.layer) {
        return Err("Layer identity already exists".into());
    }
    motion.info = composition.info.clone();
    fold_motion::program::scene_instance(&mut motion, args.definition, args.instance)?;
    let name = motion.groups[&args.definition].name.clone();
    let mut graph = motion.document(args.graph)?;
    graph
        .extensions
        .insert(fold_composition::LAYER_GRAPH.into(), true.into());
    let mut reference = DocumentRef {
        document: args.graph,
        output: "video".into(),
        extensions: Default::default(),
    };
    reference.extensions.insert(
        fold_composition::graph::OBJECT.into(),
        serde_json::json!(args.instance),
    );
    let mut layer = fold_composition::Layer::new(
        &name,
        Some(fold_platform::image_source::Source::Output(reference)),
        motion.info,
    )?;
    layer.id = args.layer;
    composition.layers.insert(0, layer);
    let mut updated = composition.document(args.composition)?;
    updated.extensions = original.extensions.clone();
    cancel.check()?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(graph), Mutation::PutDocument(updated)],
    })
}
