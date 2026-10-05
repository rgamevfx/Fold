//! Feature-neutral CPU reference and optional GPU execution of immutable inputs.
use std::io::{self, Write};

pub mod audio;
pub mod channels;
pub mod frame;
#[cfg(feature = "gpu")]
pub mod gpu;
mod graph;
pub mod operations;
pub mod region;
mod sampling;
pub mod scheduling;
pub mod vector;
mod vector_geometry;
pub mod view;
pub use graph::{Affine, ImageId, ImageOp, RenderGraph};

/// One full-frame operation; colors are premultiplied scene-linear RGBA.
/// Legacy entry points interpret RGB as linear-sRGB; the ACES entry point as ACEScg.
#[derive(Clone, Copy, Debug)]
pub struct SolidPlan {
    pub width: u32,
    pub height: u32,
    pub rgba: [f32; 4],
}

/// The interpretation is explicit; legacy frames are never relabelled ACEScg.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WorkingSpace {
    LegacyLinearSrgb,
    AcesCg,
}

/// Owned, ready, tightly packed, row-major CPU pixels: square pixels and
/// premultiplied RGBA32F in the declared scene-linear working space.
#[derive(Debug)]
pub struct Frame {
    width: u32,
    height: u32,
    pixels: Vec<[f32; 4]>,
    _storage: std::sync::Arc<fold_media::budget::Lease>,
    working_space: WorkingSpace,
    config_identity: Option<String>,
    timing: Option<frame::Timing>,
}
/// Output-transformed opaque RGBA8, tightly packed with square pixels.
/// Legacy `to_display` produces sRGB; `to_output` uses the explicit OCIO
/// processor. Consumers must retain that processor's identity/settings.
#[derive(Debug)]
pub struct DisplayFrame {
    width: u32,
    height: u32,
    rgba: Vec<u8>,
    _storage: std::sync::Arc<fold_media::budget::Lease>,
    transform_identity: String,
    timing: Option<frame::Timing>,
}

impl DisplayFrame {
    pub fn descriptor(&self) -> frame::Descriptor {
        frame::Descriptor {
            dimensions: self.dimensions(),
            pixel_aspect: [1, 1],
            planes: vec![frame::Plane {
                offset: 0,
                row_stride: self.width as u64 * 4,
                channels: 4,
                component: frame::Component::Unorm8,
            }],
            alpha: frame::Alpha::Opaque,
            color: frame::ColorEncoding::Output {
                processor_identity: self.transform_identity.clone(),
            },
            timing: self.timing,
            storage: frame::Storage::CpuOwned,
            readiness: frame::Readiness::Ready,
        }
    }
    pub fn transform_identity(&self) -> &str {
        &self.transform_identity
    }

    pub fn dimensions(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    pub fn rgba(&self) -> &[u8] {
        &self.rgba
    }
}

impl Frame {
    pub fn to_data_display(&self, range: view::Range) -> Result<DisplayFrame, String> {
        let [black, white] = range.values();
        let storage = fold_media::budget::reserve_output(self.pixels.len() as u64 * 4)?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(self.pixels.len() * 4)
            .map_err(|_| "Data display allocation failed")?;
        for pixel in &self.pixels {
            let value = (((pixel[0] - black) / (white - black)).clamp(0., 1.) * 255.).round() as u8;
            rgba.extend_from_slice(&[value, value, value, 255]);
        }
        Ok(DisplayFrame {
            width: self.width,
            height: self.height,
            rgba,
            _storage: storage,
            transform_identity: format!("fold.data-display.v1:{black:?}:{white:?}"),
            timing: self.timing,
        })
    }

    pub fn with_timing(
        mut self,
        timestamp: fold_foundation::Time,
        duration: fold_foundation::Time,
    ) -> Result<Self, String> {
        if duration <= fold_foundation::Time::ZERO {
            return Err("Frame duration must be positive".into());
        }
        self.timing = Some(frame::Timing {
            timestamp,
            duration,
        });
        Ok(self)
    }
    pub fn descriptor(&self) -> frame::Descriptor {
        frame::Descriptor {
            dimensions: self.dimensions(),
            pixel_aspect: [1, 1],
            planes: vec![frame::Plane {
                offset: 0,
                row_stride: self.width as u64 * 16,
                channels: 4,
                component: frame::Component::Float32,
            }],
            alpha: frame::Alpha::Premultiplied,
            color: frame::ColorEncoding::Scene {
                working: self.working_space,
                config_identity: self.config_identity.clone(),
            },
            timing: self.timing,
            storage: frame::Storage::CpuOwned,
            readiness: frame::Readiness::Ready,
        }
    }
    pub fn working_space(&self) -> WorkingSpace {
        self.working_space
    }

    pub fn config_identity(&self) -> Option<&str> {
        self.config_identity.as_deref()
    }

    pub fn dimensions(&self) -> [u32; 2] {
        [self.width, self.height]
    }

    /// Explicit OCIO output boundary shared by preview and delivery. The
    /// processor must originate from this frame's declared working space.
    /// Only here are signed/HDR scene values quantized/clamped to RGBA8.
    /// This allocates a float transform buffer and encoded output; callers own
    /// those allocations separately from the evaluator's live-pixel budget.
    pub fn to_output(&self, processor: &fold_color::Processor) -> Result<DisplayFrame, String> {
        let source = match self.working_space {
            WorkingSpace::LegacyLinearSrgb => fold_color::LINEAR_SRGB,
            WorkingSpace::AcesCg => fold_color::WORKING_SPACE,
        };
        if processor.source() != source {
            return Err("output processor source does not match frame working space".into());
        }
        if self
            .config_identity
            .as_deref()
            .is_some_and(|identity| identity != processor.config_identity())
        {
            return Err("output processor config does not match scene config".into());
        }
        if self.pixels.iter().any(|p| p[3] != 1.0) {
            return Err("output requires an explicit opaque matte".into());
        }
        let _transform_storage =
            fold_media::budget::reserve_working(self.pixels.len() as u64 * 16)?;
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(self.pixels.len())
            .map_err(|_| "output transform allocation failed")?;
        pixels.extend_from_slice(&self.pixels);
        processor.apply(&mut pixels)?;
        let storage = fold_media::budget::reserve_output(self.pixels.len() as u64 * 4)?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(pixels.len() * 4)
            .map_err(|_| "output allocation failed")?;
        for pixel in pixels {
            rgba.extend_from_slice(&[
                (pixel[0] * 255.).round() as u8,
                (pixel[1] * 255.).round() as u8,
                (pixel[2] * 255.).round() as u8,
                255,
            ]);
        }
        Ok(DisplayFrame {
            _storage: storage,
            width: self.width,
            height: self.height,
            rgba,
            transform_identity: processor.identity().into(),
            timing: self.timing,
        })
    }

    /// Explicit opaque-black delivery matte, consuming this owned frame without
    /// allocating another full-size image. Premultiplied RGB is unchanged over black.
    pub fn over_black(mut self) -> Self {
        for pixel in &mut self.pixels {
            pixel[3] = 1.0;
        }
        self
    }

    /// CPU presentation fallback. Reject alpha rather than silently flattening it.
    pub fn to_display(&self) -> Result<DisplayFrame, &'static str> {
        if self.working_space != WorkingSpace::LegacyLinearSrgb {
            return Err("ACEScg requires an explicit OCIO output transform");
        }
        if self.pixels.iter().any(|p| p[3] != 1.0) {
            return Err("display output requires opaque pixels");
        }
        let storage = fold_media::budget::reserve_output(self.pixels.len() as u64 * 4)?;
        let mut rgba = Vec::new();
        rgba.try_reserve_exact(self.pixels.len() * 4)
            .map_err(|_| "display allocation failed")?;
        for pixel in &self.pixels {
            rgba.extend_from_slice(&[encode(pixel[0]), encode(pixel[1]), encode(pixel[2]), 255]);
        }
        Ok(DisplayFrame {
            _storage: storage,
            width: self.width,
            height: self.height,
            rgba,
            transform_identity: "fold.legacy-linear-srgb-to-srgb.v1".into(),
            timing: self.timing,
        })
    }

    pub fn pixels(&self) -> &[[f32; 4]] {
        &self.pixels
    }

    /// Explicit linear-to-sRGB transform, RGB8 PPM. Alpha is never silently lost.
    pub fn write_ppm(&self, mut output: impl Write) -> io::Result<()> {
        if self.working_space != WorkingSpace::LegacyLinearSrgb {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "ACEScg requires an explicit OCIO output transform",
            ));
        }
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

/// Exact RGB8 quantization of the reference transfer, without per-pixel powf.
/// Thresholds are the first representable nonnegative f32 that rounds to each
/// code. Build once using the original transfer, rather than an approximate LUT.
fn encode(linear: f32) -> u8 {
    static THRESHOLDS: std::sync::OnceLock<[f32; 255]> = std::sync::OnceLock::new();
    let thresholds = THRESHOLDS.get_or_init(|| {
        std::array::from_fn(|index| {
            let target = (index + 1) as u8;
            let (mut low, mut high) = (0u32, 1.0f32.to_bits());
            while low < high {
                let mid = low + (high - low) / 2;
                if encode_reference(f32::from_bits(mid)) < target {
                    low = mid + 1;
                } else {
                    high = mid;
                }
            }
            f32::from_bits(low)
        })
    });
    thresholds.partition_point(|&threshold| linear >= threshold) as u8
}

fn encode_reference(linear: f32) -> u8 {
    let srgb = if linear <= 0.0031308 {
        12.92 * linear
    } else {
        1.055 * linear.powf(1.0 / 2.4) - 0.055
    };
    (srgb * 255.0).round() as u8
}

/// Executes the shared image graph. SolidPlan is a one-node convenience adapter.
pub fn render(plan: impl Into<RenderGraph>) -> Result<Frame, String> {
    graph::evaluate(
        plan.into(),
        &mut fold_media::Decoder::default(),
        &fold_media::Cancel::default(),
        64 * 1024 * 1024,
        None,
    )
}

/// Worker entry point with retained source handles and cooperative cancellation.
/// 128 MiB working pixels accommodates two 1080p layers plus opacity/merge.
/// Caller-retained outputs, source bytes and display copies have separate budgets.
pub fn render_with(
    plan: RenderGraph,
    decoder: &mut fold_media::Decoder,
    cancel: &fold_media::Cancel,
) -> Result<Frame, String> {
    graph::evaluate(plan, decoder, cancel, 128 * 1024 * 1024, None)
}

/// Explicit ACEScg CPU path. Authored colors are already ACEScg. Video arrives
/// as reconstructed float BT.709 RGB with its signal transfer intact; OCIO applies
/// the input assignment once. RGB8 image fixtures default to sRGB input.
/// Decoder replacement/native GPU-plane interop remain phase 18.
pub fn render_aces_with(
    plan: RenderGraph,
    config: &fold_color::Config,
    decoder: &mut fold_media::Decoder,
    cancel: &fold_media::Cancel,
) -> Result<Frame, String> {
    graph::evaluate(plan, decoder, cancel, 128 * 1024 * 1024, Some(config))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn cpu_frame_and_display_leases_follow_caller_ownership() {
        let frame = render(SolidPlan {
            width: 2,
            height: 2,
            rgba: [0., 0., 0., 1.],
        })
        .unwrap();
        let working = std::sync::Arc::downgrade(&frame._storage);
        let display = frame.to_display().unwrap();
        let output = std::sync::Arc::downgrade(&display._storage);
        drop(frame);
        assert!(working.upgrade().is_none());
        assert!(output.upgrade().is_some());
        drop(display);
        assert!(output.upgrade().is_none());
    }
    #[test]
    fn fast_transfer_matches_reference_including_quantization_boundaries() {
        let mut bits = 1u32;
        for _ in 0..100_000 {
            bits = bits.wrapping_mul(1664525).wrapping_add(1013904223);
            let value = f32::from_bits(bits);
            assert_eq!(encode(value), encode_reference(value), "{value:?}");
        }
        for code in 1..=255u8 {
            let (mut low, mut high) = (0u32, 1.0f32.to_bits());
            while low < high {
                let mid = low + (high - low) / 2;
                if encode_reference(f32::from_bits(mid)) < code {
                    low = mid + 1;
                } else {
                    high = mid;
                }
            }
            for bits in [low - 1, low, low + 1] {
                let value = f32::from_bits(bits);
                assert_eq!(encode(value), encode_reference(value));
            }
        }
    }
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
