//! Bounded flat OpenEXR adapter. Only requested sample channels are retained.
//! Compression blocks may contain additional channels; that decompression cost
//! is distinct from retained decoded planes and the viewer playback cache.
mod blocks;
pub mod cache;

use crate::{Cancel, budget};
use ::exr::prelude::*;
use serde::{Deserialize, Serialize};
use std::result::Result;
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_FILE_BYTES: u64 = 256 * 1024 * 1024;
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub name: String,
    pub source: String,
    /// RGB component spelling, not a color-space/utility-data assignment.
    pub color: bool,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Info {
    pub dimensions: [u32; 2],
    pub display_origin: [i32; 2],
    pub data_origin: [i32; 2],
    pub data_dimensions: [u32; 2],
    pub channels: Vec<Channel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sequence: Option<crate::exr_sequence::Sequence>,
}
impl Info {
    /// Relink may replace file names and bytes, but not the authored channel,
    /// window or frame-number contract.
    pub fn compatible_with(&self, other: &Self) -> bool {
        self.compatible_pixels(other)
            && match (&self.sequence, &other.sequence) {
                (None, None) => true,
                (Some(a), Some(b)) => a.frames.keys().eq(b.frames.keys()),
                _ => false,
            }
    }
    pub fn compatible_pixels(&self, other: &Self) -> bool {
        self.dimensions == other.dimensions
            && self.display_origin == other.display_origin
            && self.data_origin == other.data_origin
            && self.data_dimensions == other.data_dimensions
            && self.channels == other.channels
    }
    pub fn validate(&self) -> Result<(), fold_foundation::Failure> {
        for dimensions in [self.dimensions, self.data_dimensions] {
            let count = u64::from(dimensions[0]) * u64::from(dimensions[1]);
            if count == 0 || count > crate::MAX_PIXELS as u64 {
                return Err("EXR window exceeds the pixel budget".into());
            }
        }
        if self.channels.is_empty() || self.channels.len() > 256 {
            return Err("EXR requires 1..256 channels".into());
        }
        let mut names = std::collections::BTreeSet::new();
        for channel in &self.channels {
            if channel.name.len() > 255 || !names.insert(&channel.name) {
                return Err("EXR has duplicate or oversized channel names".into());
            }
        }
        if let Some(sequence) = &self.sequence {
            sequence.validate()?;
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Source {
    pub path: PathBuf,
    pub fingerprint: String,
    pub info: Info,
}

struct Part {
    header: ::exr::meta::header::Header,
    sources: Vec<String>,
    scratch: u64,
    retained_scratch: u64,
}
struct Layout {
    info: Info,
    parts: Vec<Part>,
}

fn metadata(reader: impl Read) -> Result<Layout, fold_foundation::Failure> {
    let metadata = MetaData::read_from_buffered(reader, true).map_err(|e| e.to_string())?;
    let first = metadata.headers.first().ok_or("EXR has no parts")?;
    let display = first.shared_attributes.display_window;
    let multipart = metadata.requirements.has_multiple_layers;
    let mut channels = Vec::new();
    let mut parts = Vec::new();
    let mut minimum = [i32::MAX; 2];
    let mut maximum = [i64::MIN; 2];
    for header in metadata.headers {
        if header.deep {
            return Err("Deep EXR requires a deep-image operator".into());
        }
        if header.shared_attributes.display_window != display {
            return Err("EXR parts must share a display window".into());
        }
        let prefix = if multipart {
            header
                .own_attributes
                .layer_name
                .as_ref()
                .ok_or("Multipart EXR requires named parts")?
                .to_string()
        } else {
            String::new()
        };
        let mut sources = Vec::new();
        for channel in &header.channels.list {
            if channel.sampling != Vec2(1, 1) {
                return Err("Subsampled EXR channels are not supported".into());
            }
            let raw = channel.name.to_string();
            let source = if multipart {
                format!("{prefix}.{raw}")
            } else {
                raw
            };
            let (layer, component) = source.rsplit_once('.').unwrap_or((
                if source == "Z" { "depth" } else { "rgba" },
                source.as_str(),
            ));
            let (component, color) = match component {
                "R" | "red" => ("red", true),
                "G" | "green" => ("green", true),
                "B" | "blue" => ("blue", true),
                "A" | "alpha" => ("alpha", false),
                value => (value, false),
            };
            channels.push(Channel {
                name: format!("{layer}.{component}"),
                source: source.clone(),
                color,
            });
            sources.push(source);
        }
        let origin = header.own_attributes.layer_position;
        let size = header.layer_size;
        let area = (size.0 as u64)
            .checked_mul(size.1 as u64)
            .ok_or("EXR part size overflow")?;
        if area == 0 || area > crate::MAX_PIXELS as u64 {
            return Err("EXR part exceeds pixel budget".into());
        }
        for axis in 0..2 {
            let position = [origin.0, origin.1][axis];
            minimum[axis] = minimum[axis].min(position);
            maximum[axis] = maximum[axis].max(i64::from(position) + [size.0, size.1][axis] as i64);
        }
        let block = header.max_block_pixel_size();
        let block_bytes = (block.0 as u64)
            .checked_mul(block.1 as u64)
            .and_then(|n| n.checked_mul(header.channels.bytes_per_pixel as u64))
            .ok_or("EXR block size overflow")?;
        let scratch = match header.compression {
            Compression::Uncompressed
            | Compression::ZIP1
            | Compression::ZIP16
            | Compression::RLE => block_bytes.checked_mul(4),
            _ => (area * header.channels.list.len() as u64 * 4)
                .checked_add(block_bytes.saturating_mul(4)),
        }
        .and_then(|n| n.checked_add(1024 * 1024))
        .ok_or("EXR scratch size overflow")?;
        parts.push(Part {
            header,
            sources,
            scratch,
            retained_scratch: block_bytes.saturating_add(128),
        });
    }
    if multipart {
        channels.sort_by(|a, b| a.name.cmp(&b.name));
    }
    let info = Info {
        dimensions: [
            u32::try_from(display.size.0).map_err(|_| "EXR width overflow")?,
            u32::try_from(display.size.1).map_err(|_| "EXR height overflow")?,
        ],
        display_origin: [display.position.0, display.position.1],
        data_origin: minimum,
        data_dimensions: [
            u32::try_from(maximum[0] - i64::from(minimum[0])).map_err(|_| "EXR bounds overflow")?,
            u32::try_from(maximum[1] - i64::from(minimum[1])).map_err(|_| "EXR bounds overflow")?,
        ],
        channels,
        sequence: None,
    };
    info.validate()?;
    Ok(Layout { info, parts })
}

thread_local! {
    // exr 1.74.2 keeps its byte-interleave buffer for the lifetime of the thread.
    // Grow matching leases monotonically; returning from decode must not release
    // accounting for storage that the dependency still owns.
    static RETAINED_SCRATCH: std::cell::RefCell<(u64, Vec<Arc<budget::Lease>>)> = const {
        std::cell::RefCell::new((0, Vec::new()))
    };
}
fn retain_scratch(bytes: u64, lease: Arc<budget::Lease>) {
    RETAINED_SCRATCH.with(|state| {
        let mut state = state.borrow_mut();
        if bytes > state.0 {
            let growth = bytes - state.0;
            state.1.push(lease.shrink(growth));
            state.0 = bytes;
        }
    })
}
pub fn inspect(path: &Path, cancel: &Cancel) -> Result<Info, fold_foundation::Failure> {
    cancel.check()?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_BYTES {
        return Err("EXR exceeds 256 MiB compressed source limit".into());
    }
    let result = metadata(std::io::BufReader::new(file))?;
    cancel.check()?;
    Ok(result.info)
}
#[derive(Debug)]
pub struct Pixels {
    pub pixels: Vec<[f32; 4]>,
    _lease: Arc<budget::Lease>,
}

/// None supplies zero (RGB) or one (alpha). Data channels bypass color transforms;
/// the caller assigns color processing only to an explicitly selected RGB tuple.
pub fn decode(
    source: &Source,
    channels: [Option<&str>; 4],
    dimensions: [u32; 2],
    cancel: &Cancel,
) -> Result<Pixels, fold_foundation::Failure> {
    source.info.validate()?;
    cancel.check()?;
    let count = u64::from(dimensions[0]) * u64::from(dimensions[1]);
    if count == 0 || count > crate::MAX_PIXELS as u64 {
        return Err("Invalid EXR output dimensions".into());
    }
    let mut file = std::fs::File::open(&source.path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > MAX_FILE_BYTES {
        return Err("EXR exceeds 256 MiB compressed source limit".into());
    }
    let _source_lease = budget::reserve_working(size)?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(size as usize)
        .map_err(|_| "EXR source allocation failed")?;
    let mut block = [0u8; 65536];
    loop {
        cancel.check()?;
        let read = file.read(&mut block).map_err(|e| e.to_string())?;
        if read == 0 {
            break;
        }
        if bytes.len() as u64 + read as u64 > size {
            return Err("EXR source changed during read".into());
        }
        bytes.extend_from_slice(&block[..read]);
    }
    if format!("sha256:{}", crate::content_hash(&bytes)) != source.fingerprint {
        return Err("EXR source fingerprint changed; reimport required".into());
    }
    let layout = metadata(Cursor::new(&bytes))?;
    if layout.info != source.info {
        return Err("EXR source metadata changed".into());
    }
    for name in channels.into_iter().flatten() {
        if !source.info.channels.iter().any(|c| c.source == name) {
            return Err(format!("Missing EXR channel '{name}'").into());
        }
    }
    let output_lease = budget::DECODED.reserve(count * 16)?;
    let mut pixels = Vec::new();

    let missing = [
        "fold.__absent_r__",
        "fold.__absent_g__",
        "fold.__absent_b__",
        "fold.__absent_a__",
    ];
    for (part_index, part) in layout.parts.into_iter().enumerate() {
        cancel.check()?;
        let selected: [Option<&str>; 4] = std::array::from_fn(|i| {
            channels[i]
                .and_then(|name| part.sources.iter().position(|s| s == name))
                .map(|index| part.header.channels.list[index].name.as_slice())
                .and_then(|bytes| std::str::from_utf8(bytes).ok())
        });
        if selected.iter().all(Option::is_none)
            && !(part_index == 0 && channels.iter().all(Option::is_none))
        {
            continue;
        }
        if part
            .header
            .channels
            .list
            .iter()
            .any(|c| missing.iter().any(|m| c.name == **m))
        {
            return Err("EXR contains reserved missing-channel marker".into());
        }
        let remap: [usize; 4] = std::array::from_fn(|i| {
            selected[i]
                .and_then(|name| selected[..i].iter().position(|c| *c == Some(name)))
                .unwrap_or(i)
        });
        let unique: [Option<&str>; 4] =
            std::array::from_fn(|i| if remap[i] == i { selected[i] } else { None });
        let size = part.header.layer_size;
        let origin = part.header.own_attributes.layer_position;
        let direct = dimensions == source.info.dimensions
            && dimensions == [size.0 as u32, size.1 as u32]
            && source.info.display_origin == [origin.0, origin.1]
            && (0..4).all(|i| selected[i].is_some() == channels[i].is_some());
        let _samples = if direct {
            None
        } else {
            Some(budget::reserve_working(size.area() as u64 * 16)?)
        };
        let chunks = ::exr::block::read(
            Cancellable {
                inner: Cursor::new(&bytes),
                cancel: cancel.clone(),
            },
            true,
        )
        .map_err(|e| e.to_string())?
        .filter_chunks(true, |_, _, block| {
            block.layer == part_index && block.level == Vec2(0, 0)
        })
        .map_err(|e| e.to_string())?;
        let block = part.header.max_block_pixel_size();
        let samples = blocks::read(
            chunks,
            blocks::Selection {
                names: unique.map(|name| name.map(str::to_owned)),
                remap,
                opaque: channels[3].is_none(),
            },
            size,
            part.scratch + (block.area() as u64 * 16),
            part.retained_scratch,
            cancel,
        )?;
        if direct {
            return Ok(Pixels {
                pixels: samples,
                _lease: output_lease,
            });
        }
        if pixels.is_empty() {
            pixels
                .try_reserve_exact(count as usize)
                .map_err(|_| "EXR output allocation failed")?;
            pixels.resize(count as usize, [0.; 4]);
        }
        for y in 0..dimensions[1] {
            cancel.check()?;
            let sy = i64::from(y) * i64::from(source.info.dimensions[1]) / i64::from(dimensions[1])
                + i64::from(source.info.display_origin[1])
                - i64::from(origin.1);
            for x in 0..dimensions[0] {
                let sx = i64::from(x) * i64::from(source.info.dimensions[0])
                    / i64::from(dimensions[0])
                    + i64::from(source.info.display_origin[0])
                    - i64::from(origin.0);
                if sx < 0 || sy < 0 || sx >= size.0 as i64 || sy >= size.1 as i64 {
                    continue;
                }
                let sample = samples[sy as usize * size.0 + sx as usize];
                let pixel = &mut pixels[y as usize * dimensions[0] as usize + x as usize];
                if channels[3].is_none() {
                    pixel[3] = 1.;
                }
                for i in 0..4 {
                    if selected[i].is_some() {
                        let value = sample[i];
                        if !value.is_finite() {
                            return Err("EXR contains nonfinite requested samples".into());
                        }
                        pixel[i] = value;
                    }
                }
            }
        }
    }
    Ok(Pixels {
        pixels,
        _lease: output_lease,
    })
}
struct Cancellable<T> {
    inner: T,
    cancel: Cancel,
}
impl<T: Read> Read for Cancellable<T> {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.cancel.check().map_err(std::io::Error::other)?;
        self.inner.read(bytes)
    }
}
impl<T: std::io::Seek> std::io::Seek for Cancellable<T> {
    fn seek(&mut self, position: std::io::SeekFrom) -> std::io::Result<u64> {
        self.inner.seek(position)
    }
}
