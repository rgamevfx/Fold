//! Still-image placement through the shared browser transaction capability.
use fold_foundation::ObjectId;
use fold_motion_canvas::{
    editing::{self, Edit},
    model::{Canvas, Content},
};
use fold_platform::browser::{Placement, PlacementSource};
use fold_project::{EditBatch, ItemId, Snapshot};

pub(crate) fn place(snapshot: &Snapshot, request: &Placement) -> Result<EditBatch, String> {
    if request.audio_only
        || request.track.is_some()
        || request.new_track
        || request.source_start != fold_foundation::Time::ZERO
        || request.duration.is_some()
        || !matches!(request.mode, fold_platform::browser::PlacementMode::Place)
        || request
            .graph
            .as_ref()
            .is_some_and(|graph| graph.object.is_some())
    {
        return Err(
            "Motion image placement requires an image asset and an object/group destination".into(),
        );
    }
    let PlacementSource::Asset(asset) = request.source else {
        return Err(
            "Motion imports still-image assets; compose document outputs in Compositing".into(),
        );
    };
    let metadata = fold_platform::browser::asset_metadata(snapshot, asset)?;
    let dimensions = crate::image_source::dimensions(&metadata.profile)?;
    crate::image_source::interpretation(
        snapshot
            .state()
            .assets
            .get(&asset)
            .ok_or("Missing image asset")?,
    )?;
    let original = snapshot
        .state()
        .documents
        .get(&request.target)
        .ok_or("Missing Motion document")?;
    let canvas = Canvas::from_document(original)?;
    if request.at < fold_foundation::Time::ZERO || request.at >= canvas.duration {
        return Err("Image placement time is outside the Motion document".into());
    }
    if request
        .canvas_position
        .is_some_and(|position| position.iter().any(|value| !value.is_finite()))
    {
        return Err("Image drop position must be finite".into());
    }
    let parent = request.graph.as_ref().and_then(|graph| graph.group);
    let index = match parent {
        None => canvas.roots.len(),
        Some(parent) => match &canvas
            .objects
            .get(&parent)
            .ok_or("Missing Motion group")?
            .content
        {
            Content::Group { children, .. } => children.len(),
            _ => return Err("Image placement parent must be a group".into()),
        },
    };
    let mut image = fold_motion_canvas::tools::image(asset, dimensions)?;
    let scale = (canvas.width as f64 / dimensions[0] as f64)
        .min(canvas.height as f64 / dimensions[1] as f64)
        .min(1.);
    let size = dimensions.map(|value| value as f64 * scale);
    if let Content::Image { width, height, .. } = &mut image.content {
        width.value = size[0];
        height.value = size[1];
    }
    let mut center = request
        .canvas_position
        .unwrap_or([canvas.width as f64 / 2., canvas.height as f64 / 2.]);
    if let Some(parent) = parent {
        center = fold_motion_canvas::tools::transforms(&canvas, request.at)?[&parent]
            .inverse()?
            .point(center);
    }
    image.transform.x.value = center[0] - size[0] / 2.;
    image.transform.y.value = center[1] - size[1] / 2.;
    image.graph_position = request.graph.as_ref().map(|graph| graph.position);
    if let Some(item) = snapshot
        .state()
        .organization
        .items
        .iter()
        .find(|item| item.id == ItemId::Asset(asset))
    {
        image.name = item.name.clone();
    }
    editing::stage(
        snapshot,
        request.target,
        &[Edit::Insert {
            id: ObjectId::new(),
            object: Box::new(image),
            parent,
            index,
        }],
    )
}
