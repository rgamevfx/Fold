//! Feature-neutral CPU execution. Creative packages compile immutable inputs here.
use std::io::{self, Write};

/// One full-frame operation; colors are scene-linear sRGB, premultiplied RGBA.
#[derive(Clone, Copy, Debug)]
pub struct SolidPlan {
    pub width: u32,
    pub height: u32,
    pub rgba: [f32; 4],
}

/// Owned, tightly packed, row-major CPU pixels. Metadata is fixed for this path:
/// square pixels, scene-linear sRGB primaries, premultiplied RGBA32F.
#[derive(Debug)]
pub struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<[f32; 4]>,
}
/// Display-ready opaque SDR sRGB RGBA8, tightly packed with square pixels.
/// Constructed only by the engine output transform; dimensions and bytes agree.
#[derive(Debug)]
pub struct DisplayFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
}

impl DisplayFrame {
    pub fn dimensions(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

impl Frame {
    /// CPU presentation fallback. Reject alpha rather than silently flattening it.
    pub fn to_display(&self) -> Result<DisplayFrame, &'static str> {
        if self.pixels.iter().any(|p| p[3] != 1.0) {
            return Err("display output requires opaque pixels");
        }
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(self.pixels.len() * 4)
            .map_err(|_| "display allocation failed")?;
        for pixel in &self.pixels {
            rgba.extend_from_slice(&[encode(pixel[0]), encode(pixel[1]), encode(pixel[2]), 255]);
        }
        Ok(DisplayFrame {
            width: self.width,
            height: self.height,
            rgba,
        })
    }

    pub fn pixels(&self) -> &[[f32; 4]] {
        &self.pixels
    }

    /// Explicit linear-to-sRGB transform, RGB8 PPM. Alpha is never silently lost.
    pub fn write_ppm(&self, mut output: impl Write) -> io::Result<()> {
        if self.pixels.iter().any(|p| p[3] != 1.0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "PPM requires opaque pixels",
            ));
        }
        write!(output, "P6\n{} {}\n255\n", self.width, self.height)?;
        for pixel in &self.pixels {
            let rgb = [encode(pixel[0]), encode(pixel[1]), encode(pixel[2])];
            output.write_all(&rgb)?;
        }
        Ok(())
    }
}

fn encode(linear: f32) -> u8 {
    let srgb = if linear <= 0.0031308 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}

pub fn render(plan: SolidPlan) -> Result<Frame, &'static str> {
    // Fixed 64 MiB pixel budget for this initial CPU path.
    let count = u64::from(plan.width) * u64::from(plan.height);
    if count == 0 || count > 4_194_304 {
        return Err("resolution must contain 1..=4194304 pixels");
    }
    let [r, g, b, a] = plan.rgba;
    if !plan
        .rgba
        .iter()
        .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
        || r > a
        || g > a
        || b > a
    {
        return Err("expected finite SDR premultiplied linear RGBA");
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count as usize)
        .map_err(|_| "frame allocation failed")?;
    pixels.resize(count as usize, plan.rgba);
    Ok(Frame {
        width: plan.width,
        height: plan.height,
        pixels,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn display_matches_headless_output_without_mutating_scene_pixels() {
        let frame = render(SolidPlan {
            width: 2,
            height: 1,
            rgba: [0.0, 0.5, 1.0, 1.0],
        })
        .unwrap();
        let display = frame.to_display().unwrap();
        assert_eq!(display.dimensions(), [2, 1]);
        assert_eq!(display.rgba(), &[0, 188, 255, 255, 0, 188, 255, 255]);
        let mut ppm = Vec::new();
        frame.write_ppm(&mut ppm).unwrap();
        let rgb: Vec<_> = display
            .rgba()
            .chunks_exact(4)
            .flat_map(|p| p[..3].iter().copied())
            .collect();
        assert_eq!(&ppm[b"P6\n2 1\n255\n".len()..], rgb);
        assert_eq!(frame.pixels(), &[[0.0, 0.5, 1.0, 1.0]; 2]);
        assert!(
            render(SolidPlan {
                width: 1,
                height: 1,
                rgba: [0.0; 4]
            })
            .unwrap()
            .to_display()
            .is_err()
        );
    }

    #[test]
    fn output_transform_and_limits() {
        let plan = SolidPlan {
            width: 1,
            height: 1,
            rgba: [0.0, 0.5, 1.0, 1.0],
        };
        let mut bytes = Vec::new();
        render(plan).unwrap().write_ppm(&mut bytes).unwrap();
        assert_eq!(bytes, b"P6\n1 1\n255\n\x00\xbc\xff");
        for (width, height) in [(0, 1), (u32::MAX, u32::MAX)] {
            assert!(
                render(SolidPlan {
                    width,
                    height,
                    ..plan
                })
                .is_err()
            );
        }
        for rgba in [[f32::NAN, 0.0, 0.0, 1.0], [1.0, 0.0, 0.0, 0.5]] {
            assert!(render(SolidPlan { rgba, ..plan }).is_err());
        }
        let transparent = render(SolidPlan {
            rgba: [0.0; 4],
            ..plan
        })
        .unwrap();
        assert!(transparent.write_ppm(Vec::new()).is_err());
    }
}
