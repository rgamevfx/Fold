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
    if let Source::Output(reference) = &source
        && request.graph.is_none()
    {
        let name = snapshot
            .state()
            .organization
            .items
            .iter()
            .find(|item| item.id == fold_project::ItemId::Document(reference.document))
            .map(|item| item.name.clone())
            .unwrap_or_else(|| "Composition".into());
        crate::authoring::composition::place(
            &mut motion,
            reference.clone(),
            info,
            request.at,
            request.source_start,
            duration,
            name,
        )?;
        let mut document = motion.document(request.target)?;
        document.extensions = old.extensions.clone();
        return Ok(document);
    }
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
    if let Some(target) = &request.graph {
        source_node.position = target.position;
        let id = source_node.id;
        if let Some(group) = target.group {
            motion
                .groups
                .get_mut(&group)
                .ok_or("Missing destination group")?
                .graph
                .nodes
                .push(source_node);
        } else {
            if let Some(object) = target.object {
                let scene = motion.scene.as_ref().ok_or("Missing destination scene")?;
                let object = scene
                    .objects
                    .iter()
                    .find(|o| o.id == object)
                    .ok_or("Missing destination object")?;
                if object.locked {
                    return Err("Destination object is locked".into());
                }
            }
            motion.graph.nodes.push(source_node);
            if let Some(object) = target.object {
                crate::authoring::network_scope::include(&mut motion, object, [id]);
            }
        }
        let mut document = motion.document(request.target)?;
        document.extensions = old.extensions.clone();
        return Ok(document);
    }
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
    let item_id = match &request.source {
        PlacementSource::Asset(id) => fold_project::ItemId::Asset(*id),
        PlacementSource::Output(reference) => fold_project::ItemId::Document(reference.document),
    };
    item.name = fold_platform::browser::item_name(snapshot, item_id, "Image Card");
    item.start = request.at;
    item.end = request
        .at
        .checked_add(duration)
        .map_err(|e| e.to_string())?;
    let mut document = motion.document(request.target)?;
    document.extensions = old.extensions.clone();
    Ok(document)
}
