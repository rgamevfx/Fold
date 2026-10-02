//! Worker-only linked-source inspection. No source copies or project models.
use crate::{Cancel, VideoInfo, WaveInfo, fingerprint, process::Process};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub const METADATA_KEY: &str = "fold.media.source.v1";
pub const INTERPRETATION_KEY: &str = "fold.media.input.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceProfile {
    Video(VideoInfo),
    Wave(WaveInfo),
    Ppm { dimensions: [u32; 2] },
}

/// Probed facts, not a color assignment. FFprobe stream records retain native
/// color, codec/profile, time_base/start/duration, layout and side-data verbatim.
/// Missing fields remain unknown, never defaulted to sRGB.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceMetadata {
    pub profile: SourceProfile,
    pub streams: Vec<StreamMetadata>,
    pub raw_format: serde_json::Value,
    pub timing_policy: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct StreamMetadata {
    pub index: u32,
    pub kind: String,
    pub facts: serde_json::Value,
    pub bit_depth: Option<u8>,
    pub planes: Option<u8>,
    pub alpha: Option<String>,
}

/// Separate user assignment; production OCIO processing is phase 16.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputInterpretation {
    pub color_space: Option<String>,
    pub alpha: Option<String>,
    pub provenance: String,
}

#[derive(Clone, Debug)]
pub struct LinkedSource {
    pub path: PathBuf,
    pub fingerprint: String,
    pub metadata: SourceMetadata,
}

pub fn inspect_linked(path: &Path, cancel: &Cancel) -> Result<LinkedSource, String> {
    cancel.check()?;
    let path = path
        .canonicalize()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let hash = fingerprint(&path, cancel)?;
    let extension = path
        .extension()
        .and_then(|s| s.to_str())
        .unwrap_or("")
        .to_ascii_lowercase();
    let profile = match extension.as_str() {
        "wav" => SourceProfile::Wave(crate::inspect_wave(&path, cancel)?.info),
        "mp4" => SourceProfile::Video(crate::inspect(&path, cancel)?.info),
        "ppm" => {
            let mut bytes = Vec::new();
            std::fs::File::open(&path)
                .map_err(|e| e.to_string())?
                .take(crate::MAX_SOURCE_BYTES as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| e.to_string())?;
            SourceProfile::Ppm {
                dimensions: crate::RgbImage::decode_ppm(&bytes)?.dimensions(),
            }
        }
        _ => return Err("unsupported linked media: expected MP4, WAVE or P6 PPM".into()),
    };
    let (streams, raw_format) = if matches!(profile, SourceProfile::Ppm { .. }) {
        (
            vec![StreamMetadata {
                index: 0,
                kind: "video".into(),
                bit_depth: Some(8),
                planes: Some(1),
                alpha: Some("opaque".into()),
                facts: serde_json::json!({
                    "codec_name": "ppm", "pix_fmt": "rgb24", "bits_per_raw_sample": 8,
                    "alpha": "opaque", "interpretation": "legacy P6 sRGB adapter"
                }),
            }],
            serde_json::Value::Null,
        )
    } else {
        let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        let mut command = Command::new("ffprobe");
        command
            .args([
                "-v",
                "error",
                "-show_streams",
                "-show_format",
                "-of",
                "json",
            ])
            .arg(&path)
            .stdin(Stdio::null())
            .stdout(Stdio::from(output.reopen().map_err(|e| e.to_string())?));
        Process::spawn(
            command,
            cancel.clone(),
            Some((output.path().into(), 1024 * 1024)),
        )?
        .finish()?;
        let raw: serde_json::Value = serde_json::from_reader(
            output
                .reopen()
                .map_err(|e| e.to_string())?
                .take(1024 * 1024),
        )
        .map_err(|e| e.to_string())?;
        let records = raw["streams"].as_array().ok_or("missing stream metadata")?;
        if records.len() > 32 {
            return Err("too many media streams".into());
        }
        let streams = records
            .iter()
            .map(|facts| {
                Ok(StreamMetadata {
                    index: facts["index"]
                        .as_u64()
                        .and_then(|v| u32::try_from(v).ok())
                        .ok_or("invalid stream index")?,
                    kind: facts["codec_type"]
                        .as_str()
                        .ok_or("missing stream kind")?
                        .into(),
                    bit_depth: match facts["pix_fmt"].as_str() {
                        Some("yuv420p") => Some(8),
                        _ => facts["bits_per_sample"]
                            .as_u64()
                            .and_then(|v| u8::try_from(v).ok())
                            .filter(|v| *v != 0),
                    },
                    planes: (facts["pix_fmt"] == "yuv420p").then_some(3),
                    alpha: (facts["codec_type"] == "video").then(|| "opaque".into()),
                    facts: facts.clone(),
                })
            })
            .collect::<Result<Vec<_>, String>>()?;
        (streams, raw["format"].clone())
    };
    if fingerprint(&path, cancel)? != hash {
        return Err("source changed during inspection".into());
    }
    cancel.check()?;
    let timing_policy = match &profile {
        SourceProfile::Video(_) => "verified exact CFR, zero origin",
        SourceProfile::Wave(_) => "exact sample frames, zero origin",
        SourceProfile::Ppm { .. } => "still image",
    }
    .into();
    Ok(LinkedSource {
        path,
        fingerprint: hash,
        metadata: SourceMetadata {
            profile,
            streams,
            raw_format,
            timing_policy,
        },
    })
}

impl SourceMetadata {
    /// Conservative relink compatibility: no duration, stream or decode-contract
    /// changes for authored uses. Container tags/location/bitrate may differ.
    pub fn compatible_with(&self, other: &Self) -> bool {
        const FIELDS: &[&str] = &[
            "codec_name",
            "codec_type",
            "profile",
            "pix_fmt",
            "width",
            "height",
            "sample_aspect_ratio",
            "field_order",
            "time_base",
            "start_pts",
            "duration_ts",
            "r_frame_rate",
            "sample_rate",
            "channels",
            "channel_layout",
            "sample_fmt",
            "bits_per_sample",
            "bits_per_raw_sample",
            "color_primaries",
            "color_transfer",
            "color_space",
            "color_range",
            "chroma_location",
            "side_data_list",
        ];
        self.profile == other.profile
            && self.timing_policy == other.timing_policy
            && self.streams.len() == other.streams.len()
            && self.streams.iter().zip(&other.streams).all(|(a, b)| {
                a.index == b.index
                    && a.kind == b.kind
                    && a.bit_depth == b.bit_depth
                    && a.planes == b.planes
                    && a.alpha == b.alpha
                    && FIELDS.iter().all(|k| a.facts[*k] == b.facts[*k])
            })
    }
}
