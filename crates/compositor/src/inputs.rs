//! Provider-owned image input binding; external editors never interpret this graph.
use crate::{Composite, OperationConfig, Source};
use fold_foundation::{ObjectId, Time};
use fold_platform::{image_source::Source as ImageSource, layer_graph::ImageInput};
use fold_project::{Document, Snapshot};
pub(crate) fn describe(document: &Document) -> Result<Vec<ImageInput>, String> {
    let native = Composite::read_document(document)?;
    let graph = native.prepare_document(document)?;
    let mut inputs = vec![];
    for (index, node) in graph.nodes.iter().enumerate() {
        if let OperationConfig::Read { source, .. } = &node.config() {
            inputs.push(ImageInput {
                id: serde_json::to_string(&node.id).unwrap(),
                label: format!("Read {}", index + 1),
                source: match source {
                    Source::Document { source, .. } => Some(ImageSource::Output(source.clone())),
                    Source::Asset { asset, .. } | Source::Exr { asset, .. } => {
                        Some(ImageSource::Asset(*asset))
                    }
                    Source::Unassigned { .. } => None,
                },
            });
        }
    }
    // The initial solid is a replaceable source, preserving its stable node ID.
    if inputs.is_empty() {
        let solids: Vec<_> = graph
            .nodes
            .iter()
            .filter(|n| matches!(n.config(), OperationConfig::Solid { .. }))
            .collect();
        if let [node] = solids.as_slice()
            && node.animation.is_empty()
        {
            inputs.push(ImageInput {
                id: serde_json::to_string(&node.id).unwrap(),
                label: "Source image".into(),
                source: None,
            });
        }
    }
    Ok(inputs)
}
pub(crate) fn bind(
    snapshot: &Snapshot,
    document: &Document,
    port: &str,
    input: Option<ImageSource>,
    info: fold_media::VideoInfo,
) -> Result<Document, String> {
    let id: ObjectId = serde_json::from_str(port).map_err(|e| e.to_string())?;
    let mut graph = Composite::from_document(document)?;
    let source = match input {
        Some(ImageSource::Output(source)) => Source::Document {
            source,
            info: info.clone(),
        },
        Some(ImageSource::Asset(asset)) => {
            crate::placement::source_from_asset(snapshot, asset, &info)?
        }
        None => Source::Unassigned { info: info.clone() },
    };
    let node = graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .ok_or("Missing input node")?;
    let mut config = node.prepare()?.config().clone();
    match &mut config {
        OperationConfig::Read {
            source: current, ..
        } => *current = source,
        OperationConfig::Solid { .. } if node.animation.is_empty() => {
            config = OperationConfig::Read {
                options: Default::default(),
                source,
                start: Time::ZERO,
                source_start: Time::ZERO,
                duration: info.time(info.frames)?,
            }
        }
        _ => return Err("Input is no longer a replaceable source".into()),
    }
    node.set_configuration(config)?;
    let mut updated = graph.document(document.id)?;
    updated.extensions = document.extensions.clone();
    Ok(updated)
}
