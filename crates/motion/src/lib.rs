//! Motion-owned authoring payloads and immutable plan compilation.
use fold_foundation::{DocumentId, Time};
use fold_platform::VideoProvider;
use fold_project::{Document, Revision};
use fold_render::{RenderGraph, SolidPlan};
use serde::{Deserialize, Serialize};

pub const PACKAGE: &str = "fold.motion";
pub const SOLID: &str = "fold.motion.solid";

/// Initial generated source: opaque scene-linear SDR sRGB.
#[derive(Clone, Copy, Serialize, Deserialize)]
pub struct Solid {
    pub rgb: [f32; 3],
}
impl Solid {
    fn validate(self) -> Result<(), String> {
        if self
            .rgb
            .iter()
            .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        {
            Ok(())
        } else {
            Err("solid RGB must be finite and in 0..=1".into())
        }
    }
    pub fn document(self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        Ok(Document {
            id,
            type_id: SOLID.into(),
            package_id: PACKAGE.into(),
            schema_version: 1,
            revision: Revision::default(),
            dependencies: vec![],
            assets: vec![],
            payload: serde_json::to_vec(&self).map_err(|e| e.to_string())?,
            extensions: Default::default(),
        })
    }
}

pub struct SolidProvider;
impl VideoProvider for SolidProvider {
    fn package_id(&self) -> &'static str {
        PACKAGE
    }
    fn type_id(&self) -> &'static str {
        SOLID
    }
    fn compile(
        &self,
        document: &Document,
        output: &str,
        _time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String> {
        if document.package_id != PACKAGE
            || document.type_id != SOLID
            || document.schema_version != 1
        {
            return Err("unsupported solid document identity or schema".into());
        }
        if output != "video" {
            return Err(format!("unsupported solid output: {output}"));
        }
        if !document.dependencies.is_empty() || !document.assets.is_empty() {
            return Err("solid source cannot consume dependencies or assets".into());
        }
        let solid: Solid = serde_json::from_slice(&document.payload)
            .map_err(|e| format!("invalid solid payload: {e}"))?;
        solid.validate()?;
        let [r, g, b] = solid.rgb;
        Ok(SolidPlan {
            width,
            height,
            rgba: [r, g, b, 1.0],
        }
        .into())
    }
}
