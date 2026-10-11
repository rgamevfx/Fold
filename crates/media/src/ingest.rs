//! Worker-only linked-source inspection. No source copies or project models.
use crate::{Cancel, VideoInfo, WaveInfo, fingerprint, process::Process};
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

pub const SUPPORTED_EXTENSIONS: &[&str] = &[
    "mp4", "mov", "mxf", "wav", "mp3", "m4a", "aac", "flac", "ppm", "exr", "jpg", "jpeg", "png",
    "webp", "tif", "tiff", "gif", "bmp",
];

pub const METADATA_KEY: &str = "fold.media.source.v1";
pub const INTERPRETATION_KEY: &str = "fold.media.input.v1";

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SourceProfile {
    Video(VideoInfo),
    Exr(crate::exr::Info),
    Wave(WaveInfo),
    Ppm { dimensions: [u32; 2] },
    Raster(crate::raster::Info),
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

pub fn inspect_linked(
    path: &Path,
    cancel: &Cancel,
) -> Result<LinkedSource, fold_foundation::Failure> {
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
    let mut video_format = None;
    let profile = match extension.as_str() {
        "exr" => {
            let mut info = crate::exr::inspect(&path, cancel)?;
            info.sequence = crate::exr_sequence::discover(&path, &info, cancel)?;
            SourceProfile::Exr(info)
        }
        "wav" | "mp3" | "m4a" | "aac" | "flac" => SourceProfile::Wave(crate::inspect_wave(&path, cancel)?.info),
        "mp4" | "mov" | "mxf" => {
            let (info,format) = match crate::video::verified_info(&hash).zip(crate::video::verified_format(&hash)) {
                Some(value) => value,
                None => crate::video::probe(&path,cancel)?,
            };
            video_format=Some(format);
            SourceProfile::Video(info)
        },
        "jpg" | "jpeg" | "png" | "webp" | "tif" | "tiff" | "gif" | "bmp" => {
            let (_, info) = crate::still::load(&path, &hash, cancel)?;
            SourceProfile::Raster(info.ok_or("image extension does not match its content")?)
        }
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
        _ => return Err("unsupported linked media: expected MP4/MOV/MXF video, WAV/MP3/AAC/M4A/FLAC audio, JPEG/PNG/WebP/TIFF/GIF/BMP, P6 PPM or OpenEXR".into()),
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
    } else if let SourceProfile::Raster(info) = &profile {
        (
            vec![StreamMetadata {
                index: 0,
                kind: "video".into(),
                facts: serde_json::to_value(info).map_err(|e| e.to_string())?,
                bit_depth: Some(info.bit_depth),
                planes: Some(1),
                alpha: Some(
                    if info.premultiplied {
                        "premultiplied"
                    } else if info.alpha {
                        "straight"
                    } else {
                        "opaque"
                    }
                    .into(),
                ),
            }],
            serde_json::Value::Null,
        )
    } else if let SourceProfile::Exr(info) = &profile {
        // The asset profile owns the manifest once; stream facts describe pixels.
        let mut stream_info = info.clone();
        stream_info.sequence = None;
        (
            vec![StreamMetadata {
                index: 0,
                kind: "video".into(),
                facts: serde_json::to_value(stream_info).map_err(|e| e.to_string())?,
                bit_depth: None,
                planes: None,
                alpha: None,
            }],
            serde_json::Value::Null,
        )
    } else {
        let output = crate::scratch::file().map_err(|e| e.to_string())?;
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
                    bit_depth: if facts["codec_type"] == "video" {
                        crate::VideoFormat::parse(facts).ok().map(|f| f.depth as u8)
                    } else {
                        facts["bits_per_raw_sample"]
                            .as_str()
                            .and_then(|v| v.parse().ok())
                            .filter(|v| *v != 0)
                            .or_else(|| {
                                facts["bits_per_sample"]
                                    .as_u64()
                                    .and_then(|v| v.try_into().ok())
                                    .filter(|v| *v != 0)
                            })
                    },
                    planes: if facts["codec_type"] == "video" {
                        crate::VideoFormat::parse(facts)
                            .ok()
                            .map(|f| if f.alpha { 4 } else { 3 })
                    } else {
                        None
                    },
                    alpha: (facts["codec_type"] == "video").then(|| {
                        if facts["pix_fmt"]
                            .as_str()
                            .is_some_and(|p| p.starts_with("yuva"))
                        {
                            "straight"
                        } else {
                            "opaque"
                        }
                        .into()
                    }),
                    facts: facts.clone(),
                })
            })
            .collect::<Result<Vec<_>, fold_foundation::Failure>>()?;
        (streams, raw["format"].clone())
    };
    if fingerprint(&path, cancel)? != hash {
        return Err("source changed during inspection".into());
    }
    cancel.check()?;
    if let (SourceProfile::Video(info), Some(format)) = (&profile, video_format) {
        crate::video::remember_verified(&hash, info, format);
    }
    let timing_policy = match &profile {
        SourceProfile::Video(_) => "verified exact CFR, zero origin",
        SourceProfile::Wave(_) => "exact sample frames, zero origin",
        SourceProfile::Exr(info) if info.sequence.is_some() => {
            "exact numbered EXR frames; rate assigned per use"
        }
        SourceProfile::Ppm { .. } | SourceProfile::Raster(_) | SourceProfile::Exr(_) => {
            "still image"
        }
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
        (match (&self.profile, &other.profile) {
            (SourceProfile::Exr(a), SourceProfile::Exr(b)) => a.compatible_with(b),
            (a, b) => a == b,
        }) && self.timing_policy == other.timing_policy
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
