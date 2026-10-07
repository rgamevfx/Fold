//! Public image-source sockets for Motion graphs, using stable node IDs.
use crate::{
    Motion,
    nodes::image::{Entry, Settings},
};
use fold_foundation::{ObjectId, Time};
use fold_platform::{image_source::Source, layer_graph::ImageInput};
use fold_project::Document;
pub(crate) fn describe(motion: &Motion) -> Result<Vec<ImageInput>, String> {
    let mut inputs = vec![];
    for node in &motion.graph.nodes {
        if node.kind == "fold.motion.image" {
            let settings = node.settings::<Settings>()?;
            // Multi-image collections retain their own internal selection semantics.
            if settings.sources.len() <= 1 {
                inputs.push(ImageInput {
                    id: serde_json::to_string(&node.id).unwrap(),
                    label: format!("Image source {}", inputs.len() + 1),
                    source: settings.sources.first().map(|e| e.source.clone()),
                });
            }
        }
    }
    Ok(inputs)
}
pub(crate) fn bind(
    document: &Document,
    port: &str,
    source: Option<Source>,
    info: fold_media::VideoInfo,
) -> Result<Document, String> {
    let id: ObjectId = serde_json::from_str(port).map_err(|e| e.to_string())?;
    let mut motion = Motion::from_document(document)?;
    let node = crate::authoring::node(&mut motion, id)?;
    if node.kind != "fold.motion.image" {
        return Err("Not an image source node".into());
    }
    let mut settings = node.settings::<Settings>()?;
    if settings.sources.len() > 1 {
        return Err("Edit an image collection inside its graph".into());
    }
    let previous = settings.sources.pop();
    if let Some(source) = source {
        settings.sources.push(Entry {
            source,
            info,
            offset: previous.as_ref().map_or(Time::ZERO, |e| e.offset),
            extensions: previous.map(|e| e.extensions).unwrap_or_default(),
        });
    }
    node.settings = serde_json::to_value(settings).map_err(|e| e.to_string())?;
    let mut updated = motion.document(document.id)?;
    updated.extensions = document.extensions.clone();
    Ok(updated)
}
