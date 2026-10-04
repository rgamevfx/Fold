use crate::{Cancel, RgbImage, process::Process};
use fold_foundation::{Rounding, Time};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
};

#[cfg(test)]
#[path = "video_range_tests.rs"]
mod tests;

const MAX_FILE: u64 = 2 * 1024 * 1024 * 1024;
const MAX_FRAMES: u32 = 18000;
static DECODED_BYTES: AtomicU64 = AtomicU64::new(0);
/// Process-wide cache retention, excluding caller-held images and codec buffers.
pub fn decoded_cache_bytes() -> u64 {
    DECODED_BYTES.load(Ordering::Acquire)
}
// Proofs refer to verified complete file bytes, never just a path or mtime.
static VERIFIED: Mutex<VecDeque<(String, VideoInfo)>> = Mutex::new(VecDeque::new());
pub(crate) fn verified_info(fingerprint: &str) -> Option<VideoInfo> {
    VERIFIED
        .lock()
        .unwrap()
        .iter()
        .find(|(hash, _)| hash == fingerprint)
        .map(|(_, info)| info.clone())
}
pub(crate) fn remember_verified(fingerprint: &str, info: &VideoInfo) {
    let mut entries = VERIFIED.lock().unwrap();
    entries.retain(|(hash, _)| hash != fingerprint);
    if entries.len() == 128 {
        entries.pop_front();
    }
    entries.push_back((fingerprint.into(), info.clone()));
}

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
    let info = match verified_info(&hash) {
        Some(info) => info,
        None => probe(&path, cancel)?,
    };
    if fingerprint(&path, cancel)? != hash {
        return Err("source changed during import".into());
    }
    remember_verified(&hash, &info);
    Ok(VideoSource {
        path,
        fingerprint: hash,
        info,
    })
}

pub(crate) fn probe(path: &Path, cancel: &Cancel) -> Result<VideoInfo, String> {
    let output = tempfile::NamedTempFile::new().map_err(|e| e.to_string())?;
    let mut cmd = base("ffprobe");
    cmd.args(["-threads", "1", "-select_streams", "v", "-show_streams", "-show_frames", "-show_format", "-show_entries", "stream=codec_name,pix_fmt,width,height,r_frame_rate,time_base,start_pts,duration_ts,color_space,color_transfer,color_primaries,color_range,sample_aspect_ratio,field_order,chroma_location:stream_side_data=rotation:frame=best_effort_timestamp,pkt_duration,duration:format=format_name", "-of", "json"])
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
    validate_timing(&info, tb, frames, stream)?;
    Ok(info)
}

fn validate_timing(
    info: &VideoInfo,
    tb: [u32; 2],
    frames: &[serde_json::Value],
    stream: &serde_json::Value,
) -> Result<(), String> {
    // This first profile is exactly CFR from timestamp zero. Reject VFR instead
    // of guessing frame numbers from average frame rate.
    for (i, frame) in frames.iter().enumerate() {
        let pts = frame["best_effort_timestamp"]
            .as_i64()
            .ok_or("missing frame PTS")?;
        let time = Time::new(pts, tb[1])
            .and_then(|t| t.checked_scale(i64::from(tb[0]), 1))
            .map_err(|e| e.to_string())?;
        let duration =
            if let Some(value) = frame.get("duration").or_else(|| frame.get("pkt_duration")) {
                value.as_i64().ok_or("invalid frame duration")?
            } else {
                // Some MP4s expose PTS and a stream end but omit packet/frame
                // durations. Prove each interval from those exact integer times;
                // never substitute average rate or guess the final frame length.
                let end = frames
                    .get(i + 1)
                    .and_then(|next| next["best_effort_timestamp"].as_i64())
                    .or_else(|| {
                        (i + 1 == frames.len() && stream["start_pts"].as_i64() == Some(0))
                            .then(|| stream["duration_ts"].as_i64())
                            .flatten()
                    })
                    .ok_or("missing frame duration and exact end timestamp")?;
                end.checked_sub(pts).ok_or("frame duration overflow")?
            };
        let duration = Time::new(duration, tb[1])
            .and_then(|t| t.checked_scale(i64::from(tb[0]), 1))
            .map_err(|e| e.to_string())?;
        if time != info.time(i as u32)? || duration != info.time(1)? {
            return Err("only zero-origin constant-frame-rate video is supported".into());
        }
    }
    Ok(())
}

/// Explicit source-decoder policy. `Cuda` is the compatibility host-download
/// route; `CudaNative` requires the supervised GPU-resident surface service.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum DecodeBackend {
    #[default]
    Software,
    Cuda,
    CudaNative,
}
#[derive(Clone, Copy, Debug, Default)]
pub struct DecodeStatistics {
    pub launches: u64,
    /// Frames received from the range pipe, excluding codec-internal preroll.
    pub decoded_frames: u64,
    pub cache_hits: u64,
    pub pipe_bytes: u64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum DecodeFormat {
    Rgb8,
    Signal,
    Yuv420,
}
impl DecodeFormat {
    fn bytes(self, [w, h]: [u32; 2]) -> u64 {
        let pixels = u64::from(w) * u64::from(h);
        match self {
            Self::Rgb8 => pixels * 3,
            Self::Signal => pixels * 12,
            Self::Yuv420 => pixels + 2 * u64::from(w.div_ceil(2)) * u64::from(h.div_ceil(2)),
        }
    }
}
#[derive(Clone)]
enum Decoded {
    Rgb(RgbImage),
    Yuv(crate::YuvFrame),
}
impl Decoded {
    fn dimensions(&self) -> [u32; 2] {
        match self {
            Self::Rgb(f) => f.dimensions(),
            Self::Yuv(f) => f.dimensions(),
        }
    }
    fn storage_bytes(&self) -> u64 {
        match self {
            Self::Rgb(f) => f.storage_bytes(),
            Self::Yuv(f) => f.storage_bytes(),
        }
    }
    fn format(&self) -> DecodeFormat {
        match self {
            Self::Rgb(f) if f.is_signal() => DecodeFormat::Signal,
            Self::Rgb(_) => DecodeFormat::Rgb8,
            Self::Yuv(_) => DecodeFormat::Yuv420,
        }
    }
    fn rgb(self) -> Result<RgbImage, String> {
        match self {
            Self::Rgb(f) => Ok(f),
            _ => Err("expected RGB decoder result".into()),
        }
    }
}
struct ActiveStream {
    fingerprint: String,
    info: VideoInfo,
    dimensions: [u32; 2],
    format: DecodeFormat,
    next: u32,
    stream: crate::video_stream::Stream,
    pin: Arc<crate::source_pin::Pinned>,
}
/// Two retained range decoders and two leases on process-wide verified sources.
/// Source bytes cannot change during decoding; pins are not a frame cache.
pub struct Decoder {
    pinned: VecDeque<Arc<crate::source_pin::Pinned>>,
    streams: VecDeque<ActiveStream>,
    backend: DecodeBackend,
    statistics: DecodeStatistics,
    // Shared across sources/resolutions, bounded to 64 MiB and 512 frames.
    // Only demanded frames are retained; FFmpeg is backpressured by its pipe.
    read_ahead: VecDeque<(String, VideoInfo, u32, Decoded)>,
}
impl Default for Decoder {
    fn default() -> Self {
        Self::with_backend(DecodeBackend::Software)
    }
}
impl Decoder {
    /// Explicit process policy, independent of project content and GPU rendering.
    /// CUDA failure is reported, never silently relabelled as hardware success.
    pub fn from_environment() -> Result<Self, String> {
        let backend = match std::env::var("FOLD_VIDEO_DECODER").as_deref() {
            Err(std::env::VarError::NotPresent) | Ok("software") => DecodeBackend::Software,
            Ok("cuda") => DecodeBackend::Cuda,
            Ok("cuda-native") => DecodeBackend::CudaNative,
            _ => return Err("FOLD_VIDEO_DECODER must be software, cuda, or cuda-native".into()),
        };
        Ok(Self::with_backend(backend))
    }
    pub fn with_backend(backend: DecodeBackend) -> Self {
        Self {
            pinned: VecDeque::new(),
            streams: VecDeque::new(),
            backend,
            statistics: DecodeStatistics::default(),
            read_ahead: VecDeque::new(),
        }
    }
    pub fn backend(&self) -> DecodeBackend {
        self.backend
    }
    pub fn statistics(&self) -> DecodeStatistics {
        self.statistics
    }
    pub fn retained_bytes(&self) -> u64 {
        self.read_ahead
            .iter()
            .map(|(_, _, _, image)| image.storage_bytes())
            .sum()
    }
    pub(crate) fn pin(&mut self, source: &VideoSource, cancel: &Cancel) -> Result<PathBuf, String> {
        cancel.check()?;
        source.info.validate_media_profile()?;
        if let Some(index) = self.pinned.iter().position(|p| {
            p.source.fingerprint == source.fingerprint && p.source.info == source.info
        }) {
            let pinned = self.pinned.remove(index).unwrap();
            let path = pinned.file.path().to_owned();
            self.pinned.push_back(pinned);
            return Ok(path);
        }
        if self.pinned.len() == 2 {
            // Keep active range leases, not an idle copy whose range already
            // ended. Otherwise two cursor slots can accidentally retain three
            // whole sources and reject a new request under the scratch budget.
            let unused = self
                .pinned
                .iter()
                .position(|pin| {
                    !self
                        .streams
                        .iter()
                        .any(|stream| Arc::ptr_eq(&stream.pin, pin))
                })
                .unwrap_or(0);
            self.pinned.remove(unused);
        }
        let pinned = crate::source_pin::acquire(source, cancel)?;
        let path = pinned.file.path().to_owned();
        self.pinned.push_back(pinned);
        Ok(path)
    }

    pub fn decode(
        &mut self,
        source: &VideoSource,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
    ) -> Result<RgbImage, String> {
        self.decode_mode(source, time, dimensions, cancel, DecodeFormat::Rgb8)?
            .rgb()
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
        cancel.check()?;
        let pixels = u64::from(dimensions[0]) * u64::from(dimensions[1]);
        if pixels == 0 || pixels > super::MAX_PIXELS as u64 {
            return Err("invalid reconstruction dimensions".into());
        }
        self.decode_native(source, time, cancel)?
            .reconstruct_signal(dimensions, cancel)
    }

    /// Independent FFmpeg reconstruction oracle for native-pipeline qualification.
    /// Not the normal GPU or CPU rendering path.
    pub fn decode_signal_reference(
        &mut self,
        source: &VideoSource,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
    ) -> Result<RgbImage, String> {
        self.decode_mode(source, time, dimensions, cancel, DecodeFormat::Signal)?
            .rgb()
    }

    /// Preserve native source planes, independent of viewer resolution. GPU
    /// consumers reconstruct and resize; RGB methods remain compatibility/reference adapters.
    pub fn decode_native(
        &mut self,
        source: &VideoSource,
        time: Time,
        cancel: &Cancel,
    ) -> Result<crate::YuvFrame, String> {
        match self.decode_mode(
            source,
            time,
            [source.info.width, source.info.height],
            cancel,
            DecodeFormat::Yuv420,
        )? {
            Decoded::Yuv(frame) => Ok(frame),
            _ => Err("expected native decoder result".into()),
        }
    }

    fn decode_mode(
        &mut self,
        source: &VideoSource,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
        format: DecodeFormat,
    ) -> Result<Decoded, String> {
        if self.backend == DecodeBackend::CudaNative {
            return Err("cuda-native requires the native GPU surface service; CPU/download fallback is forbidden".into());
        }
        let frame_bytes = format.bytes(dimensions);
        let budget = 64 * 1024 * 1024;
        let frame = source.info.frame_at(time)?;
        if self.backend == DecodeBackend::Cuda
            && (!(48..=4096).contains(&source.info.width)
                || !(16..=4096).contains(&source.info.height))
        {
            return Err("GTX 1070 CUDA/NVDEC H.264 requires source dimensions 48..4096 by 16..4096; select the software decoder".into());
        }
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
                        && image.format() == format
                })
        {
            self.statistics.cache_hits += 1;
            return Ok(image.clone());
        }
        let mut retained = self.retained_bytes();
        while retained + frame_bytes > budget || self.read_ahead.len() >= 512 {
            let (_, _, _, image) = self
                .read_ahead
                .pop_front()
                .ok_or("decoded-frame budget exceeded")?;
            retained -= image.storage_bytes();
            DECODED_BYTES.fetch_sub(image.storage_bytes(), Ordering::AcqRel);
        }
        // A nearby forward request can consume/discard a short gap without a
        // new codec launch. Reverse/far seeks create an exact new range.
        let existing = self.streams.iter().position(|stream| {
            stream.fingerprint == source.fingerprint
                && stream.info == source.info
                && stream.dimensions == dimensions
                && stream.format == format
                && stream.next <= frame
                && frame - stream.next <= 32
        });
        let mut active = if let Some(index) = existing {
            self.streams.remove(index).unwrap()
        } else {
            if self.streams.len() == 2 {
                self.streams.pop_front();
            }
            self.open_stream(source, frame, dimensions, format, cancel)?
        };
        let (bytes, _pipe_storage) = loop {
            // On cancellation/error `active` drops and kills the blocked child.
            let bytes = active.stream.read(cancel)?;
            self.statistics.decoded_frames += 1;
            self.statistics.pipe_bytes += bytes.0.len() as u64;
            let index = active.next;
            active.next += 1;
            if index == frame {
                break bytes;
            }
        };
        if active.next < source.info.frames {
            self.streams.push_back(active);
        }
        let image = match format {
            DecodeFormat::Signal => Decoded::Rgb(RgbImage::from_gbr(dimensions, &bytes)?),
            DecodeFormat::Rgb8 => {
                Decoded::Rgb(RgbImage::from_rgb(dimensions, bytes).map_err(str::to_owned)?)
            }
            DecodeFormat::Yuv420 => Decoded::Yuv(crate::YuvFrame::from_420(
                dimensions,
                bytes,
                source.info.time(frame)?,
                Time::new(i64::from(source.info.rate[1]), source.info.rate[0])
                    .map_err(|e| e.to_string())?,
            )?),
        };
        cancel.check()?;
        // Other workers may already occupy the shared retention allowance. A
        // current frame may still be returned, but do not grow another cache.
        if DECODED_BYTES
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |used| {
                used.checked_add(image.storage_bytes())
                    .filter(|bytes| *bytes <= 128 * 1024 * 1024)
            })
            .is_ok()
        {
            self.read_ahead.push_back((
                source.fingerprint.clone(),
                source.info.clone(),
                frame,
                image.clone(),
            ));
        }
        Ok(image)
    }

    fn open_stream(
        &mut self,
        source: &VideoSource,
        frame: u32,
        dimensions: [u32; 2],
        format: DecodeFormat,
        cancel: &Cancel,
    ) -> Result<ActiveStream, String> {
        let path = self.pin(source, cancel)?;
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
        let transfer = if format == DecodeFormat::Signal {
            "709"
        } else {
            "iec61966-2-1"
        };
        let pixel_format = match format {
            DecodeFormat::Signal => "gbrpf32le",
            DecodeFormat::Rgb8 => "rgb24",
            DecodeFormat::Yuv420 => "yuv420p",
        };
        let download = if self.backend == DecodeBackend::Cuda {
            "hwdownload,format=nv12,"
        } else {
            ""
        };
        let scale = format!("scale={}:{}:flags=neighbor,", dimensions[0], dimensions[1]);
        // Subsampled YUV reconstruction requires even dimensions. For odd
        // requests, reconstruct at native size before nearest float-RGB scaling.
        let even = dimensions.iter().all(|n| n.is_multiple_of(2));
        let (before, after) = if even {
            (scale.as_str(), "")
        } else {
            ("", scale.as_str())
        };
        let filter = if format == DecodeFormat::Yuv420 {
            format!("{download}select=gte(n\\,{offset}),format=yuv420p")
        } else {
            format!(
                "{download}select=gte(n\\,{offset}),{before}zscale=matrixin=709:transferin=709:primariesin=709:rangein=limited:matrix=gbr:transfer={transfer}:primaries=709:range=full,format=gbrpf32le,{after}format={pixel_format}"
            )
        };
        let mut cmd = base("ffmpeg");
        if self.backend == DecodeBackend::Cuda {
            cmd.args([
                "-hwaccel",
                "cuda",
                "-hwaccel_device",
                "0",
                "-hwaccel_output_format",
                "cuda",
            ]);
        }
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
            &(source.info.frames - frame).to_string(),
            "-fps_mode",
            "passthrough",
            // Bound the rawvideo output encoder too: its automatic frame-thread
            // queue otherwise retains many full float images behind the pipe.
            "-threads:v",
            "1",
            "-f",
            "rawvideo",
            "-pix_fmt",
            pixel_format,
            "pipe:1",
        ])
        .stdout(Stdio::piped());
        let frame_bytes = format.bytes(dimensions) as usize;
        let stream = crate::video_stream::Stream::new(cmd, frame_bytes)?;
        self.statistics.launches += 1;
        Ok(ActiveStream {
            fingerprint: source.fingerprint.clone(),
            info: source.info.clone(),
            dimensions,
            format,
            next: frame,
            stream,
            pin: self.pinned.back().unwrap().clone(),
        })
    }
}

impl Drop for Decoder {
    fn drop(&mut self) {
        DECODED_BYTES.fetch_sub(self.retained_bytes(), Ordering::AcqRel);
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
    _disk: Arc<crate::budget::Lease>,
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
        let disk = crate::budget::reserve_delivery(MAX_FILE)?;
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
            _disk: disk,
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
