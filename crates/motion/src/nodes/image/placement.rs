//! Undoable Project placement builds the same image/card graph.
use super::*;
pub fn place(
    snapshot: &fold_project::Snapshot,
    request: &fold_platform::browser::Placement,
    info: Option<fold_media::VideoInfo>,
) -> Result<fold_project::Document, String> {
    use fold_platform::browser::PlacementSource;
    if request.audio_only || request.track.is_some() {
        return Err("Motion placement requires an image source".into());
    }
    let old = snapshot
        .state()
        .documents
        .get(&request.target)
        .ok_or("missing Motion document")?;
    let mut motion = crate::Motion::from_document(old)?;
    let (source, info) = match &request.source {
        PlacementSource::Output(reference) => (
            Source::Output(reference.clone()),
            info.ok_or("missing image output capability")?,
        ),
        PlacementSource::Asset(id) => {
            let info = fold_platform::image_source::asset_info(snapshot, *id, &motion.info)?;
            (Source::Asset(*id), info)
        }
    };
    let duration = request.range(info.time(info.frames)?)?;
    let offset = request
        .source_start
        .checked_sub(request.at)
        .map_err(|e| e.to_string())?;
    let mut source_node = Node::new("fold.motion.image")?;
    source_node.settings = serde_json::to_value(Settings {
        sources: vec![Entry {
            source,
            info: info.clone(),
            offset,
            extensions: Default::default(),
        }],
        ..Default::default()
    })
    .map_err(|e| e.to_string())?;
    let object = crate::authoring::scene::create_object(&mut motion, "fold.motion.image_card")?;
    let card = motion
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == object)
        .unwrap();
    card.connect("image", source_node.id, "image");
    let fit = (motion.info.width as f64 / info.width as f64)
        .min(motion.info.height as f64 / info.height as f64)
        .min(1.);
    card.set(
        "size",
        Datum::Vector([info.width as f64 * fit, info.height as f64 * fit]),
    );
    source_node.position = [card.position[0] - 280., card.position[1]];
    motion.graph.nodes.push(source_node);
    let item = motion
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == object)
        .unwrap();
    item.start = request.at;
    item.end = request
        .at
        .checked_add(duration)
        .map_err(|e| e.to_string())?;
    let mut document = motion.document(request.target)?;
    document.extensions = old.extensions.clone();
    Ok(document)
}
