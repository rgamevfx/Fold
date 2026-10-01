//! Worker-owned, disk-backed stereo f32 PCM at 48 kHz. Audio is resampled once
//! per pinned source, independently of video cadence. Missing audio is silence.
use crate::{Cancel, Decoder, VideoSource, WaveSource, process::Process};
use std::{
    collections::VecDeque,
    io::{Read, Seek, SeekFrom},
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
}
#[derive(Default)]
pub struct AudioDecoder {
    decoder: Decoder,
    pcm: VecDeque<Pcm>,
}
fn command(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(["-v", "error"])
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    cmd
}
pub fn has_audio(path: &Path, cancel: &Cancel) -> Result<bool, String> {
    let probe = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
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
    Process::spawn(cmd, cancel.clone(), Some((probe.path().into(), 4096)))?.finish()?;
    Ok(probe.as_file().metadata().map_err(|e| e.to_string())?.len() != 0)
}
impl AudioDecoder {
    fn prepare(&mut self, source: &AudioSource, cancel: &Cancel) -> Result<&mut Pcm, String> {
        cancel.check()?;
        let identity = source.identity();
        if let Some(index) = self.pcm.iter().position(|p| p.identity == identity) {
            let pcm = self.pcm.remove(index).unwrap();
            self.pcm.push_back(pcm);
            return Ok(self.pcm.back_mut().unwrap());
        }
        let mut wave_copy = None;
        let path = match source {
            AudioSource::Video(source) => self.decoder.pin(source, cancel)?,
            AudioSource::Wave(source) => {
                use std::io::Write;
                let mut copy = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
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
                    if size > 512 * 1024 * 1024 {
                        return Err("WAVE exceeds source budget".into());
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
        let file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        // Absent audio is silence; corrupt media is still an error.
        if has_audio(&path, cancel)? {
            let mut cmd = command("ffmpeg");
            cmd.args(["-nostdin", "-y", "-threads", "1", "-i"])
                .arg(path)
                .args([
                    "-map",
                    "0:a:0",
                    "-vn",
                    "-af",
                    "aresample=48000:async=1:first_pts=0",
                    "-ac",
                    "2",
                    "-ar",
                    "48000",
                    "-f",
                    "f32le",
                ])
                .arg(file.path());
            Process::spawn(
                cmd,
                cancel.clone(),
                Some((
                    file.path().into(),
                    (MAX_AUDIO_SAMPLES + u64::from(AUDIO_RATE)) * 8,
                )),
            )?
            .finish()?;
        }
        let bytes = file.as_file().metadata().map_err(|e| e.to_string())?.len();
        if bytes % 8 != 0 || bytes > (MAX_AUDIO_SAMPLES + u64::from(AUDIO_RATE)) * 8 {
            return Err("audio exceeds ten-minute PCM budget".into());
        }
        drop(wave_copy);
        if self.pcm.len() >= 128
            || self.pcm.iter().map(|p| p.frames * 8).sum::<u64>() + bytes > 512 * 1024 * 1024
        {
            return Err("audio preparation exceeds 512 MiB / 128-source PCM budget".into());
        }
        self.pcm.push_back(Pcm {
            identity,
            file,
            frames: bytes / 8,
        });
        Ok(self.pcm.back_mut().unwrap())
    }
    pub fn preflight(&mut self, source: &AudioSource, cancel: &Cancel) -> Result<(), String> {
        self.prepare(source, cancel).map(|_| ())
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
        let pcm = self.prepare(source, cancel)?;
        let mut output = vec![[0.0; 2]; count];
        let skip = start.saturating_neg().max(0).min(count as i64) as usize;
        let first = start.max(0) as u64;
        let length = (count - skip).min(pcm.frames.saturating_sub(first) as usize);
        if length != 0 {
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
                    frame[ch] = value.clamp(-1.0, 1.0);
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
            "mp4",
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
