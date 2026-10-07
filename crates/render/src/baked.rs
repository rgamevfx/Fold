//! Immutable, mipmapped working textures. The versioned cache is not project
//! content: originals and exact source-time keys remain authoritative.
use fold_media::Cancel;
use serde::{Deserialize, Serialize};
use std::{path::Path, sync::Arc};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum Format {
    #[default]
    Auto,
    Bc7,
    Float16,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Level {
    pub width: u32,
    pub height: u32,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug)]
pub struct Texture {
    pub format: Format,
    pub aces: bool,
    pub config: Option<String>,
    pub levels: Vec<Level>,
    pub identity: String,
    _storage: Arc<fold_media::budget::Lease>,
}
#[derive(Serialize, Deserialize)]
struct Header {
    version: u32,
    format: Format,
    aces: bool,
    config: Option<String>,
    levels: Vec<[u32; 3]>,
}
const LIMIT: usize = 192 * 1024 * 1024;
type Verified = (std::path::PathBuf, String, fold_media::exr::cache::Stamp);
static VERIFIED: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<Verified>>> =
    std::sync::OnceLock::new();
fn remember(path: &Path, identity: &str, stamp: fold_media::exr::cache::Stamp) {
    let mut entries = VERIFIED.get_or_init(Default::default).lock().unwrap();
    entries.retain(|e| e.0 != path);
    if entries.len() >= 4096 {
        entries.pop_front();
    }
    entries.push_back((path.into(), identity.into(), stamp));
}
impl Texture {
    pub fn dimensions(&self) -> [u32; 2] {
        [self.levels[0].width, self.levels[0].height]
    }
    pub fn bytes(&self) -> u64 {
        self.levels.iter().map(|l| l.bytes.len() as u64).sum()
    }
    pub fn validate(&self) -> Result<(), String> {
        if self.format == Format::Auto
            || self.levels.is_empty()
            || self.levels.len() > 13
            || self.bytes() > LIMIT as u64
        {
            return Err("invalid baked texture".into());
        }
        let [w, h] = self.dimensions();
        if w == 0 || h == 0 || w > 4096 || h > 4096 {
            return Err("baked texture dimensions exceed 4096".into());
        }
        if self.levels.len() != (32 - w.max(h).leading_zeros()) as usize {
            return Err("incomplete texture mip chain".into());
        }
        for (i, l) in self.levels.iter().enumerate() {
            let dims = [(w >> i).max(1), (h >> i).max(1)];
            let expected = if self.format == Format::Bc7 {
                dims[0].div_ceil(4) * dims[1].div_ceil(4) * 16
            } else {
                dims[0] * dims[1] * 8
            };
            if [l.width, l.height] != dims || l.bytes.len() != expected as usize {
                return Err("invalid texture mip size".into());
            }
        }
        Ok(())
    }
    pub fn encode(
        frame: &crate::Frame,
        requested: Format,
        cancel: &Cancel,
    ) -> Result<Self, String> {
        Self::encode_with(frame, requested, cancel, |rgba, w, h, cancel| {
            let mut blocks = vec![0u8; (w * h) as usize];
            for row in 0..h / 4 {
                cancel.check()?;
                block_compression::encode::compress_rgba8(
                    block_compression::CompressionVariant::BC7(
                        block_compression::BC7Settings::alpha_fast(),
                    ),
                    &rgba[(row * 4 * w * 4) as usize..],
                    &mut blocks[(row * w * 4) as usize..((row + 1) * w * 4) as usize],
                    w,
                    4,
                    w * 4,
                );
            }
            Ok(blocks)
        })
    }
    pub(crate) fn encode_with(
        frame: &crate::Frame,
        requested: Format,
        cancel: &Cancel,
        mut compress: impl FnMut(&[u8], u32, u32, &Cancel) -> Result<Vec<u8>, String>,
    ) -> Result<Self, String> {
        let dims = [frame.width, frame.height];
        if dims.iter().any(|v| *v == 0 || *v > 4096) {
            return Err("bake resolution must be 1..4096".into());
        }
        let sdr = frame
            .pixels
            .iter()
            .flatten()
            .all(|v| (0.0..=1.0).contains(v));
        let format = match requested {
            Format::Auto => {
                if sdr {
                    Format::Bc7
                } else {
                    Format::Float16
                }
            }
            Format::Bc7 if !sdr => {
                return Err(
                    "BC7 cannot preserve HDR or negative RGB; choose Auto or Float16".into(),
                );
            }
            v => v,
        };
        let _scratch = fold_media::budget::reserve_working(frame.pixels.len() as u64 * 32)?;
        let mut data = frame.pixels.clone();
        let [mut w, mut h] = dims;
        let mut levels = vec![];
        loop {
            cancel.check()?;
            let bytes = if format == Format::Bc7 {
                let pw = w.div_ceil(4) * 4;
                let ph = h.div_ceil(4) * 4;
                let mut rgba = vec![0u8; (pw * ph * 4) as usize];
                for y in 0..ph {
                    cancel.check()?;
                    for x in 0..pw {
                        let p = data[(y.min(h - 1) * w + x.min(w - 1)) as usize];
                        let i = ((y * pw + x) * 4) as usize;
                        for c in 0..4 {
                            rgba[i + c] = (if c == 3 { p[c] } else { encode_transfer(p[c]) }
                                * 255.0)
                                .round()
                                .clamp(0.0, 255.0) as u8;
                        }
                    }
                }
                compress(&rgba, pw, ph, cancel)?
            } else {
                let mut bytes = Vec::with_capacity(data.len() * 8);
                for p in &data {
                    for v in p {
                        let half = half::f16::from_f32(*v);
                        if !half.is_finite() {
                            return Err("texture exceeds Float16 range".into());
                        }
                        bytes.extend(half.to_bits().to_le_bytes());
                    }
                }
                bytes
            };
            levels.push(Level {
                width: w,
                height: h,
                bytes,
            });
            if w == 1 && h == 1 {
                break;
            }
            let nw = (w / 2).max(1);
            let nh = (h / 2).max(1);
            let mut next = Vec::with_capacity((nw * nh) as usize);
            // Area average in premultiplied scene-linear space, including odd edges.
            for y in 0..nh {
                cancel.check()?;
                for x in 0..nw {
                    let x0 = x * w / nw;
                    let x1 = (x + 1) * w / nw;
                    let y0 = y * h / nh;
                    let y1 = (y + 1) * h / nh;
                    let mut p = [0.; 4];
                    for yy in y0..y1 {
                        for xx in x0..x1 {
                            for c in 0..4 {
                                p[c] += data[(yy * w + xx) as usize][c];
                            }
                        }
                    }
                    for v in &mut p {
                        *v /= ((x1 - x0) * (y1 - y0)) as f32;
                    }
                    next.push(p);
                }
            }
            data = next;
            w = nw;
            h = nh;
        }
        let mut result = Self {
            format,
            aces: frame.working_space() == crate::WorkingSpace::AcesCg,
            config: frame.config_identity().map(str::to_owned),
            _storage: fold_media::budget::reserve_working(
                levels.iter().map(|l| l.bytes.len() as u64).sum(),
            )?,
            levels,
            identity: String::new(),
        };
        result.validate()?;
        result.identity = fold_media::content_hash(&result.serialize()?);
        Ok(result)
    }
    fn serialize(&self) -> Result<Vec<u8>, String> {
        let header = serde_json::to_vec(&Header {
            version: 1,
            format: self.format,
            aces: self.aces,
            config: self.config.clone(),
            levels: self
                .levels
                .iter()
                .map(|l| [l.width, l.height, l.bytes.len() as u32])
                .collect(),
        })
        .map_err(|e| e.to_string())?;
        let mut bytes = b"FOLDTEX1".to_vec();
        bytes.extend((header.len() as u32).to_le_bytes());
        bytes.extend(header);
        for l in &self.levels {
            bytes.extend(&l.bytes);
        }
        Ok(bytes)
    }
    pub fn save(&self, path: &Path, cancel: &Cancel) -> Result<(), String> {
        use std::io::Write;
        cancel.check()?;
        self.validate()?;
        let bytes = self.serialize()?;
        if fold_media::content_hash(&bytes) != self.identity {
            return Err("texture content identity changed".into());
        }
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(path)
            .map_err(|e| e.to_string())?;
        file.write_all(&bytes).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        cancel.check()?;
        remember(
            path,
            &self.identity,
            fold_media::exr::cache::Stamp::read(path, cancel)?,
        );
        Ok(())
    }
    pub fn load(path: &Path, identity: &str, cancel: &Cancel) -> Result<Arc<Self>, String> {
        type Cached = (
            std::path::PathBuf,
            String,
            fold_media::exr::cache::Stamp,
            Arc<Texture>,
        );
        static CACHE: std::sync::OnceLock<std::sync::Mutex<std::collections::VecDeque<Cached>>> =
            std::sync::OnceLock::new();
        let stamp = fold_media::exr::cache::Stamp::read(path, cancel)?;
        let cache = CACHE.get_or_init(Default::default);
        {
            let mut entries = cache.lock().map_err(|_| "texture cache poisoned")?;
            if let Some(i) = entries
                .iter()
                .position(|e| e.0 == path && e.1 == identity && e.2 == stamp)
            {
                let entry = entries.remove(i).unwrap();
                let texture = entry.3.clone();
                entries.push_back(entry);
                return Ok(texture);
            }
        }
        // Digest verification survives decoded-cache eviction. The small ledger
        // retains no pixel data and a strong file stamp invalidates every change.
        let verified = VERIFIED
            .get_or_init(Default::default)
            .lock()
            .map_err(|_| "texture verification cache poisoned")?
            .iter()
            .any(|e| e.0 == path && e.1 == identity && e.2 == stamp);
        let texture = Self::load_uncached(path, identity, !verified, cancel)?;
        remember(path, identity, stamp.clone());
        let mut entries = cache.lock().map_err(|_| "texture cache poisoned")?;
        entries.retain(|e| e.0 != path);
        while entries.iter().map(|e| e.3.bytes()).sum::<u64>() + texture.bytes() > 64 * 1024 * 1024
            || entries.len() >= 128
        {
            if entries.pop_front().is_none() {
                break;
            }
        }
        if texture.bytes() <= 64 * 1024 * 1024 {
            entries.push_back((path.into(), identity.into(), stamp, texture.clone()));
        }
        Ok(texture)
    }
    fn load_uncached(
        path: &Path,
        identity: &str,
        verify: bool,
        cancel: &Cancel,
    ) -> Result<Arc<Self>, String> {
        use std::io::Read;
        cancel.check()?;
        let length = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
        if length > LIMIT as u64 + 32768 {
            return Err("texture cache file too large".into());
        }
        let _read = fold_media::budget::reserve_working(length)?;
        let mut bytes = vec![];
        std::fs::File::open(path)
            .map_err(|_| "Baked texture missing; rebuild or clear the bake")?
            .take((LIMIT + 65536) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() < 12
            || bytes.len() > LIMIT + 32768
            || &bytes[..8] != b"FOLDTEX1"
            || verify && fold_media::content_hash(&bytes) != identity
        {
            return Err("Baked texture is damaged; rebuild it".into());
        }
        let n = u32::from_le_bytes(bytes[8..12].try_into().unwrap()) as usize;
        if n > 32768 || 12 + n > bytes.len() {
            return Err("invalid texture header".into());
        }
        let header: Header =
            serde_json::from_slice(&bytes[12..12 + n]).map_err(|e| e.to_string())?;
        if header.version != 1 {
            return Err("unsupported texture cache version".into());
        }
        let mut offset = 12 + n;
        let mut levels = vec![];
        for [width, height, length] in header.levels {
            let end = offset
                .checked_add(length as usize)
                .ok_or("texture length overflow")?;
            if end > bytes.len() {
                return Err("truncated texture".into());
            }
            levels.push(Level {
                width,
                height,
                bytes: bytes[offset..end].to_vec(),
            });
            offset = end;
        }
        if offset != bytes.len() {
            return Err("unexpected texture bytes".into());
        }
        let texture = Self {
            format: header.format,
            aces: header.aces,
            config: header.config,
            _storage: fold_media::budget::reserve_working(
                levels.iter().map(|l| l.bytes.len() as u64).sum(),
            )?,
            levels,
            identity: identity.into(),
        };
        texture.validate()?;
        cancel.check()?;
        Ok(Arc::new(texture))
    }
    pub fn decode(&self, level: usize, cancel: &Cancel) -> Result<crate::Frame, String> {
        self.validate()?;
        cancel.check()?;
        let l = &self.levels[level.min(self.levels.len() - 1)];
        let storage =
            fold_media::budget::reserve_working(u64::from(l.width) * u64::from(l.height) * 16)?;
        let mut pixels = vec![[0.; 4]; (l.width * l.height) as usize];
        if self.format == Format::Bc7 {
            let bw = l.width.div_ceil(4);
            for (i, block) in l.bytes.chunks_exact(16).enumerate() {
                cancel.check()?;
                let mut rgba = [0u8; 64];
                block_compression::decode::decode_block_bc7(block, &mut rgba, 16);
                let bx = i as u32 % bw * 4;
                let by = i as u32 / bw * 4;
                for y in 0..4 {
                    for x in 0..4 {
                        if bx + x < l.width && by + y < l.height {
                            let q = ((y * 4 + x) * 4) as usize;
                            pixels[((by + y) * l.width + bx + x) as usize] =
                                std::array::from_fn(|c| {
                                    if c == 3 {
                                        rgba[q + c] as f32 / 255.0
                                    } else {
                                        decode_transfer(rgba[q + c] as f32 / 255.0)
                                    }
                                });
                        }
                    }
                }
            }
        } else {
            for (p, bytes) in pixels.iter_mut().zip(l.bytes.chunks_exact(8)) {
                for c in 0..4 {
                    p[c] =
                        half::f16::from_bits(u16::from_le_bytes([bytes[c * 2], bytes[c * 2 + 1]]))
                            .to_f32();
                    fold_color::validate_pixel(*p)?;
                }
            }
        }
        Ok(crate::Frame {
            width: l.width,
            height: l.height,
            pixels,
            _storage: storage,
            working_space: if self.aces {
                crate::WorkingSpace::AcesCg
            } else {
                crate::WorkingSpace::LegacyLinearSrgb
            },
            config_identity: self.config.clone(),
            timing: None,
        })
    }
}
fn encode_transfer(v: f32) -> f32 {
    if v <= 0.0031308 {
        v * 12.92
    } else {
        1.055 * v.powf(1.0 / 2.4) - 0.055
    }
}
fn decode_transfer(v: f32) -> f32 {
    if v <= 0.04045 {
        v / 12.92
    } else {
        ((v + 0.055) / 1.055).powf(2.4)
    }
}

/// A shared ordered material batch. Each instance's offset.w is its texture layer.
#[derive(Clone, Debug)]
pub struct Batch {
    pub textures: Vec<Arc<Texture>>,
    pub draw: crate::textured::Draw,
}
impl Batch {
    pub fn validate(&self) -> Result<(), String> {
        self.draw.validate()?;
        if self.textures.is_empty() || self.textures.len() > 64 {
            return Err("baked texture batch requires 1..64 images".into());
        }
        let first = &self.textures[0];
        first.validate()?;
        for t in &self.textures {
            t.validate()?;
            if t.dimensions() != first.dimensions()
                || t.format != first.format
                || t.aces != first.aces
                || t.config != first.config
            {
                return Err("texture array members must share size, format and color".into());
            }
        }
        if self.draw.instances.iter().any(|i| {
            i.offset[3] < 0.
                || i.offset[3] >= self.textures.len() as f32
                || i.offset[3].fract() != 0.
        }) {
            return Err("invalid texture layer".into());
        }
        Ok(())
    }
    pub fn rasterize(
        &self,
        driver: Option<&crate::Frame>,
        width: u32,
        height: u32,
        cancel: &Cancel,
        aces: bool,
    ) -> Result<Vec<[f32; 4]>, String> {
        self.validate()?;
        if u64::from(width) * u64::from(height) * self.draw.instances.len() as u64
            > 256 * 1024 * 1024
        {
            return Err("baked CPU compositing work budget exceeded".into());
        }
        let mut out = vec![[0.; 4]; (width * height) as usize];
        // Keep only the current layer expanded on CPU. A compressed array can
        // fit comfortably in GPU memory while its full float expansion cannot.
        let mut decoded: Option<(usize, Vec<crate::Frame>)> = None;
        let _scratch =
            fold_media::budget::reserve_working(u64::from(width) * u64::from(height) * 16)?;
        for instance in &self.draw.instances {
            cancel.check()?;
            let layer = instance.offset[3] as usize;
            if decoded.as_ref().is_none_or(|(index, _)| *index != layer) {
                drop(decoded.take());
                let source = &self.textures[layer];
                let levels = (0..source.levels.len())
                    .map(|i| source.decode(i, cancel))
                    .collect::<Result<Vec<_>, _>>()?;
                decoded = Some((layer, levels));
            }
            let texture = &decoded.as_ref().unwrap().1;
            let mut draw = self.draw.clone();
            draw.instances = vec![*instance];
            let pixels = crate::textured::rasterize_mips(
                &draw,
                texture,
                driver.unwrap_or(&texture[0]),
                width,
                height,
                cancel,
                aces,
            )?;
            for (dst, src) in out.iter_mut().zip(pixels) {
                for c in 0..4 {
                    dst[c] = src[c] + dst[c] * (1. - src[3]);
                }
            }
        }
        Ok(out)
    }
}
pub fn fullscreen(texture: Arc<Texture>, dimensions: [u32; 2]) -> Batch {
    use crate::textured::*;
    let [w, h] = dimensions.map(|v| v as f32);
    let mesh = Arc::new(Mesh {
        vertices: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]
            .map(|uv| Vertex {
                position: [uv[0] * w, uv[1] * h],
                uv,
                sample: uv,
                center: [w / 2., h / 2.],
            })
            .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
    });
    Batch {
        textures: vec![texture],
        draw: Draw::new(mesh),
    }
}
