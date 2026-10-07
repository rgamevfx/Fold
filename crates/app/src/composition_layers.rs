//! Cross-feature transactions. Engines stay independent; the application assembles
//! their editable outputs under the common composition owner.
use fold_composition::{Composition, Layer};
use fold_compositor::{Composite, Node, Parameters as P, Source as ImageSource};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_platform::image_source::Source;
use fold_project::{DocumentRef, EditBatch, Item, ItemId, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

pub const ADD: &str = "fold.app.add-layer-graph";
pub const CONNECT: &str = "fold.app.composition-connect";
fn reference(document: DocumentId) -> DocumentRef {
    DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    }
}
fn item(snapshot: &Snapshot, parent: DocumentId, id: DocumentId, name: String) -> Mutation {
    Mutation::PutItem(Item {
        id: ItemId::Document(id),
        name,
        parent: snapshot
            .state()
            .organization
            .items
            .iter()
            .find(|i| i.id == ItemId::Document(parent))
            .map(|i| i.parent)
            .unwrap_or_default(),
        order: snapshot.state().organization.items.len() as u64,
        extensions: Default::default(),
    })
}
fn source(
    snapshot: &Snapshot,
    source: &Source,
    info: fold_media::VideoInfo,
) -> Result<ImageSource, String> {
    match source {
        Source::Asset(asset) => fold_compositor::source_from_asset(snapshot, *asset, &info),
        Source::Output(source) => Ok(ImageSource::Document {
            source: source.clone(),
            info,
        }),
    }
}
fn source_info(
    snapshot: &Snapshot,
    source: &Source,
    authored: &fold_media::VideoInfo,
) -> Result<fold_media::VideoInfo, String> {
    match source {
        Source::Output(reference) => {
            crate::packages::builtins().output(snapshot, reference.document)
        }
        Source::Asset(id) => {
            let asset = snapshot
                .state()
                .assets
                .get(id)
                .ok_or("Missing image asset")?;
            if asset
                .extensions
                .contains_key(fold_media::ingest::METADATA_KEY)
            {
                fold_platform::image_source::asset_info(snapshot, *id, authored)
            } else {
                Ok(authored.clone())
            }
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct AddArgs {
    pub document: DocumentId,
    pub kind: String,
    #[serde(default)]
    pub layer: Option<ObjectId>,
}
pub fn add(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: AddArgs = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing composition")?;
    let mut composition = Composition::from_document(original)?;
    let id = DocumentId::new();
    let mut object = None;
    let (mut document, name, info) = match args.kind.as_str() {
        "motion" => {
            let mut motion = fold_motion::Motion::new_scene();
            motion.info = composition.info.clone();
            object = Some(fold_motion::authoring::scene::create_object(
                &mut motion,
                "fold.motion.text",
            )?);
            (motion.document(id)?, "MoGraph layer", motion.info)
        }
        "image" => {
            let solid = Node::new(
                P::Solid {
                    rgba: [0.2, 0.3, 0.5, 1.],
                },
                vec![],
            );
            let output = Node::new(P::Output, vec![solid.id]);
            let graph = Composite {
                info: composition.info.clone(),
                output: output.id,
                nodes: vec![solid, output],
                extensions: Default::default(),
            };
            (graph.document(id)?, "Compositing layer", graph.info)
        }
        "process" => {
            let selected = args.layer.ok_or("Select a layer to process")?;
            if composition.locked(selected) {
                return Err("Unlock the layer before processing".into());
            }
            let layer = composition.layer(selected)?;
            let input = layer
                .source
                .as_ref()
                .ok_or("Precompose the group before image processing")?;
            let info = source_info(snapshot, input, &layer.info)?;
            let read = Node::new(
                P::Read {
                    source: source(snapshot, input, info.clone())?,
                    start: Time::ZERO,
                    source_start: Time::ZERO,
                    duration: info.time(info.frames)?,
                },
                vec![],
            );
            // Neutral processing preserves pixels until the artist edits the graph.
            let grade = Node::new(
                P::ColorGrade {
                    settings: Default::default(),
                },
                vec![read.id],
            );
            let output = Node::new(P::Output, vec![grade.id]);
            let graph = Composite {
                info: info.clone(),
                output: output.id,
                nodes: vec![read, grade, output],
                extensions: Default::default(),
            };
            (graph.document(id)?, "Image processing", info)
        }
        _ => return Err("Unknown graph kind".into()),
    };
    document
        .extensions
        .insert(fold_composition::LAYER_GRAPH.into(), true.into());
    if args.kind == "process" {
        let layer = composition.layer_mut(args.layer.unwrap())?;
        layer.source = Some(Source::Output(reference(id)));
        layer.info = info;
    } else {
        let mut graph = reference(id);
        if let Some(object) = object {
            graph.extensions.insert(
                fold_composition::graph::OBJECT.into(),
                serde_json::json!(object),
            );
        }
        let layer = Layer::new(name, Some(Source::Output(graph)), info)?;
        composition.layers.insert(0, layer);
    }
    let mut updated = composition.document(args.document)?;
    updated.extensions = original.extensions.clone();
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![
            Mutation::PutDocument(document),
            item(snapshot, args.document, id, name.into()),
            Mutation::PutDocument(updated),
        ],
    })
}
#[derive(Serialize, Deserialize)]
pub struct ConnectArgs {
    pub document: DocumentId,
    pub source: Option<ObjectId>,
    pub target: ObjectId,
    #[serde(default)]
    pub port: Option<String>,
}
pub fn connect(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: ConnectArgs = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let composition = Composition::from_document(
        snapshot
            .state()
            .documents
            .get(&args.document)
            .ok_or("Missing composition")?,
    )?;
    if args.source == Some(args.target) || composition.locked(args.target) {
        return Err("Connect a different source to an unlocked graph".into());
    }
    let to = composition.layer(args.target)?;
    let Some(Source::Output(target)) = &to.source else {
        return Err("Target must be an editable graph".into());
    };
    let (source, info) = match args.source {
        Some(id) => {
            let layer = composition.layer(id)?;
            let source = layer
                .source
                .as_ref()
                .ok_or("Precompose a group before connecting its image")?;
            (
                Some(source.clone()),
                source_info(snapshot, source, &layer.info)?,
            )
        }
        None => (None, to.info.clone()),
    };
    let registry = crate::packages::builtins();
    let inputs = registry.image_inputs(snapshot, target.document)?;
    let port = match args.port {
        Some(port) => port,
        None if inputs.len() == 1 => inputs[0].id.clone(),
        _ => return Err("Choose a named image input on the graph".into()),
    };
    let document = registry.bind_image_input(snapshot, target.document, &port, source, info)?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}

/// Edit the shared hierarchy and each graph's object metadata atomically. The
/// common hierarchy owns these properties; node graphs retain their geometry,
/// relationships and editable node identities.
pub fn edit(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    let args: fold_composition::package::EditArgs =
        serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut batch = fold_composition::package::edit(snapshot, bytes, cancel)?;
    let before = snapshot
        .state()
        .documents
        .get(&args.document)
        .map(|d| Composition::from_document(d))
        .transpose()?;
    let graph_ids: std::collections::BTreeSet<_> = before
        .iter()
        .flat_map(|c| &c.layers)
        .chain(&args.composition.layers)
        .filter_map(|l| {
            let Some(Source::Output(source)) = &l.source else {
                return None;
            };
            l.graph_object().map(|_| source.document)
        })
        .collect();
    for graph_id in graph_ids {
        cancel.check()?;
        let original = snapshot
            .state()
            .documents
            .get(&graph_id)
            .ok_or("Missing layer graph")?;
        if original.type_id != fold_motion::MOTION {
            return Err("Unsupported object graph provider".into());
        }
        let mut motion = fold_motion::Motion::from_document(original)?;
        let old = motion.clone();
        let belongs = |l: &&Layer| matches!(&l.source, Some(Source::Output(source)) if source.document == graph_id);
        let layers: Vec<_> = args.composition.layers.iter().filter(belongs).collect();
        let removed: std::collections::BTreeSet<_> = before
            .iter()
            .flat_map(|c| &c.layers)
            .filter(belongs)
            .filter_map(|l| l.graph_object())
            .filter(|id| !layers.iter().any(|l| l.graph_object() == Some(*id)))
            .collect();
        let scene = motion.scene.as_mut().ok_or("Layer graph has no objects")?;
        scene.objects.retain(|o| !removed.contains(&o.id));
        for layer in &layers {
            let Some(object_id) = layer.graph_object() else {
                continue;
            };
            let object = scene
                .objects
                .iter_mut()
                .find(|o| o.id == object_id)
                .ok_or("Layer graph object is missing")?;
            object.state = layer.graph_state(&args.composition.layers)?;
        }
        scene.objects.sort_by_key(|object| {
            layers
                .iter()
                .position(|l| l.graph_object() == Some(object.id))
                .unwrap_or(usize::MAX)
        });
        if motion != old {
            let mut document = motion.document(graph_id)?;
            document.extensions = original.extensions.clone();
            batch.mutations.push(Mutation::PutDocument(document));
        }
    }
    Ok(batch)
}
