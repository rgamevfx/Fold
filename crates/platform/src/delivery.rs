//! Feature-independent, snapshot-pinned image sequence delivery settings.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Precision {
    Half,
    Float,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Compression {
    None,
    Rle,
    Zip1,
    Zip16,
    Piz,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Channels {
    Rgba,
    Rgb,
    Alpha,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Settings {
    pub destination: String,
    pub channels: Channels,
    pub precision: Precision,
    pub compression: Compression,
    pub color_space: String,
    pub first: u32,
    pub last: u32,
    pub step: u32,
    pub premultiplied: bool,
    pub create_folders: bool,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            destination: String::new(),
            channels: Channels::Rgba,
            precision: Precision::Half,
            compression: Compression::Zip1,
            color_space: "ACEScg".into(),
            first: 0,
            last: 0,
            step: 1,
            premultiplied: true,
            create_folders: true,
        }
    }
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        if self.first > self.last
            || self.step == 0
            || self.last > i32::MAX as u32
            || self.destination.len() > 4096
            || self.color_space.is_empty()
            || self.color_space.len() > 256
        {
            return Err("Invalid Write range, step or destination".into());
        }
        Ok(())
    }
    pub fn path(&self, frame: u32) -> Result<std::path::PathBuf, String> {
        self.validate()?;
        let pattern = &self.destination;
        if pattern.is_empty() {
            return Err("Choose a destination".into());
        }
        let path = if let Some(first) = pattern.find('#') {
            let count = pattern[first..].chars().take_while(|c| *c == '#').count();
            if count > 12 || pattern[first + count..].contains('#') {
                return Err("Use one frame-number field, such as comp.####.exr".into());
            }
            format!(
                "{}{:0width$}{}",
                &pattern[..first],
                frame,
                &pattern[first + count..],
                width = count
            )
        } else {
            if self.first != self.last {
                return Err("Sequences require a frame-number field, such as comp.####.exr".into());
            }
            pattern.clone()
        };
        let path = std::path::PathBuf::from(path);
        if !path.is_absolute()
            || !path
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("exr"))
        {
            return Err("Choose an absolute .exr destination".into());
        }
        Ok(path)
    }
}
