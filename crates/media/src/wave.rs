//! Standalone mono/stereo audio. Historical Wave names remain archive-compatible;
//! the verified source may be PCM WAV, MP3, AAC/M4A or FLAC.
use crate::{Cancel, fingerprint, process::Process};
use fold_foundation::Time;
use serde::{Deserialize, Serialize};
use std::{
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
};
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct WaveInfo {
    pub rate: u32,
    pub channels: u16,
    pub frames: u32,
}
impl WaveInfo {
    pub fn validate(&self) -> Result<(), String> {
        if !(8000..=192000).contains(&self.rate)
            || !(1..=2).contains(&self.channels)
            || self.frames == 0
            || u64::from(self.frames) > u64::from(self.rate) * 600
        {
            return Err("Audio requires mono/stereo, 8–192 kHz, and up to ten minutes".into());
        }
        Ok(())
    }
    pub fn duration(&self) -> Result<Time, String> {
        self.validate()?;
        Time::new(i64::from(self.frames), self.rate).map_err(|e| e.to_string())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WaveSource {
    pub path: PathBuf,
    pub fingerprint: String,
    pub info: WaveInfo,
}
static VERIFIED: std::sync::Mutex<std::collections::VecDeque<(String, WaveInfo)>> =
    std::sync::Mutex::new(std::collections::VecDeque::new());
pub fn inspect_wave(path: &Path, cancel: &Cancel) -> Result<WaveSource, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let hash = fingerprint(&path, cancel)?;
    if let Some(info) = VERIFIED
        .lock()
        .unwrap()
        .iter()
        .find(|(key, _)| key == &hash)
        .map(|(_, info)| info.clone())
    {
        return Ok(WaveSource {
            path,
            fingerprint: hash,
            info,
        });
    }
    let output = crate::scratch::file().map_err(|e| e.to_string())?;
    let mut cmd = Command::new("ffprobe");
    cmd.args([
        "-v",
        "error",
        "-threads",
        "1",
        "-select_streams",
        "a",
        "-show_streams",
        "-show_frames",
        "-show_format",
        "-show_entries",
        "stream=codec_name,sample_rate,channels,start_pts,duration_ts,time_base:frame=nb_samples:format=format_name",
        "-of",
        "json",
    ])
    .arg(&path)
    .stdin(Stdio::null())
    .stdout(Stdio::from(output.reopen().map_err(|e| e.to_string())?));
    Process::spawn(
        cmd,
        cancel.clone(),
        Some((output.path().into(), 8 * 1024 * 1024)),
    )?
    .finish()?;
    let value: serde_json::Value = serde_json::from_reader(
        output
            .reopen()
            .map_err(|e| e.to_string())?
            .take(8 * 1024 * 1024),
    )
    .map_err(|e| e.to_string())?;
    let streams = value["streams"].as_array().ok_or("missing audio streams")?;
    if streams.len() != 1 {
        return Err("exactly one standalone audio stream is supported".into());
    }
    let s = &streams[0];
    let codec = s["codec_name"].as_str().unwrap_or("");
    if ![
        "pcm_s16le",
        "pcm_s24le",
        "pcm_s32le",
        "pcm_f32le",
        "mp3",
        "aac",
        "flac",
    ]
    .contains(&codec)
    {
        return Err(format!("unsupported audio codec: {codec}"));
    }
    let mut frames = value["frames"]
        .as_array()
        .ok_or("missing decoded audio sample counts")?
        .iter()
        .try_fold(0u64, |n, f| {
            n.checked_add(f["nb_samples"].as_u64().ok_or("missing audio samples")?)
                .ok_or("audio length overflow")
        })?;
    let rate: u32 = s["sample_rate"]
        .as_str()
        .ok_or("missing audio rate")?
        .parse()
        .map_err(|_| "invalid audio rate")?;
    // AAC access units may extend past the MOV edit-list end. Respect an exact
    // container end without retaining encoder padding as authored duration.
    if value["format"]["format_name"]
        .as_str()
        .is_some_and(|v| v.split(',').any(|v| v == "mov"))
        && s["start_pts"].as_i64() == Some(0)
    {
        if let Some((n, d)) = s["time_base"].as_str().and_then(|v| v.split_once('/')) {
            if let (Ok(n), Ok(d), Some(duration)) = (
                n.parse::<u64>(),
                d.parse::<u64>(),
                s["duration_ts"].as_u64(),
            ) {
                let samples = u128::from(duration) * u128::from(n) * u128::from(rate);
                if d > 0 && samples % u128::from(d) == 0 {
                    frames = frames.min(
                        (samples / u128::from(d))
                            .try_into()
                            .map_err(|_| "audio duration overflow")?,
                    );
                }
            }
        }
    }
    let info = WaveInfo {
        rate,
        channels: s["channels"]
            .as_u64()
            .and_then(|v| v.try_into().ok())
            .ok_or("invalid channel count")?,
        frames: frames.try_into().map_err(|_| "audio length overflow")?,
    };
    info.validate()?;
    if fingerprint(&path, cancel)? != hash {
        return Err("audio changed during import".into());
    }
    let mut verified = VERIFIED.lock().unwrap();
    verified.retain(|(key, _)| key != &hash);
    if verified.len() >= 128 {
        verified.pop_front();
    }
    verified.push_back((hash.clone(), info.clone()));
    Ok(WaveSource {
        path,
        fingerprint: hash,
        info,
    })
}
