use fold_foundation::{AssetId, DocumentId, Time};
use fold_media::{VideoInfo, VideoSource};
use fold_platform::VideoProvider;
use fold_project::{Document, Revision, Snapshot};
use fold_render::{ImageOp, RenderGraph};
use serde::{Deserialize, Serialize};

pub const VIDEO_LAYERS: &str = "fold.timeline.video-layers";
/// Minimal media-slice authoring: one or two synchronized layers. Independent
/// of compositor models; richer timeline clip editing follows in phase 7.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoLayers {
    pub assets: Vec<AssetId>,
    pub info: VideoInfo,
    pub foreground_opacity: f32,
    #[serde(flatten)]
    pub extensions: fold_project::Metadata,
}
impl VideoLayers {
    fn validate(&self) -> Result<(), String> {
        self.info.validate()?;
        if ["assets", "info", "foreground_opacity"]
            .iter()
            .any(|key| self.extensions.contains_key(*key))
        {
            return Err("video extensions cannot shadow authoring fields".into());
        }
        if !(1..=2).contains(&self.assets.len())
            || !self.foreground_opacity.is_finite()
            || !(0.0..=1.0).contains(&self.foreground_opacity)
        {
            return Err("expected one/two video layers and opacity in 0..=1".into());
        }
        Ok(())
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        Ok(Document {
            id,
            type_id: VIDEO_LAYERS.into(),
            package_id: super::PACKAGE.into(),
            schema_version: 1,
            revision: Revision::default(),
            dependencies: vec![],
            assets: self.assets.clone(),
            payload: serde_json::to_vec(self).map_err(|e| e.to_string())?,
            extensions: Default::default(),
        })
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        if document.package_id != super::PACKAGE
            || document.type_id != VIDEO_LAYERS
            || document.schema_version != 1
            || !document.dependencies.is_empty()
            || document.payload.len() > 65536
        {
            return Err("unsupported video layer document".into());
        }
        let value: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        value.validate()?;
        if value.assets != document.assets {
            return Err("video asset declarations disagree with payload".into());
        }
        Ok(value)
    }
}
pub struct VideoLayersProvider;
impl VideoProvider for VideoLayersProvider {
    fn package_id(&self) -> &'static str {
        super::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        VIDEO_LAYERS
    }
    fn compile(
        &self,
        _: &Document,
        _: &str,
        _: Time,
        _: u32,
        _: u32,
    ) -> Result<RenderGraph, String> {
        Err("video layers require snapshot asset resolution".into())
    }
    fn compile_snapshot(
        &self,
        snapshot: &Snapshot,
        document: &Document,
        output: &str,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String> {
        if output != "video" {
            return Err("unsupported video output".into());
        }
        let layers = VideoLayers::from_document(document)?;
        layers.info.frame_at(time)?;
        let mut nodes = Vec::new();
        for id in &layers.assets {
            let asset = snapshot
                .state()
                .assets
                .get(id)
                .ok_or("missing video asset")?;
            nodes.push(ImageOp::Video {
                source: VideoSource {
                    path: asset.location.clone().into(),
                    fingerprint: asset.fingerprint.clone(),
                    info: layers.info.clone(),
                },
                time,
            });
        }
        if nodes.len() == 2 {
            nodes.push(ImageOp::Opacity {
                input: 1,
                opacity: layers.foreground_opacity,
            });
            nodes.push(ImageOp::Over {
                foreground: 2,
                background: 0,
            });
        }
        Ok(RenderGraph {
            width,
            height,
            output: nodes.len() - 1,
            nodes,
        })
    }
}
