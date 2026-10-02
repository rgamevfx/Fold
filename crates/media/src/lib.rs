//! Bounded media adapters and opaque, full-range SDR sRGB RGB8 source images.
use std::sync::Arc;

pub mod audio;
pub mod ingest;
mod process;
pub mod thumbnail;
mod video;
mod wave;
pub use process::Cancel;
pub use video::{Decoder, Encoder, VideoInfo, VideoSource, fingerprint, inspect};
pub use wave::{WaveInfo, WaveSource, inspect_wave};

pub fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub const MAX_PIXELS: usize = 4_194_304;
pub const MAX_SOURCE_BYTES: usize = MAX_PIXELS * 3 + 4096;

/// Validated immutable RGB: sRGB RGB8 fixtures/legacy decode, or native BT.709
/// encoded float RGB after YUV reconstruction (`is_signal`). This is not a
/// working image or viewer cache entry; OCIO consumes `encoded_pixels` for ACES.
#[derive(Clone, Debug)]
pub struct RgbImage {
    dimensions: [u32; 2],
    rgb: Arc<[u8]>,
    signal: Option<Arc<[[f32; 4]]>>,
}

impl RgbImage {
    /// P6 fixtures/image sequences are one adapter, not the viewer cache format.
    fn ppm(bytes: &[u8]) -> Result<([u32; 2], &[u8]), &'static str> {
        if bytes.len() > MAX_SOURCE_BYTES {
            return Err("PPM exceeds source byte budget");
        }
        let mut position = 0;
        fn token<'a>(bytes: &'a [u8], position: &mut usize) -> &'a [u8] {
            loop {
                while bytes.get(*position).is_some_and(u8::is_ascii_whitespace) {
                    *position += 1;
                }
                if bytes.get(*position) != Some(&b'#') {
                    break;
                }
                while bytes.get(*position).is_some_and(|b| *b != b'\n') {
                    *position += 1;
                }
            }
            let start = *position;
            while bytes
                .get(*position)
                .is_some_and(|b| !b.is_ascii_whitespace() && *b != b'#')
            {
                *position += 1;
            }
            &bytes[start..*position]
        }
        if token(bytes, &mut position) != b"P6" {
            return Err("expected binary P6 PPM");
        }
        let mut number = || {
            let text = std::str::from_utf8(token(bytes, &mut position))
                .map_err(|_| "invalid PPM number")?;
            if text.is_empty() || !text.bytes().all(|b| b.is_ascii_digit()) {
                return Err("invalid PPM number");
            }
            text.parse::<u32>().map_err(|_| "invalid PPM number")
        };
        let width = number()?;
        let height = number()?;
        if number()? != 255 {
            return Err("only RGB8 PPM maxval 255 is supported");
        }
        let count = u64::from(width) * u64::from(height);
        if count == 0 || count > MAX_PIXELS as u64 {
            return Err("PPM resolution exceeds pixel budget");
        }
        // P6 requires exactly one whitespace separator before binary pixels.
        // Never skip arbitrary whitespace here: a pixel may itself be whitespace.
        if !bytes.get(position).is_some_and(u8::is_ascii_whitespace) {
            return Err("missing PPM raster separator");
        }
        position += 1;
        if position > 4096 {
            return Err("PPM header exceeds budget");
        }
        let rgb = &bytes[position..];
        if rgb.len() != count as usize * 3 {
            return Err("PPM raster length mismatch");
        }
        Ok(([width, height], rgb))
    }

    /// Header/length validation only: no decoded storage allocation or processing.
    pub fn ppm_dimensions(bytes: &[u8]) -> Result<[u32; 2], &'static str> {
        Self::ppm(bytes).map(|(size, _)| size)
    }
    pub fn decode_ppm(bytes: &[u8]) -> Result<Self, &'static str> {
        let (dimensions, rgb) = Self::ppm(bytes)?;
        Ok(Self {
            dimensions,
            rgb: rgb.into(),
            signal: None,
        })
    }

    pub fn from_rgb(dimensions: [u32; 2], rgb: Vec<u8>) -> Result<Self, &'static str> {
        let count = u64::from(dimensions[0]) * u64::from(dimensions[1]);
        if count == 0 || count > MAX_PIXELS as u64 || rgb.len() as u64 != count * 3 {
            return Err("invalid RGB8 frame dimensions or storage");
        }
        Ok(Self {
            dimensions,
            rgb: rgb.into(),
            signal: None,
        })
    }

    pub(crate) fn from_gbr(dimensions: [u32; 2], bytes: &[u8]) -> Result<Self, String> {
        let count = dimensions[0] as usize * dimensions[1] as usize;
        if count == 0 || count > MAX_PIXELS || bytes.len() != count * 12 {
            return Err("Invalid float decode planes".into());
        }
        let sample = |plane: usize, pixel: usize| {
            let start = (plane * count + pixel) * 4;
            f32::from_le_bytes(bytes[start..start + 4].try_into().unwrap())
        };
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|_| "Float decode allocation failed")?;
        for i in 0..count {
            let p = [sample(2, i), sample(0, i), sample(1, i), 1.];
            if p.iter().any(|v| !v.is_finite()) {
                return Err("Nonfinite decoder output".into());
            }
            pixels.push(p);
        }
        Ok(Self {
            dimensions,
            rgb: Arc::from([]),
            signal: Some(pixels.into()),
        })
    }
    pub fn encoded_pixels(&self) -> impl Iterator<Item = [f32; 4]> + '_ {
        (0..self.dimensions[0] as usize * self.dimensions[1] as usize).map(|i| {
            self.signal.as_ref().map_or_else(
                || {
                    [
                        self.rgb[i * 3] as f32 / 255.,
                        self.rgb[i * 3 + 1] as f32 / 255.,
                        self.rgb[i * 3 + 2] as f32 / 255.,
                        1.,
                    ]
                },
                |p| p[i],
            )
        })
    }
    pub fn is_signal(&self) -> bool {
        self.signal.is_some()
    }
    pub fn storage_bytes(&self) -> u64 {
        self.signal
            .as_ref()
            .map_or(self.rgb.len() as u64, |p| p.len() as u64 * 16)
    }

    pub fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }

    /// Legacy/reference input transfer conversion to linear Rec.709 primaries.
    /// ACES evaluation uses encoded pixels and an explicit OCIO input assignment.
    pub fn linear_pixels(&self) -> impl Iterator<Item = [f32; 4]> + '_ {
        // RGB8 has only 256 possible inputs. Cache the exact reference transfer
        // values rather than recomputing three powf calls per pixel per frame.
        static LINEAR: std::sync::OnceLock<[f32; 256]> = std::sync::OnceLock::new();
        let linear = LINEAR.get_or_init(|| {
            std::array::from_fn(|index| {
                let v = index as f32 / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            })
        });
        (0..self.dimensions[0] as usize * self.dimensions[1] as usize).map(|i| {
            if let Some(signal) = &self.signal {
                let decode = |v: f32| {
                    if v < 0.081 {
                        v / 4.5
                    } else {
                        ((v + 0.099) / 1.099).powf(1. / 0.45)
                    }
                };
                [
                    decode(signal[i][0]),
                    decode(signal[i][1]),
                    decode(signal[i][2]),
                    1.,
                ]
            } else {
                [
                    linear[self.rgb[i * 3] as usize],
                    linear[self.rgb[i * 3 + 1] as usize],
                    linear[self.rgb[i * 3 + 2] as usize],
                    1.,
                ]
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cached_input_transfer_matches_every_rgb8_reference_value() {
        let image =
            RgbImage::from_rgb([256, 1], (0..=255u8).flat_map(|v| [v; 3]).collect()).unwrap();
        for (index, pixel) in image.linear_pixels().enumerate() {
            let v = index as f32 / 255.;
            let expected = if v <= 0.04045 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            };
            assert_eq!(pixel, [expected, expected, expected, 1.]);
        }
    }
    #[test]
    fn strict_profile_and_binary_separator() {
        let image = RgbImage::decode_ppm(b"P6\n# fixture\n1 1\n255\n\x0a\x20\xff").unwrap();
        assert_eq!(image.dimensions(), [1, 1]);
        let pixel = image.linear_pixels().next().unwrap();
        assert_eq!(pixel[2..], [1.0, 1.0]);
        assert!(pixel[0] < pixel[1]);
        for bytes in [
            b"P3 1 1 255\nabc".as_slice(),
            b"P6 0 1 255\n",
            b"P6 1 1 65535\nabc",
            b"P6 1 1 255\nab",
            b"P6 1 1 255\nabcd",
            b"P6 4294967295 4294967295 255\n",
        ] {
            assert!(RgbImage::decode_ppm(bytes).is_err());
        }
    }
}
