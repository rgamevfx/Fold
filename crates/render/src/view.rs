//! Viewer-only channel selection. This identity never changes delivery settings.
use crate::channels::ChannelName;
use serde::{Deserialize, Serialize};
use std::hash::{Hash, Hasher};

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
#[serde(try_from = "[f32; 2]", into = "[f32; 2]")]
pub struct Range([f32; 2]);
impl Range {
    pub fn new(black: f32, white: f32) -> Result<Self, String> {
        if !black.is_finite()
            || !white.is_finite()
            || white <= black
            || !(white - black).is_finite()
        {
            return Err("Display range requires finite black < white".into());
        }
        Ok(Self([
            if black == 0. { 0. } else { black },
            if white == 0. { 0. } else { white },
        ]))
    }
    pub fn values(self) -> [f32; 2] {
        self.0
    }
}
impl Default for Range {
    fn default() -> Self {
        Self([0., 1.])
    }
}
impl TryFrom<[f32; 2]> for Range {
    type Error = String;
    fn try_from(v: [f32; 2]) -> Result<Self, String> {
        Self::new(v[0], v[1])
    }
}
impl From<Range> for [f32; 2] {
    fn from(value: Range) -> Self {
        value.0
    }
}
impl PartialEq for Range {
    fn eq(&self, other: &Self) -> bool {
        self.0.map(f32::to_bits) == other.0.map(f32::to_bits)
    }
}
impl Eq for Range {}
impl Hash for Range {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.0.map(f32::to_bits).hash(state);
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum View {
    Rgb { layer: String },
    Channel { name: ChannelName, range: Range },
}
impl Default for View {
    fn default() -> Self {
        Self::Rgb {
            layer: "rgba".into(),
        }
    }
}
pub type Selection = [Option<ChannelName>; 4];
impl View {
    pub fn selection(&self) -> Result<Selection, String> {
        match self {
            Self::Rgb { layer } => Ok([
                Some(format!("{layer}.red").try_into()?),
                Some(format!("{layer}.green").try_into()?),
                Some(format!("{layer}.blue").try_into()?),
                None,
            ]),
            Self::Channel { name, .. } => Ok([
                Some(name.clone()),
                Some(name.clone()),
                Some(name.clone()),
                None,
            ]),
        }
    }
    pub fn label(&self) -> String {
        match self {
            Self::Rgb { layer } => format!("{layer} · RGB"),
            Self::Channel { name, .. } => name.to_string(),
        }
    }
}
