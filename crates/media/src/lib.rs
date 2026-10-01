//! Bounded media adapters and opaque, full-range SDR sRGB RGB8 source images.
use std::sync::Arc;

mod process;
mod video;
pub use process::Cancel;
pub use video::{Decoder, Encoder, VideoInfo, VideoSource, fingerprint, inspect};

pub fn content_hash(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    format!("{:x}", Sha256::digest(bytes))
}

pub const MAX_PIXELS: usize = 4_194_304;
pub const MAX_SOURCE_BYTES: usize = MAX_PIXELS * 3 + 4096;

/// Validated immutable tightly packed RGB8 pixels with sRGB transfer.
/// This is a decoded image, not an encoded file or a viewer-cache entry.
#[derive(Clone, Debug)]
pub struct RgbImage {
    dimensions: [u32; 2],
    rgb: Arc<[u8]>,
}

impl RgbImage {
    /// P6 fixtures/image sequences are one adapter, not the viewer cache format.
    pub fn decode_ppm(bytes: &[u8]) -> Result<Self, &'static str> {
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
        Ok(Self {
            dimensions: [width, height],
            rgb: rgb.into(),
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
        })
    }

    pub fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }

    /// Input transfer conversion. Output is opaque premultiplied linear sRGB.
    pub fn linear_pixels(&self) -> impl Iterator<Item = [f32; 4]> + '_ {
        self.rgb.chunks_exact(3).map(|rgb| {
            let linear = |v: u8| {
                let v = f32::from(v) / 255.0;
                if v <= 0.04045 {
                    v / 12.92
                } else {
                    ((v + 0.055) / 1.055).powf(2.4)
                }
            };
            [linear(rgb[0]), linear(rgb[1]), linear(rgb[2]), 1.0]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
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
