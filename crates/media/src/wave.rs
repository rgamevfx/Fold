//! Deliberately narrow standalone music/dialogue path: RIFF/WAVE PCM16,
//! mono/stereo, 8–192 kHz, at most ten minutes. Parsing never loads sample data.
use crate::{Cancel, fingerprint};
use fold_foundation::Time;
use serde::{Deserialize, Serialize};
use std::{
    io::{Read, Seek, SeekFrom},
    path::{Path, PathBuf},
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
            return Err(
                "WAVE requires 1–2 PCM16 channels, 8–192 kHz, and up to ten minutes".into(),
            );
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

pub fn inspect_wave(path: &Path, cancel: &Cancel) -> Result<WaveSource, String> {
    cancel.check()?;
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    let mut header = [0; 12];
    file.read_exact(&mut header).map_err(|e| e.to_string())?;
    if &header[..4] != b"RIFF"
        || &header[8..] != b"WAVE"
        || u64::from(u32::from_le_bytes(header[4..8].try_into().unwrap())) + 8 != size
    {
        return Err("unsupported or truncated RIFF/WAVE".into());
    }
    let mut format = None;
    let mut data = None;
    let mut offset = 12u64;
    while offset + 8 <= size {
        cancel.check()?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|e| e.to_string())?;
        let mut chunk = [0; 8];
        file.read_exact(&mut chunk).map_err(|e| e.to_string())?;
        let length = u32::from_le_bytes(chunk[4..].try_into().unwrap());
        let next = offset + 8 + u64::from(length) + u64::from(length % 2);
        if next > size {
            return Err("truncated WAVE chunk".into());
        }
        match &chunk[..4] {
            b"fmt " => {
                if format.is_some() || !(16..=64).contains(&length) {
                    return Err("unsupported WAVE format chunk".into());
                }
                let mut bytes = [0; 16];
                file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
                let word = |i| u16::from_le_bytes([bytes[i], bytes[i + 1]]);
                let rate = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
                let channels = word(2);
                if !(1..=2).contains(&channels)
                    || word(0) != 1
                    || word(14) != 16
                    || word(12) != channels * 2
                {
                    return Err("only PCM16 WAVE is supported".into());
                }
                format = Some((rate, channels));
            }
            b"data" if data.replace(length).is_some() => {
                return Err("multiple WAVE data chunks".into());
            }
            _ => {}
        }
        offset = next;
    }
    let (rate, channels) = format.ok_or("missing WAVE format")?;
    if channels == 0 || channels > 2 {
        return Err("unsupported WAVE channel layout".into());
    }
    let length = data.ok_or("missing WAVE data")?;
    if !length.is_multiple_of(u32::from(channels) * 2) {
        return Err("partial WAVE sample".into());
    }
    let info = WaveInfo {
        rate,
        channels,
        frames: length / (u32::from(channels) * 2),
    };
    info.validate()?;
    Ok(WaveSource {
        path: path.canonicalize().map_err(|e| e.to_string())?,
        fingerprint: fingerprint(path, cancel)?,
        info,
    })
}
