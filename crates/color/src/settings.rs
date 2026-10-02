//! Persistable color contracts. Missing project settings mean legacy, never ACES.
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
pub type Extensions = BTreeMap<String, serde_json::Value>;
pub const PROJECT_KEY: &str = "fold.color";
pub const INPUT_KEY: &str = "fold.color.input";
pub const OUTPUT_KEY: &str = "fold.color.output";
pub const SRGB_INPUT: &str = "sRGB Encoded Rec.709 (sRGB)";
pub const VIDEO_INPUT: &str = "Camera Rec.709";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ConfigSource {
    Bundled {
        sha256: String,
    },
    External {
        path: std::path::PathBuf,
        resources: BTreeMap<String, String>,
    },
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectColor {
    pub version: u32,
    pub working_space: String,
    pub ocio_version: String,
    pub config: ConfigSource,
    #[serde(flatten)]
    pub extensions: Extensions,
}
impl Default for ProjectColor {
    fn default() -> Self {
        Self {
            version: 1,
            working_space: crate::WORKING_SPACE.into(),
            ocio_version: crate::OCIO_VERSION.into(),
            config: ConfigSource::Bundled {
                sha256: crate::BUNDLED_SHA256.into(),
            },
            extensions: Extensions::new(),
        }
    }
}
impl ProjectColor {
    pub fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.working_space != crate::WORKING_SPACE
            || self.ocio_version != crate::OCIO_VERSION
        {
            return Err("Unsupported project color configuration".into());
        }
        match &self.config {
            ConfigSource::Bundled { sha256 } if sha256 != crate::BUNDLED_SHA256 => {
                Err("Bundled color configuration identity mismatch".into())
            }
            ConfigSource::External { path, resources }
                if !path.is_absolute() || resources.is_empty() =>
            {
                Err(
                    "External color configuration requires an absolute path and resource identity"
                        .into(),
                )
            }
            _ => Ok(()),
        }
    }
    pub fn load(&self, runtime: &crate::Runtime) -> Result<crate::Config, String> {
        self.validate()?;
        match &self.config {
            ConfigSource::Bundled { .. } => runtime.bundled(),
            ConfigSource::External { path, resources } => {
                let config = runtime.external(path)?;
                if &config.identity().resources != resources {
                    return Err("Color configuration resources changed".into());
                }
                config.conversion(crate::WORKING_SPACE, crate::WORKING_SPACE)?;
                config.display(crate::WORKING_SPACE, &Default::default())?;
                Ok(config)
            }
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputTransform {
    pub display: String,
    pub view: String,
    #[serde(default)]
    pub look: Option<String>,
    #[serde(flatten)]
    pub extensions: Extensions,
}
impl Default for OutputTransform {
    fn default() -> Self {
        Self {
            display: "Rec.1886 Rec.709 - Display".into(),
            view: "ACES 1.0 - SDR Video".into(),
            look: None,
            extensions: Extensions::new(),
        }
    }
}
impl OutputTransform {
    pub fn display_transform(&self) -> crate::DisplayTransform {
        crate::DisplayTransform {
            display: self.display.clone(),
            view: self.view.clone(),
            look: self.look.clone(),
        }
    }
    pub fn label(&self) -> String {
        let display = self
            .display
            .strip_suffix(" - Display")
            .unwrap_or(&self.display);
        let display = if display == "Rec.1886 Rec.709" {
            "Rec.709"
        } else {
            display
        };
        let base = if self.view == "ACES 1.0 - SDR Video" {
            display.into()
        } else {
            format!("{display} / {}", self.view)
        };
        self.look
            .as_ref()
            .map_or(base.clone(), |look| format!("{base} / {look}"))
    }
}
#[derive(Clone, Debug, Default)]
pub struct Choices {
    pub inputs: Vec<String>,
    pub outputs: Vec<OutputTransform>,
    pub error: Option<String>,
}
impl Choices {
    pub fn from_config(config: &crate::Config) -> Result<Self, String> {
        let catalog = config.catalog()?;
        let mut outputs = Vec::new();
        for (display, views) in catalog.displays {
            for view in views {
                outputs.push(OutputTransform {
                    display: display.clone(),
                    view: view.clone(),
                    look: None,
                    extensions: Extensions::new(),
                });
                for look in &catalog.looks {
                    let choice = OutputTransform {
                        display: display.clone(),
                        view: view.clone(),
                        look: Some(look.clone()),
                        extensions: Extensions::new(),
                    };
                    if config
                        .display(crate::WORKING_SPACE, &choice.display_transform())
                        .is_ok()
                    {
                        outputs.push(choice);
                    }
                }
            }
        }
        let mut inputs = catalog.spaces;
        inputs.sort_by_key(|space| {
            (
                match space.as_str() {
                    VIDEO_INPUT => 0,
                    SRGB_INPUT => 1,
                    crate::WORKING_SPACE => 2,
                    _ => 3,
                },
                space.clone(),
            )
        });
        outputs.sort_by_key(|output| {
            (
                !(matches!(
                    output.display.as_str(),
                    "Rec.1886 Rec.709 - Display" | "sRGB - Display"
                ) && output.view == "ACES 1.0 - SDR Video"
                    && output.look.is_none()),
                match output.display.as_str() {
                    "Rec.1886 Rec.709 - Display" => 0,
                    "sRGB - Display" => 1,
                    _ => 2,
                },
                output.view != "ACES 1.0 - SDR Video",
                output.look.is_some(),
                output.label(),
            )
        });
        Ok(Self {
            inputs,
            outputs,
            error: None,
        })
    }
}
