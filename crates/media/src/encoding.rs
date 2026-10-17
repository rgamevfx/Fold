//! Movie encoding choices shared by Write, Delivery and the FFmpeg adapter.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProResProfile {
    Proxy,
    Lt,
    Standard,
    Hq,
    #[default]
    FourFourFourFour,
    Xq,
}
impl ProResProfile {
    pub const ALL: [Self; 6] = [
        Self::Proxy,
        Self::Lt,
        Self::Standard,
        Self::Hq,
        Self::FourFourFourFour,
        Self::Xq,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Proxy => "422 Proxy",
            Self::Lt => "422 LT",
            Self::Standard => "422",
            Self::Hq => "422 HQ",
            Self::FourFourFourFour => "4444",
            Self::Xq => "4444 XQ",
        }
    }
    pub fn ffmpeg(self) -> &'static str {
        match self {
            Self::Proxy => "0",
            Self::Lt => "1",
            Self::Standard => "2",
            Self::Hq => "3",
            Self::FourFourFourFour => "4",
            Self::Xq => "5",
        }
    }
    pub fn supports_alpha(self) -> bool {
        matches!(self, Self::FourFourFourFour | Self::Xq)
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ProRes {
    pub profile: ProResProfile,
    /// Retained when selecting a 422 profile; effective only for 4444 profiles.
    pub alpha: bool,
}
impl Default for ProRes {
    fn default() -> Self {
        Self {
            profile: Default::default(),
            alpha: true,
        }
    }
}
impl ProRes {
    pub fn preserves_alpha(self) -> bool {
        self.alpha && self.profile.supports_alpha()
    }
    pub fn pixel_format(self) -> &'static str {
        if self.preserves_alpha() {
            "yuva444p10le"
        } else if self.profile.supports_alpha() {
            "yuv444p10le"
        } else {
            "yuv422p10le"
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum H264Preset {
    Ultrafast,
    #[default]
    Veryfast,
    Fast,
    Medium,
    Slow,
}
impl H264Preset {
    pub const ALL: [Self; 5] = [
        Self::Ultrafast,
        Self::Veryfast,
        Self::Fast,
        Self::Medium,
        Self::Slow,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Ultrafast => "ultrafast",
            Self::Veryfast => "veryfast",
            Self::Fast => "fast",
            Self::Medium => "medium",
            Self::Slow => "slow",
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct H264 {
    pub crf: u32,
    pub preset: H264Preset,
}
impl Default for H264 {
    fn default() -> Self {
        Self {
            crf: 18,
            preset: Default::default(),
        }
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Movie {
    ProRes(ProRes),
    H264(H264),
}
impl Movie {
    pub fn validate(self) -> Result<(), String> {
        if matches!(self, Self::H264(H264 { crf, .. }) if crf > 51) {
            return Err("H.264 quality must be between 0 and 51".into());
        }
        Ok(())
    }
    pub fn extension(self) -> &'static str {
        match self {
            Self::ProRes(_) => "mov",
            Self::H264(_) => "mp4",
        }
    }
    pub fn preserves_alpha(self) -> bool {
        matches!(self, Self::ProRes(p) if p.preserves_alpha())
    }
    pub fn from_path(path: &std::path::Path) -> Result<Self, String> {
        match path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .as_deref()
        {
            Some("mov") => Ok(Self::ProRes(Default::default())),
            Some("mp4") => Ok(Self::H264(Default::default())),
            _ => Err("Choose .mp4 (H.264) or .mov (ProRes)".into()),
        }
    }
    pub fn validate_path(self, path: &std::path::Path) -> Result<(), String> {
        self.validate()?;
        if !path
            .extension()
            .is_some_and(|e| e.eq_ignore_ascii_case(self.extension()))
        {
            return Err(format!(
                "Selected encoding requires a .{} destination",
                self.extension()
            ));
        }
        Ok(())
    }
}
