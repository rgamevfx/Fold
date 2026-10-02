use crate::{Cancel, RgbImage, process::Process};
use fold_foundation::{Rounding, Time};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FRAMES: u32 = 18000;

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct VideoInfo {
    pub width: u32,
    pub height: u32,
    pub rate: [u32; 2],
    pub frames: u32,
}
impl VideoInfo {
    /// General authored output constraints, independent of an encoder profile.
    pub fn validate(&self) -> Result<(), String> {
        let pixels = u64::from(self.width) * u64::from(self.height);
        if pixels == 0
            || pixels > crate::MAX_PIXELS as u64
            || self.rate.contains(&0)
            || self.frames == 0
            // Desktop frame entry uses signed 32-bit indices.
            || self.frames > i32::MAX as u32
        {
            return Err("invalid output dimensions, frame rate, or duration".into());
        }
        Ok(())
    }
    /// Restrictions of the currently verified MP4/H.264 adapter, not documents.
    fn validate_media_profile(&self) -> Result<(), String> {
        self.validate()?;
        if !self.width.is_multiple_of(2)
            || !self.height.is_multiple_of(2)
            || self.frames > MAX_FRAMES
        {
            return Err("MP4/H.264 requires even dimensions and at most 18000 frames".into());
        }
        Ok(())
    }
    pub fn time(&self, frame: u32) -> Result<Time, String> {
        let numerator = i64::from(frame)
            .checked_mul(i64::from(self.rate[1]))
            .ok_or("output time exceeds rational range")?;
        Time::new(numerator, self.rate[0]).map_err(|e| e.to_string())
    }
    pub fn frame_at(&self, time: Time) -> Result<u32, String> {
        self.validate()?;
        let frame = time
            .to_ticks(self.rate[0], self.rate[1], Rounding::Floor)
            .map_err(|e| e.to_string())?;
        if frame < 0 || frame >= i64::from(self.frames) {
            return Err("time outside video range".into());
        }
        Ok(frame as u32)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct VideoSource {
    pub path: PathBuf,
    pub fingerprint: String,
    pub info: VideoInfo,
}

pub fn fingerprint(path: &Path, cancel: &Cancel) -> Result<String, String> {
    let mut file = std::fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("media must be a regular file".into());
    }
    let mut hash = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size = 0u64;
    loop {
        cancel.check()?;
        let n = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        size += n as u64;
        if size > MAX_FILE {
            return Err("media exceeds 2 GiB source budget".into());
        }
        hash.update(&buffer[..n]);
    }
    Ok(format!("sha256:{:x}", hash.finalize()))
}

fn base(program: &str) -> Command {
    let mut cmd = Command::new(program);
    cmd.args(["-v", "error"])
        .stdin(Stdio::null())
        .stdout(Stdio::null());
    cmd
}

pub fn inspect(path: &Path, cancel: &Cancel) -> Result<VideoSource, String> {
    let path = path.canonicalize().map_err(|e| e.to_string())?;
    let hash = fingerprint(&path, cancel)?;
    let info = probe(&path, cancel)?;
    if fingerprint(&path, cancel)? != hash {
        return Err("source changed during import".into());
    }
    Ok(VideoSource {
        path,
        fingerprint: hash,
        info,
    })
}

fn probe(path: &Path, cancel: &Cancel) -> Result<VideoInfo, String> {
    let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut cmd = base("ffprobe");
    cmd.args(["-threads", "1", "-select_streams", "v", "-show_streams", "-show_frames", "-show_format", "-show_entries", "stream=codec_name,pix_fmt,width,height,r_frame_rate,time_base,color_space,color_transfer,color_primaries,color_range,sample_aspect_ratio,field_order,chroma_location:stream_side_data=rotation:frame=best_effort_timestamp,pkt_duration:format=format_name", "-of", "json"])
        .arg(path).stdout(Stdio::from(output.reopen().map_err(|e| e.to_string())?));
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
    if value["streams"].as_array().map(Vec::len) != Some(1) {
        return Err("exactly one video stream per source is supported".into());
    }
    let stream = &value["streams"][0];
    for (key, expected) in [
        ("codec_name", "h264"),
        ("pix_fmt", "yuv420p"),
        ("color_space", "bt709"),
        ("color_transfer", "bt709"),
        ("color_primaries", "bt709"),
        ("color_range", "tv"),
        ("sample_aspect_ratio", "1:1"),
        ("field_order", "progressive"),
        ("chroma_location", "left"),
    ] {
        if stream[key].as_str() != Some(expected) {
            return Err(format!(
                "unsupported {key}: expected {expected}, got {}",
                stream[key]
            ));
        }
    }
    if !value["format"]["format_name"]
        .as_str()
        .unwrap_or("")
        .split(',')
        .any(|s| s == "mp4")
    {
        return Err("only MP4/H.264 is supported".into());
    }
    if stream["side_data_list"].as_array().is_some_and(|items| {
        items
            .iter()
            .any(|s| s["rotation"].as_i64().unwrap_or(0) != 0)
    }) {
        return Err("rotated video is not supported".into());
    }
    fn ratio(value: &serde_json::Value) -> Result<[u32; 2], String> {
        let (n, d) = value
            .as_str()
            .and_then(|s| s.split_once('/'))
            .ok_or("missing rational video metadata")?;
        Ok([
            n.parse().map_err(|_| "invalid rate")?,
            d.parse().map_err(|_| "invalid rate")?,
        ])
    }
    let rate = ratio(&stream["r_frame_rate"])?;
    let tb = ratio(&stream["time_base"])?;
    let frames = value["frames"]
        .as_array()
        .ok_or("missing frame timestamps")?;
    let info = VideoInfo {
        width: stream["width"]
            .as_u64()
            .and_then(|n| n.try_into().ok())
            .ok_or("missing width")?,
        height: stream["height"]
            .as_u64()
            .and_then(|n| n.try_into().ok())
            .ok_or("missing height")?,
        rate,
        frames: frames.len().try_into().map_err(|_| "too many frames")?,
    };
    info.validate_media_profile()?;
    // This first profile is exactly CFR from timestamp zero. Reject VFR instead
    // of guessing frame numbers from average frame rate.
    for (i, frame) in frames.iter().enumerate() {
        let pts = frame["best_effort_timestamp"]
            .as_i64()
            .ok_or("missing frame PTS")?;
        let time = Time::new(pts, tb[1])
            .and_then(|t| t.checked_scale(i64::from(tb[0]), 1))
            .map_err(|e| e.to_string())?;
        let duration = frame["pkt_duration"]
            .as_i64()
            .ok_or("missing frame duration")?;
        let duration = Time::new(duration, tb[1])
            .and_then(|t| t.checked_scale(i64::from(tb[0]), 1))
            .map_err(|e| e.to_string())?;
        if time != info.time(i as u32)? || duration != info.time(1)? {
            return Err("only zero-origin constant-frame-rate video is supported".into());
        }
    }
    Ok(info)
}

struct Pinned {
    source: VideoSource,
    file: tempfile::NamedTempFile,
}
/// Two worker-owned verified copies; source bytes cannot change during decoding.
/// Temporary files are removed on eviction/drop, not a persistent frame cache.
#[derive(Default)]
pub struct Decoder {
    pinned: VecDeque<Pinned>,
    // Bounded sequential read-ahead avoids launching a codec for every playback
    // frame. Shared across sources/resolutions: 32 MiB legacy RGB8, 64 MiB when
    // retaining float signal RGB, at most 512 frames. Both paths share one LRU.
    read_ahead: VecDeque<(String, VideoInfo, u32, RgbImage)>,
}
impl Decoder {
    pub(crate) fn pin(&mut self, source: &VideoSource, cancel: &Cancel) -> Result<PathBuf, String> {
        source.info.validate_media_profile()?;
        if let Some(index) = self.pinned.iter().position(|p| {
            p.source.fingerprint == source.fingerprint && p.source.info == source.info
        }) {
            let pinned = self.pinned.remove(index).unwrap();
            let path = pinned.file.path().to_owned();
            self.pinned.push_back(pinned);
            return Ok(path);
        }
        let mut input = std::fs::File::open(&source.path)
            .map_err(|e| format!("missing media {}: {e}", source.path.display()))?;
        if !input.metadata().map_err(|e| e.to_string())?.is_file() {
            return Err("media must be a regular file".into());
        }
        // Evict before allocating the next copy: at most two 2-GiB files per worker.
        if self.pinned.len() == 2 {
            self.pinned.pop_front();
        }
        let mut file = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        let mut hash = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];
        let mut size = 0u64;
        loop {
            cancel.check()?;
            let n = input.read(&mut buffer).map_err(|e| e.to_string())?;
            if n == 0 {
                break;
            }
            size += n as u64;
            if size > MAX_FILE {
                return Err("media exceeds 2 GiB source budget".into());
            }
            hash.update(&buffer[..n]);
            file.write_all(&buffer[..n]).map_err(|e| e.to_string())?;
        }
        if format!("sha256:{:x}", hash.finalize()) != source.fingerprint {
            return Err("source fingerprint changed; reimport required".into());
        }
        // Ensure persisted metadata cannot reinterpret the same fingerprint.
        let actual = probe(file.path(), cancel)?;
        if actual != source.info {
            return Err("source metadata mismatch".into());
        }
        let path = file.path().to_owned();
        self.pinned.push_back(Pinned {
            source: source.clone(),
            file,
        });
        Ok(path)
    }

    pub fn decode(
        &mut self,
        source: &VideoSource,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
    ) -> Result<RgbImage, String> {
        self.decode_mode(source, time, dimensions, cancel, false)
    }

    /// Reconstruct BT.709 YUV as float RGB with its camera transfer intact.
    /// OCIO input assignment, not the decoder, chooses the working conversion.
    pub fn decode_signal(
        &mut self,
        source: &VideoSource,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
    ) -> Result<RgbImage, String> {
        self.decode_mode(source, time, dimensions, cancel, true)
    }

    fn decode_mode(
        &mut self,
        source: &VideoSource,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
        signal: bool,
    ) -> Result<RgbImage, String> {
        let disk_pixel = if signal { 12 } else { 3 };
        let stored_pixel = if signal { 16 } else { 3 };
        let budget = if signal { 64 } else { 32 } * 1024 * 1024;
        let frame = source.info.frame_at(time)?;
        let count = u64::from(dimensions[0]) * u64::from(dimensions[1]);
        if count == 0 || count > crate::MAX_PIXELS as u64 {
            return Err("invalid decode resolution".into());
        }
        cancel.check()?;
        if let Some((_, _, _, image)) =
            self.read_ahead
                .iter()
                .find(|(fingerprint, info, index, image)| {
                    fingerprint == &source.fingerprint
                        && info == &source.info
                        && *index == frame
                        && image.dimensions() == dimensions
                        && image.is_signal() == signal
                })
        {
            return Ok(image.clone());
        }
        let path = self.pin(source, cancel)?;
        let batch = u64::from((source.info.frames - frame).min(32))
            .min((16 * 1024 * 1024 / (count * stored_pixel)).max(1));
        let mut retained: u64 = self
            .read_ahead
            .iter()
            .map(|(_, _, _, image)| image.storage_bytes())
            .sum();
        while retained + count * stored_pixel * batch > budget
            || self.read_ahead.len() + batch as usize > 512
        {
            let (_, _, _, image) = self
                .read_ahead
                .pop_front()
                .ok_or("read-ahead budget exceeded")?;
            retained -= image.storage_bytes();
        }
        let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
        // Seek to a whole second at/before the requested frame. Accurate input
        // seek discards earlier frames; select the remaining exact CFR offset.
        // Whole seconds avoid decimal rounding at fractional-rate boundaries.
        let target = source.info.time(frame)?;
        let second = target.numerator() / i64::from(target.denominator());
        let first = Time::new(second, 1)
            .unwrap()
            .to_ticks(source.info.rate[0], source.info.rate[1], Rounding::Ceil)
            .map_err(|e| e.to_string())?;
        let offset = i64::from(frame) - first;
        let transfer = if signal { "709" } else { "iec61966-2-1" };
        let pixel_format = if signal { "gbrpf32le" } else { "rgb24" };
        let filter = format!(
            "select=gte(n\\,{offset}),scale={}:{}:flags=neighbor,zscale=matrixin=709:transferin=709:primariesin=709:rangein=limited:matrix=gbr:transfer={transfer}:primaries=709:range=full,format=gbrpf32le,format={pixel_format}",
            dimensions[0], dimensions[1]
        );
        let mut cmd = base("ffmpeg");
        cmd.args([
            "-nostdin",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-noautorotate",
            "-ss",
            &second.to_string(),
            "-i",
        ])
        .arg(path)
        .args([
            "-map",
            "0:v:0",
            "-an",
            "-vf",
            &filter,
            "-frames:v",
            &batch.to_string(),
            "-fps_mode",
            "passthrough",
            "-f",
            "rawvideo",
            "-pix_fmt",
            pixel_format,
            "pipe:1",
        ])
        .stdout(Stdio::from(output.reopen().map_err(|e| e.to_string())?));
        Process::spawn(
            cmd,
            cancel.clone(),
            Some((output.path().into(), count * disk_pixel * batch)),
        )?
        .finish()?;
        cancel.check()?;
        let mut bytes = Vec::new();
        output
            .reopen()
            .map_err(|e| e.to_string())?
            .take(count * disk_pixel * batch + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() as u64 != count * disk_pixel * batch {
            return Err("incomplete video read-ahead decode".into());
        }
        for (offset, bytes) in bytes
            .chunks_exact((count * disk_pixel) as usize)
            .enumerate()
        {
            self.read_ahead.push_back((
                source.fingerprint.clone(),
                source.info.clone(),
                frame + offset as u32,
                if signal {
                    RgbImage::from_gbr(dimensions, bytes)?
                } else {
                    RgbImage::from_rgb(dimensions, bytes.to_vec()).map_err(str::to_owned)?
                },
            ));
        }
        Ok(self.read_ahead[self.read_ahead.len() - batch as usize]
            .3
            .clone())
    }
}

/// Ordered RGB8 sRGB frames to video-only limited-range BT.709 H.264/MP4.
/// Writes a temporary destination; no partial final output or overwrites.
pub struct Encoder {
    process: Option<Process>,
    output: Option<tempfile::NamedTempFile>,
    destination: PathBuf,
    pixels: usize,
    expected: u32,
    written: u32,
    cancel: Cancel,
}
impl Encoder {
    pub fn new(
        destination: &Path,
        info: &VideoInfo,
        frames: u32,
        cancel: Cancel,
    ) -> Result<Self, String> {
        Self::new_with_signal(destination, info, frames, cancel, false)
    }
    /// Display-rendered Rec.709 RGB: only RGB-to-YUV reconstruction is applied,
    /// never a second sRGB/scene transfer. Rec.1886 display signal is retained.
    pub fn new_rec709(
        destination: &Path,
        info: &VideoInfo,
        frames: u32,
        cancel: Cancel,
    ) -> Result<Self, String> {
        Self::new_with_signal(destination, info, frames, cancel, true)
    }
    fn new_with_signal(
        destination: &Path,
        info: &VideoInfo,
        frames: u32,
        cancel: Cancel,
        rec709: bool,
    ) -> Result<Self, String> {
        info.validate()?;
        let mut delivery = info.clone();
        delivery.frames = frames;
        delivery.validate_media_profile()?;
        if frames == 0 || frames > info.frames || destination.exists() {
            return Err("invalid export range or destination already exists".into());
        }
        let parent = destination
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or(Path::new("."));
        let output = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
        let transfer = if rec709 { "709" } else { "iec61966-2-1" };
        let filter = format!(
            "format=gbrpf32le,zscale=matrixin=gbr:transferin={transfer}:primariesin=709:rangein=full:matrix=709:transfer=709:primaries=709:range=limited,format=yuv420p,setsar=1"
        );
        let mut cmd = base("ffmpeg");
        cmd.args([
            "-y",
            "-threads",
            "1",
            "-filter_threads",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-video_size",
            &format!("{}x{}", info.width, info.height),
            "-framerate",
            &format!("{}/{}", info.rate[0], info.rate[1]),
            "-i",
            "pipe:0",
            "-an",
            "-vf",
            &filter,
            "-c:v",
            "libx264",
            "-threads",
            "1",
            "-preset",
            "veryfast",
            "-crf",
            "18",
            "-color_range",
            "tv",
            "-colorspace",
            "bt709",
            "-color_trc",
            "bt709",
            "-color_primaries",
            "bt709",
            "-movflags",
            "+faststart",
            "-f",
            "mp4",
        ])
        .arg(output.path())
        .stdin(Stdio::piped());
        let process = Process::spawn(cmd, cancel.clone(), Some((output.path().into(), MAX_FILE)))?;
        Ok(Self {
            process: Some(process),
            output: Some(output),
            destination: destination.into(),
            pixels: info.width as usize * info.height as usize,
            expected: frames,
            written: 0,
            cancel,
        })
    }
    pub fn write_rgb(&mut self, rgb: &[u8]) -> Result<(), String> {
        self.cancel.check()?;
        if rgb.len() != self.pixels * 3 || self.written >= self.expected {
            return Err("export frame length/count mismatch".into());
        }
        if let Err(error) = self
            .process
            .as_mut()
            .ok_or("encoder already stopped")?
            .stdin
            .as_mut()
            .unwrap()
            .write_all(rgb)
        {
            let details = self
                .process
                .take()
                .unwrap()
                .finish()
                .err()
                .unwrap_or_default();
            return Err(format!("encoder input: {error}; {details}"));
        }
        self.written += 1;
        Ok(())
    }
    pub fn finish(mut self) -> Result<(), String> {
        if self.written != self.expected {
            return Err("incomplete export".into());
        }
        self.process
            .take()
            .ok_or("encoder already stopped")?
            .finish()?;
        self.cancel.check()?;
        let output = self.output.take().unwrap();
        output.as_file().sync_all().map_err(|e| e.to_string())?;
        output
            .persist_noclobber(&self.destination)
            .map_err(|e| e.to_string())?;
        #[cfg(unix)]
        {
            let parent = self
                .destination
                .parent()
                .filter(|p| !p.as_os_str().is_empty())
                .unwrap_or(Path::new("."));
            std::fs::File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|e| format!("export finalized, but directory sync failed: {e}"))?;
        }
        Ok(())
    }
}
