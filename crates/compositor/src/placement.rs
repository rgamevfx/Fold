//! Project sources enter as editable Read nodes, without changing existing wiring.
use crate::{Composite, Node, Parameters, Source};
use fold_foundation::DocumentId;
use fold_platform::browser::{Placement, PlacementSource, asset_metadata};
use fold_project::{Document, Snapshot};

pub fn create(id: DocumentId) -> Result<Document, String> {
    let solid = Node::new(Parameters::Solid { rgba: [0.; 4] }, vec![]);
    let output = Node::new(Parameters::Output, vec![solid.id]);
    let mut graph = Composite {
        info: fold_media::VideoInfo {
            width: 1280,
            height: 720,
            rate: [24, 1],
            frames: 120,
        },
        output: output.id,
        nodes: vec![solid, output],
        extensions: Default::default(),
    };
    graph.auto_layout()?;
    graph.document(id)
}
pub fn place(
    snapshot: &Snapshot,
    request: &Placement,
    info: Option<fold_media::VideoInfo>,
) -> Result<Document, String> {
    if request.audio_only || request.track.is_some() {
        return Err(
            "compositor placement requires an image source, not an audio/track target".into(),
        );
    }
    let old = snapshot
        .state()
        .documents
        .get(&request.target)
        .ok_or("missing composite")?;
    let mut graph = Composite::from_document(old)?;
    let source = match &request.source {
        PlacementSource::Asset(asset) => source_from_profile(
            *asset,
            asset_metadata(snapshot, *asset)?.profile,
            &graph.info,
        )?,
        PlacementSource::Output(source) => Source::Document {
            source: source.clone(),
            info: info.ok_or("missing output capability")?,
        },
    };
    let duration = request.range(source.info().time(source.info().frames)?)?;
    let mut node = Node::new(
        Parameters::Read {
            source,
            start: request.at,
            source_start: request.source_start,
            duration,
        },
        vec![],
    );
    node.position = Some([0., graph.nodes.len() as f32 * 100.]);
    graph.nodes.push(node);
    let mut document = graph.document(request.target)?;
    document.extensions = old.extensions.clone();
    Ok(document)
}

/// Shared by placement and the application's worker-side file picker.
pub fn source_from_profile(
    asset: fold_foundation::AssetId,
    profile: fold_media::ingest::SourceProfile,
    composition: &fold_media::VideoInfo,
) -> Result<Source, String> {
    match profile {
        fold_media::ingest::SourceProfile::Video(info) => Ok(Source::Asset { asset, info }),
        fold_media::ingest::SourceProfile::Exr(image) => Ok(Source::Exr {
            color_layers: image
                .channels
                .iter()
                .filter(|c| c.color)
                .filter_map(|c| c.name.rsplit_once('.').map(|(layer, _)| layer.to_owned()))
                .filter(|layer| {
                    // RGB spelling alone cannot distinguish lighting from
                    // normals, positions or other utility AOVs.
                    layer == "rgba"
                        && ["red", "green", "blue"].iter().all(|component| {
                            image
                                .channels
                                .iter()
                                .any(|c| c.name == format!("{layer}.{component}"))
                        })
                })
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect(),
            asset,
            info: fold_media::VideoInfo {
                width: image.dimensions[0],
                height: image.dimensions[1],
                rate: composition.rate,
                frames: composition.frames,
            },
            image,
        }),
        _ => Err("Read requires video or OpenEXR".into()),
    }
}
