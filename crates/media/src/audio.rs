//! Worker-owned, disk-backed stereo f32 PCM at 48 kHz. Audio is resampled once
//! in retained ranges, independently of video cadence. Missing audio is silence.
use crate::{Cancel, VideoSource, WaveSource, process::Process};
use std::{
    collections::VecDeque,
    io::{Read, Seek, SeekFrom, Write},
    path::Path,
    process::{Command, Stdio},
};
pub const AUDIO_RATE: u32 = 48_000;
pub const MAX_AUDIO_SAMPLES: u64 = 48_000 * 600;

#[derive(Clone, Debug)]
pub enum AudioSource {
    Video(VideoSource),
    Wave(WaveSource),
}
impl From<VideoSource> for AudioSource {
    fn from(source: VideoSource) -> Self {
        Self::Video(source)
    }
}
impl AudioSource {
    fn identity(&self) -> String {
        match self {
            Self::Video(s) => format!("{}:{:?}", s.fingerprint, s.info),
            Self::Wave(s) => format!("{}:{:?}", s.fingerprint, s.info),
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Video(s) => s.info.validate(),
            Self::Wave(s) => s.info.validate(),
        }
    }
}
struct Pcm {
    identity: String,
    file: tempfile::NamedTempFile,
    frames: u64,
    prepared: u64,
    stream: Option<crate::video_stream::Stream>,
    has_audio: bool,
    path: std::path::PathBuf,
    // Immutable standalone source must outlive the retained process.
    _source: Option<tempfile::NamedTempFile>,
    _video: Option<std::sync::Arc<crate::source_pin::Pinned>>,
    _source_storage: Option<std::sync::Arc<crate::budget::Lease>>,
    _storage: std::sync::Arc<crate::budget::Lease>,
}
#[derive(Default)]
pub struct AudioDecoder {
    pcm: VecDeque<Pcm>,
    interactive: bool,
}
fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(["-v", "error"])
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    cmd
}
pub fn has_audio(path: &Path, cancel: &Cancel) -> Result<bool, String> {
    has_audio_mode(path, cancel, false)
}
fn has_audio_mode(path: &Path, cancel: &Cancel, interactive: bool) -> Result<bool, String> {
    let probe = crate::scratch::file().map_err(|e| e.to_string())?;
    let mut cmd = command("ffprobe");
    cmd.args([
        "-select_streams",
        "a:0",
        "-show_entries",
        "stream=index",
        "-of",
        "csv=p=0",
    ])
    .arg(path)
    .stdout(Stdio::from(probe.reopen().map_err(|e| e.to_string())?));
    let spawn = if interactive {
        Process::audio
    } else {
        Process::spawn
    };
    spawn(cmd, cancel.clone(), Some((probe.path().into(), 4096)))?.finish()?;
    Ok(probe.as_file().metadata().map_err(|e| e.to_string())?.len() != 0)
}
impl AudioDecoder {
    /// Monitored playback alone may consume the reserved codec-process slots.
    pub fn for_playback() -> Self {
        Self {
            interactive: true,
            ..Self::default()
        }
    }
    /// Retained PCM source count and on-disk bytes (not resident memory).
    pub fn prepared_usage(&self) -> (usize, u64) {
        (
            self.pcm.len(),
            self.pcm.iter().map(|pcm| pcm.prepared * 8).sum(),
        )
    }
    /// Keep preparation for the active plan across seeks, but release sources
    /// removed by edits/project switches before enforcing the PCM budget.
    pub fn retain_sources<'a>(&mut self, sources: impl IntoIterator<Item = &'a AudioSource>) {
        let identities: std::collections::BTreeSet<_> =
            sources.into_iter().map(AudioSource::identity).collect();
        self.pcm.retain(|pcm| identities.contains(&pcm.identity));
    }

    fn prepare(&mut self, source: &AudioSource, cancel: &Cancel) -> Result<&mut Pcm, String> {
        cancel.check()?;
        source.validate()?;
        let identity = source.identity();
        if let Some(index) = self.pcm.iter().position(|p| p.identity == identity) {
            let pcm = self.pcm.remove(index).unwrap();
            self.pcm.push_back(pcm);
            return Ok(self.pcm.back_mut().unwrap());
        }
        let mut video_pin = None;
        let mut wave_copy = None;
        let mut wave_reservation = None;
        let path = match source {
            AudioSource::Video(source) => {
                let pin = crate::source_pin::acquire(source, cancel)?;
                let path = pin.file.path().to_owned();
                video_pin = Some(pin);
                path
            }
            AudioSource::Wave(source) => {
                let source_bytes = std::fs::metadata(&source.path)
                    .map_err(|e| e.to_string())?
                    .len();
                wave_reservation = Some(crate::budget::WAVE_COPY.reserve(source_bytes)?);
                let mut copy = crate::scratch::file().map_err(|e| e.to_string())?;
                let mut input = std::fs::File::open(&source.path).map_err(|e| e.to_string())?;
                let mut buffer = [0; 65536];
                let mut size = 0u64;
                loop {
                    cancel.check()?;
                    let count = input.read(&mut buffer).map_err(|e| e.to_string())?;
                    if count == 0 {
                        break;
                    }
                    size += count as u64;
                    if size > source_bytes {
                        return Err("audio exceeds source budget".into());
                    }
                    copy.write_all(&buffer[..count])
                        .map_err(|e| e.to_string())?;
                }
                let actual = crate::inspect_wave(copy.path(), cancel)?;
                if actual.fingerprint != source.fingerprint || actual.info != source.info {
                    return Err("audio fingerprint/metadata changed; reimport required".into());
                }
                let path = copy.path().to_owned();
                wave_copy = Some(copy);
                path
            }
        };
        let duration = match source {
            AudioSource::Wave(s) => s.info.duration()?,
            AudioSource::Video(s) => s.info.time(s.info.frames)?,
        };
        let frames = duration
            .to_ticks(AUDIO_RATE, 1, fold_foundation::Rounding::Ceil)
            .map_err(|e| e.to_string())? as u64;
        if frames > MAX_AUDIO_SAMPLES {
            return Err("audio exceeds ten-minute PCM budget".into());
        }
        let padded = frames.div_ceil(4096) * 4096;
        let bytes = padded * 8;
        if self.pcm.len() >= 128
            || self
                .pcm
                .iter()
                .map(|p| p.frames.div_ceil(4096) * 4096 * 8)
                .sum::<u64>()
                + bytes
                > 512 * 1024 * 1024
        {
            return Err("audio preparation exceeds 512 MiB / 128-source PCM budget".into());
        }
        let storage = crate::budget::PCM.reserve(bytes)?;
        let file = crate::scratch::file().map_err(|e| e.to_string())?;
        let has_audio = has_audio_mode(&path, cancel, self.interactive)?;
        self.pcm.push_back(Pcm {
            _storage: storage,
            identity,
            file,
            frames,
            prepared: 0,
            stream: None,
            has_audio,
            path,
            _source: wave_copy,
            _video: video_pin,
            _source_storage: wave_reservation,
        });
        Ok(self.pcm.back_mut().unwrap())
    }
    pub fn preflight(&mut self, source: &AudioSource, cancel: &Cancel) -> Result<(), String> {
        self.read(source, 0, 1, cancel).map(|_| ())
    }
    /// At most one second per request; negative and post-EOF samples are silence.
    pub fn read(
        &mut self,
        source: &AudioSource,
        start: i64,
        count: usize,
        cancel: &Cancel,
    ) -> Result<Vec<[f32; 2]>, String> {
        cancel.check()?;
        if count > AUDIO_RATE as usize {
            return Err("audio block exceeds one second".into());
        }
        let result = self.read_prepared(source, start, count, cancel);
        if result.is_err() {
            self.pcm.retain(|p| p.identity != source.identity());
        }
        result
    }
    fn read_prepared(
        &mut self,
        source: &AudioSource,
        start: i64,
        count: usize,
        cancel: &Cancel,
    ) -> Result<Vec<[f32; 2]>, String> {
        // Keep at most two backpressured PCM processes per worker. Disk blocks
        // remain reusable; a displaced cursor restarts at an exact sample trim.
        if !self
            .pcm
            .iter()
            .any(|p| p.identity == source.identity() && p.stream.is_some())
        {
            while self.pcm.iter().filter(|p| p.stream.is_some()).count() >= 2 {
                self.pcm
                    .iter_mut()
                    .find(|p| p.stream.is_some())
                    .unwrap()
                    .stream
                    .take();
            }
        }
        self.prepare(source, cancel)?;
        let pcm = self.pcm.back_mut().unwrap();
        let mut output = vec![[0.0; 2]; count];
        let skip = start.saturating_neg().max(0).min(count as i64) as usize;
        let first = start.max(0) as u64;
        let length = (count - skip).min(pcm.frames.saturating_sub(first) as usize);
        if length != 0 {
            while pcm.prepared < first + length as u64 {
                cancel.check()?;
                if pcm.stream.is_none() && pcm.has_audio {
                    let padded = pcm.frames.div_ceil(4096) * 4096;
                    let mut cmd = command("ffmpeg");
                    cmd.args(["-nostdin","-threads","1","-i"]).arg(&pcm.path)
                        .args(["-map","0:a:0","-vn","-af",
                            &format!("aresample=48000:async=1:first_pts=0,apad=whole_len={padded},atrim=start_sample={}:end_sample={padded}",pcm.prepared),
                            "-ac","2","-ar","48000","-f","f32le","pipe:1"])
                        .stdout(Stdio::piped());
                    pcm.stream = Some(crate::video_stream::Stream::with_audio(
                        cmd,
                        4096 * 8,
                        self.interactive,
                    )?);
                }
                let bytes = if let Some(stream) = pcm.stream.as_mut() {
                    stream.read(cancel)?.0
                } else {
                    vec![0; 4096 * 8]
                };
                pcm.file
                    .as_file_mut()
                    .seek(SeekFrom::Start(pcm.prepared * 8))
                    .map_err(|e| e.to_string())?;
                pcm.file
                    .as_file_mut()
                    .write_all(&bytes)
                    .map_err(|e| e.to_string())?;
                pcm.prepared += 4096;
                if pcm.prepared >= pcm.frames {
                    pcm.stream.take();
                }
            }
            let file = pcm.file.as_file_mut();
            file.seek(SeekFrom::Start(first * 8))
                .map_err(|e| e.to_string())?;
            let mut bytes = vec![0; length * 8];
            file.read_exact(&mut bytes).map_err(|e| e.to_string())?;
            for (frame, bytes) in output[skip..skip + length]
                .iter_mut()
                .zip(bytes.chunks_exact(8))
            {
                for ch in 0..2 {
                    let value = f32::from_le_bytes(bytes[ch * 4..ch * 4 + 4].try_into().unwrap());
                    if !value.is_finite() {
                        return Err("non-finite decoded audio".into());
                    }
                    frame[ch] = value;
                }
            }
        }
        Ok(output)
    }
}

/// Mux a completed temporary video and exact stereo PCM range. Never overwrite
/// a destination or publish a partial output. AAC encoder priming is container-tagged.
pub fn mux_audio(
    video: &Path,
    pcm: &Path,
    destination: &Path,
    cancel: &Cancel,
) -> Result<(), String> {
    if destination.exists() {
        return Err("export destination already exists".into());
    }
    let parent = destination
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let _disk = crate::budget::reserve_delivery(2 * 1024 * 1024 * 1024)?;
    let output = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    let mut cmd = command("ffmpeg");
    cmd.args(["-nostdin", "-y", "-i"])
        .arg(video)
        .args(["-f", "f32le", "-ar", "48000", "-ac", "2", "-i"])
        .arg(pcm)
        .args([
            "-map",
            "0:v:0",
            "-map",
            "1:a:0",
            "-c:v",
            "copy",
            "-c:a",
            "aac",
            "-b:a",
            "192k",
            "-movflags",
            "+faststart",
            "-f",
            if destination
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mov"))
            {
                "mov"
            } else {
                "mp4"
            },
        ])
        .arg(output.path());
    Process::spawn(
        cmd,
        cancel.clone(),
        Some((output.path().into(), 2 * 1024 * 1024 * 1024)),
    )?
    .finish()?;
    cancel.check()?;
    output.as_file().sync_all().map_err(|e| e.to_string())?;
    output
        .persist_noclobber(destination)
        .map_err(|e| e.to_string())?;
    #[cfg(unix)]
    std::fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| format!("export finalized but directory sync failed: {e}"))?;
    Ok(())
}
