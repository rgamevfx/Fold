//! Project defaults and document format capability shared by settings surfaces.
use fold_media::VideoInfo;
use fold_project::{Metadata, Snapshot};
use serde::{Deserialize, Serialize};
pub const PROJECT_KEY: &str = "fold.project.defaults";
pub const PROJECT_COMMAND: &str = "fold.app.project-settings";
pub const DOCUMENT_COMMAND: &str = "fold.app.document-settings";
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectDefaults {
    pub format: VideoInfo,
    /// Reserved for future Cache-node bake outputs; never the application cache.
    pub cache_folder: String,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Default for ProjectDefaults {
    fn default() -> Self {
        Self {
            format: VideoInfo {
                width: 1280,
                height: 720,
                rate: [24, 1],
                frames: 120,
            },
            cache_folder: String::new(),
            extensions: Default::default(),
        }
    }
}
impl ProjectDefaults {
    pub fn read(snapshot: &Snapshot) -> Result<Self, String> {
        let value: Self = snapshot
            .state()
            .settings
            .get(PROJECT_KEY)
            .map(|v| serde_json::from_value(v.clone()).map_err(|e| e.to_string()))
            .transpose()?
            .unwrap_or_default();
        value.format.validate()?;
        Ok(value)
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentSettings {
    pub format: VideoInfo,
    /// Timeline only: None means follow the content end, Some is an explicit output duration.
    pub output_duration: Option<fold_foundation::Time>,
    pub timeline: bool,
}
impl DocumentSettings {
    pub fn fixed(format: VideoInfo) -> Self {
        Self {
            format,
            output_duration: None,
            timeline: false,
        }
    }
}
#[derive(Serialize, Deserialize)]
pub struct ProjectEdit {
    pub defaults: ProjectDefaults,
    /// None retains the current color configuration; empty selects the bundled configuration.
    pub color_config: Option<String>,
}
#[derive(Serialize, Deserialize)]
pub struct DocumentEdit {
    pub document: fold_foundation::DocumentId,
    pub settings: DocumentSettings,
}
