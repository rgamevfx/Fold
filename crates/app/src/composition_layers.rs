//! Cross-feature transactions. Engines stay independent; the application assembles
//! their editable outputs under the common composition owner.
use fold_composition::{Composition, Layer};
use fold_compositor::{Composite, Node, Parameters as P, Source as ImageSource};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_platform::image_source::Source;
use fold_project::{EditBatch, Item, ItemId, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

pub const ADD: &str = "fold.app.add-layer-graph";
pub const CONNECT: &str = "fold.app.composition-connect";
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
    let (mut document, name, info) = match args.kind.as_str() {
        "scene" => {
            let document = fold_motion::scene::package::create(
                id,
                fold_motion::model::Canvas {
                    width: composition.info.width,
                    height: composition.info.height,
                    rate: composition.info.rate,
                    start: Time::ZERO,
                    end: composition.info.time(composition.info.frames)?,
                    extensions: Default::default(),
                },
            )?;
            (document, "Scene", composition.info.clone())
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
    let output = crate::packages::builtins()
        .document_outputs(&document)
        .into_iter()
        .next()
        .ok_or("New layer graph has no published image output")?;
    output.info?;
    let output = output.reference;
    if args.kind == "process" {
        let layer = composition.layer_mut(args.layer.unwrap())?;
        layer.source = Some(Source::Output(output));
        layer.info = info;
    } else {
        let layer = Layer::new(name, Some(Source::Output(output)), info)?;
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

/// Edit the shared hierarchy and provider-owned animation channels atomically.
pub fn edit(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    let args: fold_composition::package::EditArgs =
        serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut batch = fold_composition::package::edit(snapshot, bytes, cancel)?;
    let registry = crate::packages::builtins();
    struct AnimationProposal {
        time: Time,
        bake_time: Option<Time>,
        channels: std::collections::BTreeMap<
            fold_platform::animation::Target,
            fold_platform::animation::Channel,
        >,
    }
    let mut proposals = std::collections::BTreeMap::<DocumentId, AnimationProposal>::new();
    for edit in &args.graph_animation {
        let layer = args.composition.layer(edit.layer)?;
        if args.composition.locked(edit.layer) {
            return Err("Unlock the layer before editing keys".into());
        }
        let Some(Source::Output(source)) = &layer.source else {
            return Err("Layer has no animation graph".into());
        };
        let original = snapshot
            .state()
            .documents
            .get(&source.document)
            .ok_or("Missing animation graph")?;
        if original.type_id == fold_composition::COMPOSITION {
            return Err("Open the nested composition to edit its keys".into());
        }
        let allowed = registry.animation_channels(original, None)?;
        let time = layer.local_time(edit.time)?;
        let proposal = proposals
            .entry(source.document)
            .or_insert_with(|| AnimationProposal {
                time,
                bake_time: None,
                channels: Default::default(),
            });
        for channel in &edit.channels {
            let original = allowed
                .iter()
                .find(|c| c.target == channel.target)
                .ok_or("Animation channel does not belong to this layer")?;
            let mut identity = channel.clone();
            identity.curve = original.curve.clone();
            if &identity != original {
                return Err("Animation channel metadata changed".into());
            }
            if channel.curve.keys.is_empty() {
                if proposal.bake_time.is_some_and(|previous| previous != time) {
                    return Err(
                        "Shared graph animation removal has conflicting source times".into(),
                    );
                }
                proposal.bake_time = Some(time);
            }
            if proposal
                .channels
                .insert(channel.target.clone(), channel.clone())
                .is_some_and(|previous| previous != *channel)
            {
                return Err("Shared graph channel received conflicting edits".into());
            }
        }
    }
    // Validate one complete proposal per source. In particular, vector components
    // selected through different layer occurrences still share one typed key.
    for (document, proposal) in proposals {
        if proposal.channels.is_empty() {
            continue;
        }
        let original = snapshot
            .state()
            .documents
            .get(&document)
            .ok_or("Missing animation graph")?;
        let channels = proposal.channels.into_values().collect::<Vec<_>>();
        let updated = registry.edit_animation(
            original,
            &channels,
            proposal.bake_time.unwrap_or(proposal.time),
        )?;
        batch.mutations.push(Mutation::PutDocument(updated));
    }
    Ok(batch)
}

/// Outliner and Composition use the provider's single hierarchy transaction.
pub fn edit_hierarchy(
    snapshot: &Snapshot,
    bytes: &[u8],
    cancel: &Cancel,
) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: fold_platform::hierarchy::EditArgs =
        serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing hierarchy document")?;
    let document = crate::packages::builtins().edit_hierarchy(original, &args.edits)?;
    cancel.check()?;
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
