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
        PlacementSource::Asset(asset) => {
            let fold_media::ingest::SourceProfile::Video(info) =
                asset_metadata(snapshot, *asset)?.profile
            else {
                return Err("this compositor provider currently accepts video assets only".into());
            };
            Source::Asset {
                asset: *asset,
                info,
            }
        }
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
