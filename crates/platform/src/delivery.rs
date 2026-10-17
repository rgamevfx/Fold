//! Feature-independent, snapshot-pinned file delivery settings.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum FileType {
    #[default]
    OpenExr,
    QuickTime,
    Mp4,
}
impl FileType {
    pub const ALL: [Self; 3] = [Self::OpenExr, Self::QuickTime, Self::Mp4];
    pub fn label(self) -> &'static str {
        match self {
            Self::OpenExr => "OpenEXR sequence",
            Self::QuickTime => "QuickTime MOV",
            Self::Mp4 => "MP4",
        }
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::OpenExr => "exr",
            Self::QuickTime => "mov",
            Self::Mp4 => "mp4",
        }
    }
    pub fn default_filename(self) -> &'static str {
        match self {
            Self::OpenExr => "comp.####.exr",
            Self::QuickTime => "comp.mov",
            Self::Mp4 => "comp.mp4",
        }
    }
}
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
    #[serde(default)]
    pub file_type: FileType,
    #[serde(default)]
    pub prores: fold_media::encoding::ProRes,
    #[serde(default)]
    pub h264: fold_media::encoding::H264,
    #[serde(default)]
    pub movie_color: crate::color::OutputTransform,
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
            file_type: FileType::OpenExr,
            prores: Default::default(),
            h264: Default::default(),
            movie_color: Default::default(),
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
    pub fn movie(&self) -> Option<fold_media::encoding::Movie> {
        match self.file_type {
            FileType::OpenExr => None,
            FileType::QuickTime => Some(fold_media::encoding::Movie::ProRes(self.prores)),
            FileType::Mp4 => Some(fold_media::encoding::Movie::H264(self.h264)),
        }
    }
    /// Change only the destination syntax; all format-specific options survive.
    pub fn set_file_type(&mut self, file_type: FileType) {
        if self.file_type == file_type {
            return;
        }
        if !self.destination.is_empty() {
            let path = std::path::Path::new(&self.destination);
            let stem = path.file_stem().unwrap_or_default().to_string_lossy();
            let mut stem = stem.into_owned();
            if file_type != FileType::OpenExr {
                stem = stem.replace('#', "");
                stem = stem.trim_end_matches(['.', '_', '-']).to_owned();
            } else if !stem.contains('#') {
                stem.push_str(".####");
            }
            self.destination = path
                .with_file_name(format!("{stem}.{}", file_type.extension()))
                .to_string_lossy()
                .into_owned();
        }
        self.file_type = file_type;
    }
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
        if let Some(movie) = self.movie() {
            movie.validate()?;
        }
        Ok(())
    }
    pub fn path(&self, frame: u32) -> Result<std::path::PathBuf, String> {
        self.validate()?;
        let pattern = &self.destination;
        if pattern.is_empty() {
            return Err("Choose a destination".into());
        }
        if let Some(movie) = self.movie() {
            let path = std::path::PathBuf::from(pattern);
            if !path.is_absolute() || pattern.contains('#') {
                return Err("Movies require an absolute filename without frame numbering".into());
            }
            movie.validate_path(&path)?;
            return Ok(path);
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

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn format_switches_keep_options_and_use_container_or_sequence_paths() {
        let mut settings = Settings {
            destination: "/tmp/shot.######.exr".into(),
            last: 20,
            precision: Precision::Float,
            compression: Compression::Piz,
            ..Default::default()
        };
        settings.h264.crf = 23;
        settings.prores.profile = fold_media::encoding::ProResProfile::Hq;
        settings.set_file_type(FileType::QuickTime);
        assert_eq!(
            settings.path(0).unwrap(),
            std::path::Path::new("/tmp/shot.mov")
        );
        assert!(!settings.movie().unwrap().preserves_alpha());
        settings.set_file_type(FileType::Mp4);
        assert_eq!(
            settings.path(20).unwrap(),
            std::path::Path::new("/tmp/shot.mp4")
        );
        settings.set_file_type(FileType::OpenExr);
        assert_eq!(
            settings.path(12).unwrap(),
            std::path::Path::new("/tmp/shot.0012.exr")
        );
        assert_eq!(settings.precision, Precision::Float);
        assert_eq!(settings.compression, Compression::Piz);
        assert_eq!(settings.h264.crf, 23);
        assert_eq!(
            settings.prores.profile,
            fold_media::encoding::ProResProfile::Hq
        );
        let saved = serde_json::to_vec(&settings).unwrap();
        assert_eq!(
            serde_json::from_slice::<Settings>(&saved).unwrap(),
            settings
        );
        settings.set_file_type(FileType::Mp4);
        settings.destination = "/tmp/shot.mov".into();
        assert!(settings.path(0).unwrap_err().contains(".mp4"));
        settings.destination = "/tmp/shot.####.mp4".into();
        assert!(settings.path(0).is_err());
        settings.destination = "/tmp/shot.mp4".into();
        settings.h264.crf = 52;
        assert!(settings.path(0).is_err());
    }
    #[test]
    fn existing_exr_settings_load_without_changing_their_meaning() {
        let settings: Settings = serde_json::from_value(serde_json::json!({
            "destination":"/tmp/old.####.exr", "channels":"Rgba", "precision":"Float",
            "compression":"Piz", "color_space":"ACEScg", "first":1,"last":10,
            "step":2,"premultiplied":false,"create_folders":false
        }))
        .unwrap();
        assert_eq!(settings.file_type, FileType::OpenExr);
        assert_eq!(
            settings.path(3).unwrap(),
            std::path::Path::new("/tmp/old.0003.exr")
        );
        assert_eq!(settings.precision, Precision::Float);
        assert_eq!(settings.step, 2);
        assert!(!settings.premultiplied);
    }
}
