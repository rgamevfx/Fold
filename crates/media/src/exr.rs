//! Bounded flat OpenEXR adapter. Only requested sample channels are retained.
//! Compression blocks may contain additional channels; that decompression cost
//! is distinct from retained decoded planes and the viewer playback cache.
use crate::{Cancel, budget};
use ::exr::prelude::*;
use serde::{Deserialize, Serialize};
use std::result::Result;
use std::{
    io::{Cursor, Read},
    path::{Path, PathBuf},
    sync::Arc,
};

const MAX_FILE_BYTES: u64 = 128 * 1024 * 1024;
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
}
impl Info {
    pub fn validate(&self) -> Result<(), String> {
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
        Ok(())
    }
}
#[derive(Clone, Debug)]
pub struct Source {
    pub path: PathBuf,
    pub fingerprint: String,
    pub info: Info,
}

struct Layout {
    info: Info,
    scratch: u64,
    retained_scratch: u64,
}

fn metadata(reader: impl Read) -> Result<Layout, String> {
    let metadata = MetaData::read_from_buffered(reader, true).map_err(|e| e.to_string())?;
    if metadata.headers.len() != 1 {
        return Err(
            "Multipart EXR is not supported yet; use named channels in one flat part".into(),
        );
    }
    let header = &metadata.headers[0];
    if header.deep {
        return Err("Deep EXR requires a deep-image operator".into());
    }
    let display = header.shared_attributes.display_window;
    let mut channels = Vec::new();
    for channel in &header.channels.list {
        if channel.sampling != Vec2(1, 1) {
            return Err("Subsampled EXR channels are not supported".into());
        }
        let source = channel.name.to_string();
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
            source,
            color,
        });
    }
    let info = Info {
        dimensions: [
            u32::try_from(display.size.0).map_err(|_| "EXR width overflow")?,
            u32::try_from(display.size.1).map_err(|_| "EXR height overflow")?,
        ],
        display_origin: [display.position.0, display.position.1],
        data_origin: [
            header.own_attributes.layer_position.0,
            header.own_attributes.layer_position.1,
        ],
        data_dimensions: [
            u32::try_from(header.layer_size.0).map_err(|_| "EXR data width overflow")?,
            u32::try_from(header.layer_size.1).map_err(|_| "EXR data height overflow")?,
        ],
        channels,
    };
    info.validate()?;
    let block = header.max_block_pixel_size();
    let block_bytes = (block.0 as u64)
        .checked_mul(block.1 as u64)
        .and_then(|n| n.checked_mul(header.channels.bytes_per_pixel as u64))
        .ok_or("EXR block size overflow")?;
    let part_bytes = u64::from(info.data_dimensions[0])
        * u64::from(info.data_dimensions[1])
        * info.channels.len() as u64
        * 4;
    // The pinned decoder reads blocks sequentially. ZIP/RLE need the compressed
    // block, an expanding output buffer, and a retained byte-interleave buffer.
    // Other codecs retain the conservative whole-part reservation pending their
    // individual resource qualification. Include fixed codec/table overhead.
    let scratch = match header.compression {
        Compression::Uncompressed | Compression::ZIP1 | Compression::ZIP16 | Compression::RLE => {
            block_bytes.checked_mul(4)
        }
        _ => part_bytes.checked_add(block_bytes.saturating_mul(4)),
    }
    .and_then(|n| n.checked_add(1024 * 1024))
    .ok_or("EXR scratch size overflow")?;
    Ok(Layout {
        info,
        scratch,
        retained_scratch: block_bytes.saturating_add(128),
    })
}

thread_local! {
    // exr 1.74.2 keeps its byte-interleave buffer for the lifetime of the thread.
    // Grow matching leases monotonically; returning from decode must not release
    // accounting for storage that the dependency still owns.
    static RETAINED_SCRATCH: std::cell::RefCell<(u64, Vec<Arc<budget::Lease>>)> = const {
        std::cell::RefCell::new((0, Vec::new()))
    };
}
fn reserve_retained_scratch(bytes: u64) -> Result<(), String> {
    RETAINED_SCRATCH.with(|state| {
        let mut state = state.borrow_mut();
        if bytes > state.0 {
            let lease = budget::reserve_working(bytes - state.0)?;
            state.1.push(lease);
            state.0 = bytes;
        }
        Ok(())
    })
}
pub fn inspect(path: &Path, cancel: &Cancel) -> Result<Info, String> {
    cancel.check()?;
    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX_FILE_BYTES {
        return Err("EXR exceeds 128 MiB compressed source limit".into());
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
) -> Result<Pixels, String> {
    source.info.validate()?;
    cancel.check()?;
    let count = u64::from(dimensions[0]) * u64::from(dimensions[1]);
    if count == 0 || count > crate::MAX_PIXELS as u64 {
        return Err("Invalid EXR output dimensions".into());
    }
    let mut file = std::fs::File::open(&source.path).map_err(|e| e.to_string())?;
    let size = file.metadata().map_err(|e| e.to_string())?.len();
    if size > MAX_FILE_BYTES {
        return Err("EXR exceeds 128 MiB compressed source limit".into());
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
            return Err(format!("Missing EXR channel '{name}'"));
        }
    }
    let data_count =
        u64::from(source.info.data_dimensions[0]) * u64::from(source.info.data_dimensions[1]);
    let _scratch = budget::reserve_working(data_count * 16 + layout.scratch)?;
    reserve_retained_scratch(layout.retained_scratch)?;
    let output_lease = budget::DECODED.reserve(count * 16)?;
    // A missing name is guaranteed absent after checking the channel catalog.
    let missing = [
        "fold.__absent_r__",
        "fold.__absent_g__",
        "fold.__absent_b__",
        "fold.__absent_a__",
    ];
    let remap: [usize; 4] = std::array::from_fn(|i| {
        channels[i]
            .and_then(|name| channels[..i].iter().position(|c| *c == Some(name)))
            .unwrap_or(i)
    });
    let selected: [Option<&str>; 4] =
        std::array::from_fn(|i| if remap[i] == i { channels[i] } else { None });
    if source
        .info
        .channels
        .iter()
        .any(|c| missing.contains(&c.source.as_str()))
    {
        return Err("EXR contains reserved missing-channel marker".into());
    }
    let reader_cancel = cancel.clone();
    let image = read()
        .no_deep_data()
        .largest_resolution_level()
        .specific_channels()
        .optional(selected[0].unwrap_or(missing[0]), 0f32)
        .optional(selected[1].unwrap_or(missing[1]), 0f32)
        .optional(selected[2].unwrap_or(missing[2]), 0f32)
        .optional(selected[3].unwrap_or(missing[3]), 1f32)
        .collect_pixels(
            |size, _| (size.width(), vec![[0f32; 4]; size.area()]),
            |pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
                pixels.1[position.y() * pixels.0 + position.x()] = [r, g, b, a];
            },
        )
        .first_valid_layer()
        .all_attributes()
        .non_parallel()
        .pedantic()
        .from_buffered(Cancellable {
            inner: Cursor::new(bytes),
            cancel: reader_cancel,
        })
        .map_err(|e| e.to_string())?;
    let samples = image.layer_data.channel_data.pixels.1;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count as usize)
        .map_err(|_| "EXR output allocation failed")?;
    for y in 0..dimensions[1] {
        cancel.check()?;
        let sy = i64::from(y) * i64::from(source.info.dimensions[1]) / i64::from(dimensions[1])
            + i64::from(source.info.display_origin[1])
            - i64::from(source.info.data_origin[1]);
        for x in 0..dimensions[0] {
            let sx = i64::from(x) * i64::from(source.info.dimensions[0]) / i64::from(dimensions[0])
                + i64::from(source.info.display_origin[0])
                - i64::from(source.info.data_origin[0]);
            let pixel = if sx >= 0
                && sy >= 0
                && sx < i64::from(source.info.data_dimensions[0])
                && sy < i64::from(source.info.data_dimensions[1])
            {
                let pixel =
                    samples[sy as usize * source.info.data_dimensions[0] as usize + sx as usize];
                remap.map(|c| pixel[c])
            } else {
                [0.; 4]
            };
            if pixel.iter().any(|v| !v.is_finite()) {
                return Err("EXR contains nonfinite requested samples".into());
            }
            pixels.push(pixel);
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
