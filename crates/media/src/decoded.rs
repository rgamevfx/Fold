//! Immutable native decoded samples. Color reconstruction is a consumer operation,
//! not a requirement imposed by the decoder's transport representation.
use fold_foundation::Time;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT_FRAME: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaneLayout {
    pub offset: usize,
    pub stride: u32,
    pub dimensions: [u32; 2],
}

/// Ready CPU-plane lease. Cloning retains the allocation; constructing this
/// lease moves the pipe buffer without a second full-frame allocation/copy.
/// This is explicitly CPU-resident, including the CUDA-download adapter.
#[derive(Clone, Debug)]
pub struct YuvFrame {
    identity: u64,
    dimensions: [u32; 2],
    planes: [PlaneLayout; 4],
    format: crate::VideoFormat,
    data: Arc<Vec<u8>>,
    timestamp: Time,
    duration: Time,
    _storage: Arc<crate::budget::Lease>,
}
impl YuvFrame {
    pub fn from_420(
        dimensions: [u32; 2],
        data: Vec<u8>,
        timestamp: Time,
        duration: Time,
    ) -> Result<Self, fold_foundation::Failure> {
        Self::from_planes(
            crate::VideoFormat::default(),
            dimensions,
            data,
            timestamp,
            duration,
        )
    }
    pub fn from_planes(
        format: crate::VideoFormat,
        dimensions: [u32; 2],
        data: Vec<u8>,
        timestamp: Time,
        duration: Time,
    ) -> Result<Self, fold_foundation::Failure> {
        format.validate()?;
        let [w, h] = dimensions;
        let pixels = u64::from(w) * u64::from(h);
        if pixels == 0 || pixels > crate::MAX_PIXELS as u64 || duration <= Time::ZERO {
            return Err("invalid native frame dimensions or duration".into());
        }
        let [cw, ch] = [
            w.div_ceil(format.subsampling[0]),
            h.div_ceil(format.subsampling[1]),
        ];
        let step = format.sample_bytes();
        let y = pixels as usize * step as usize;
        let c = cw as usize * ch as usize * step as usize;
        if data.len() as u64 != format.bytes(dimensions) {
            return Err("invalid YUV plane storage".into());
        }
        let storage = crate::budget::DECODED.reserve(data.capacity() as u64)?;
        Ok(Self {
            _storage: storage,
            identity: NEXT_FRAME.fetch_add(1, Ordering::Relaxed),
            dimensions,
            format,
            planes: [
                PlaneLayout {
                    offset: 0,
                    stride: w * step,
                    dimensions,
                },
                PlaneLayout {
                    offset: y,
                    stride: cw * step,
                    dimensions: [cw, ch],
                },
                PlaneLayout {
                    offset: y + c,
                    stride: cw * step,
                    dimensions: [cw, ch],
                },
                PlaneLayout {
                    offset: y + 2 * c,
                    stride: w * step,
                    dimensions,
                },
            ],
            data: Arc::new(data),
            timestamp,
            duration,
        })
    }
    /// CPU correctness adapter for the native-plane reconstruction operation.
    /// Integer center-nearest mapping is deliberate: swscale's float-RGB resize
    /// can quantize through integer RGB and perturb exact center ties.
    pub fn reconstruct_signal(
        &self,
        dimensions: [u32; 2],
        cancel: &crate::Cancel,
    ) -> Result<crate::RgbImage, fold_foundation::Failure> {
        cancel.check()?;
        let count = u64::from(dimensions[0]) * u64::from(dimensions[1]);
        if count == 0 || count > crate::MAX_PIXELS as u64 {
            return Err("invalid reconstruction dimensions".into());
        }
        let channels = if self.format.alpha { 4 } else { 3 };
        let storage = crate::budget::DECODED.reserve(count * channels * 4)?;
        let mut planes = Vec::new();
        planes
            .try_reserve_exact(count as usize * channels as usize)
            .map_err(|_| "CPU reconstruction allocation failed")?;
        planes.resize(count as usize * channels as usize, 0.);
        let nearest = |p: u32, from: u32, to: u32| {
            ((2 * u64::from(p) + 1) * u64::from(from) / (2 * u64::from(to))) as u32
        };
        let native = self.dimensions;
        let domain = if dimensions.iter().any(|v| !v.is_multiple_of(2)) {
            native
        } else {
            dimensions
        };
        let sub = self.format.subsampling;
        let chroma_size = [domain[0].div_ceil(sub[0]), domain[1].div_ceil(sub[1])];
        let step = self.format.sample_bytes();
        let value = |offset: usize| -> f32 {
            if step == 1 {
                f32::from(self.data[offset])
            } else {
                f32::from(u16::from_le_bytes([
                    self.data[offset],
                    self.data[offset + 1],
                ]))
            }
        };
        let scale = (1u32 << (self.format.depth - 8)) as f32;
        let max = ((1u32 << self.format.depth) - 1) as f32;
        let sample = |plane: usize, x: i32, y: i32| {
            let layout = self.planes[plane];
            let x = nearest(
                x.clamp(0, chroma_size[0] as i32 - 1) as u32,
                layout.dimensions[0],
                chroma_size[0],
            );
            let y = nearest(
                y.clamp(0, chroma_size[1] as i32 - 1) as u32,
                layout.dimensions[1],
                chroma_size[1],
            );
            value(layout.offset + (y * layout.stride + x * step) as usize)
        };
        for y in 0..dimensions[1] {
            for x in 0..dimensions[0] {
                if x % 4096 == 0 {
                    cancel.check()?;
                }
                let px = nearest(x, domain[0], dimensions[0]);
                let py = nearest(y, domain[1], dimensions[1]);
                let sx = nearest(px, native[0], domain[0]);
                let sy = nearest(py, native[1], domain[1]);
                if self.format.rgb {
                    let i = (y * dimensions[0] + x) as usize;
                    for plane in 0..3 {
                        planes[plane * count as usize + i] = value(
                            self.planes[plane].offset + ((sy * native[0] + sx) * step) as usize,
                        ) / max;
                    }
                    if self.format.alpha {
                        planes[3 * count as usize + i] =
                            value(self.planes[3].offset + ((sy * native[0] + sx) * step) as usize)
                                / max;
                    }
                    continue;
                }
                let raw = value(((sy * native[0] + sx) * step) as usize);
                let luma = if self.format.full_range {
                    raw / max
                } else {
                    (raw - 16. * scale) / (219. * scale)
                };
                let cx = (px as f32 - self.format.chroma_origin[0] as f32 * 0.5) / sub[0] as f32;
                let cy = (py as f32 - self.format.chroma_origin[1] as f32 * 0.5) / sub[1] as f32;
                let ix = cx.floor() as i32;
                let iy = cy.floor() as i32;
                let fx = cx - cx.floor();
                let fy = cy - cy.floor();
                let chroma = |plane| {
                    let top = sample(plane, ix, iy) * (1. - fx) + sample(plane, ix + 1, iy) * fx;
                    let bottom =
                        sample(plane, ix, iy + 1) * (1. - fx) + sample(plane, ix + 1, iy + 1) * fx;
                    (top * (1. - fy) + bottom * fy - 128. * scale)
                        / if self.format.full_range {
                            max
                        } else {
                            224. * scale
                        }
                };
                let u = chroma(1);
                let v = chroma(2);
                let i = (y * dimensions[0] + x) as usize;
                let n = count as usize;
                planes[i] = luma - 0.187_324_27 * u - 0.468_124_27 * v;
                planes[n + i] = luma + 1.8556 * u;
                planes[2 * n + i] = luma + 1.5748 * v;
                if self.format.alpha {
                    planes[3 * n + i] =
                        (value(self.planes[3].offset + ((sy * native[0] + sx) * step) as usize)
                            / max)
                            .clamp(0., 1.);
                }
            }
        }
        Ok(crate::RgbImage {
            dimensions,
            rgb: Arc::new(Vec::new()),
            identity: Default::default(),
            signal: Some(Arc::new(planes)),
            srgb_signal: false,
            _storage: storage,
        })
    }
    /// Process-local immutable allocation identity, shared by clones. Not a
    /// persistent content key and never serialized into a project.
    pub fn identity(&self) -> u64 {
        self.identity
    }
    pub fn dimensions(&self) -> [u32; 2] {
        self.dimensions
    }
    pub fn timestamp(&self) -> Time {
        self.timestamp
    }
    pub fn duration(&self) -> Time {
        self.duration
    }
    pub fn format(&self) -> crate::VideoFormat {
        self.format
    }
    pub fn planes(&self) -> &[PlaneLayout; 4] {
        &self.planes
    }
    pub fn bytes(&self) -> &[u8] {
        &self.data
    }
    pub fn storage_bytes(&self) -> u64 {
        self.data.capacity() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn native_planes_move_storage_and_describe_odd_edges() {
        let bytes = vec![128; 17]; // 3x3 luma, two 2x2 chroma planes.
        let original = bytes.as_ptr();
        let frame =
            YuvFrame::from_420([3, 3], bytes, Time::ZERO, Time::new(1, 30).unwrap()).unwrap();
        assert_eq!(frame.bytes().as_ptr(), original);
        assert_eq!(frame.clone().bytes().as_ptr(), original);
        assert_eq!(frame.planes()[2].offset, 13);
        assert_eq!(frame.planes()[1].dimensions, [2, 2]);
        assert!(
            frame
                .reconstruct_signal([0, 3], &crate::Cancel::default())
                .is_err()
        );
        let cancel = crate::Cancel::default();
        cancel.cancel();
        assert!(
            frame
                .reconstruct_signal([3, 3], &cancel)
                .unwrap_err()
                .message
                .contains("cancel")
        );
        let mut spare = Vec::with_capacity(32);
        spare.resize(17, 128);
        let retained = YuvFrame::from_420([3, 3], spare, Time::ZERO, frame.duration).unwrap();
        assert_eq!(
            retained.storage_bytes(),
            32,
            "account for retained capacity, not just visible bytes"
        );
        assert!(
            YuvFrame::from_420([3, 3], vec![0; 16], Time::ZERO, Time::new(1, 30).unwrap()).is_err()
        );
    }
}
