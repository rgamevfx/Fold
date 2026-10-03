//! Immutable native decoded samples. Color reconstruction is a consumer operation,
//! not a requirement imposed by the decoder's transport representation.
use fold_foundation::Time;
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
static NEXT_FRAME: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum YuvEncoding {
    /// The currently verified media profile: 8-bit, limited-range BT.709,
    /// progressive 4:2:0, horizontal left / vertical center chroma siting.
    Bt709Limited420Left,
}

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
    planes: [PlaneLayout; 3],
    data: Arc<Vec<u8>>,
    timestamp: Time,
    duration: Time,
}
impl YuvFrame {
    pub fn from_420(
        dimensions: [u32; 2],
        data: Vec<u8>,
        timestamp: Time,
        duration: Time,
    ) -> Result<Self, String> {
        let [w, h] = dimensions;
        let pixels = u64::from(w) * u64::from(h);
        if pixels == 0 || pixels > crate::MAX_PIXELS as u64 || duration <= Time::ZERO {
            return Err("invalid native frame dimensions or duration".into());
        }
        let [cw, ch] = [w.div_ceil(2), h.div_ceil(2)];
        let y = pixels as usize;
        let c = cw as usize * ch as usize;
        if data.len() != y + 2 * c {
            return Err("invalid YUV plane storage".into());
        }
        Ok(Self {
            identity: NEXT_FRAME.fetch_add(1, Ordering::Relaxed),
            dimensions,
            planes: [
                PlaneLayout {
                    offset: 0,
                    stride: w,
                    dimensions,
                },
                PlaneLayout {
                    offset: y,
                    stride: cw,
                    dimensions: [cw, ch],
                },
                PlaneLayout {
                    offset: y + c,
                    stride: cw,
                    dimensions: [cw, ch],
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
    ) -> Result<crate::RgbImage, String> {
        cancel.check()?;
        let count = u64::from(dimensions[0]) * u64::from(dimensions[1]);
        if count == 0 || count > crate::MAX_PIXELS as u64 {
            return Err("invalid reconstruction dimensions".into());
        }
        let mut planes = Vec::new();
        planes
            .try_reserve_exact(count as usize * 3)
            .map_err(|_| "CPU reconstruction allocation failed")?;
        planes.resize(count as usize * 3, 0.);
        let nearest = |p: u32, from: u32, to: u32| {
            ((2 * u64::from(p) + 1) * u64::from(from) / (2 * u64::from(to))) as u32
        };
        let native = self.dimensions;
        let domain = if dimensions.iter().any(|v| !v.is_multiple_of(2)) {
            native
        } else {
            dimensions
        };
        let chroma_size = domain.map(|v| v.div_ceil(2));
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
            f32::from(self.data[layout.offset + (y * layout.stride + x) as usize])
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
                let luma = (f32::from(self.data[(sy * native[0] + sx) as usize]) - 16.) / 219.;
                let cx = px as f32 / 2.;
                let cy = (py as f32 - 0.5) / 2.;
                let ix = cx.floor() as i32;
                let iy = cy.floor() as i32;
                let fx = cx - cx.floor();
                let fy = cy - cy.floor();
                let chroma = |plane| {
                    let top = sample(plane, ix, iy) * (1. - fx) + sample(plane, ix + 1, iy) * fx;
                    let bottom =
                        sample(plane, ix, iy + 1) * (1. - fx) + sample(plane, ix + 1, iy + 1) * fx;
                    (top * (1. - fy) + bottom * fy - 128.) / 224.
                };
                let u = chroma(1);
                let v = chroma(2);
                let i = (y * dimensions[0] + x) as usize;
                let n = count as usize;
                planes[i] = luma - 0.187_324_27 * u - 0.468_124_27 * v;
                planes[n + i] = luma + 1.8556 * u;
                planes[2 * n + i] = luma + 1.5748 * v;
            }
        }
        Ok(crate::RgbImage {
            dimensions,
            rgb: Arc::from([]),
            signal: Some(planes.into()),
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
    pub fn encoding(&self) -> YuvEncoding {
        YuvEncoding::Bt709Limited420Left
    }
    pub fn planes(&self) -> &[PlaneLayout; 3] {
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
