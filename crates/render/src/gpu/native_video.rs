//! GPU-resident NVDEC ingress. CUDA writes fresh engine-owned allocations;
//! immutable inputs are cached boundedly, independent of scarce decoder surfaces.
use super::{Host, host::Reservation, yuv::Layout};
use fold_foundation::Time;
use fold_media::{
    Cancel, VideoSource,
    native::{NativeDecoder, Nv12Layout},
};
use std::{collections::VecDeque, sync::Arc};

pub(super) struct NativeInput {
    pub buffer: wgpu::Buffer,
    pub layout: Layout,
    pub reservation: Arc<Reservation>,
    bytes: u64,
}
struct Entry {
    fingerprint: String,
    info: fold_media::VideoInfo,
    frame: u32,
    host: u64,
    input: Arc<NativeInput>,
}
pub(super) struct NativeVideo {
    decoder: NativeDecoder,
    cache: VecDeque<Entry>,
    bytes: u64,
}
impl NativeVideo {
    pub fn new() -> Result<Self, String> {
        Ok(Self {
            decoder: NativeDecoder::new()?,
            cache: VecDeque::new(),
            bytes: 0,
        })
    }
    pub fn statistics(&self) -> fold_media::native::NativeStatistics {
        self.decoder.statistics()
    }
    pub fn clear(&mut self) {
        self.cache.clear();
        self.bytes = 0;
    }
    /// No pixel upload/readback. Readiness is a supervised host acknowledgment,
    /// not an asynchronous external semaphore. Call from the render worker.
    pub fn decode(
        &mut self,
        host: &Host,
        source: &VideoSource,
        time: Time,
        cancel: &Cancel,
    ) -> Result<Arc<NativeInput>, String> {
        cancel.check()?;
        // Pump deferred loss notifications before any native allocation/launch.
        host.poll()?;
        let frame = source.info.frame_at(time)?;
        if let Some(i) = self.cache.iter().position(|e| {
            e.fingerprint == source.fingerprint
                && e.info == source.info
                && e.frame == frame
                && e.host == host.id()
        }) {
            let entry = self.cache.remove(i).unwrap();
            let input = entry.input.clone();
            self.cache.push_back(entry);
            return Ok(input);
        }
        let layout = Nv12Layout::for_source(source)?;
        let limit = (64 * 1024 * 1024).min(host.memory().budget / 8);
        while !self.cache.is_empty()
            && (self.bytes + layout.bytes + 65536 > limit || self.cache.len() >= 64)
        {
            let old = self.cache.pop_front().unwrap();
            self.bytes -= old.input.bytes;
        }
        let (mut pending, reservation) =
            fold_native_video::PendingBuffer::new_reserved(host.device(), layout.bytes, |bytes| {
                match host.reserve_as(bytes, super::host::AllocationKind::Decoded) {
                    Ok(reservation) => Ok(reservation),
                    Err(_) => {
                        self.clear();
                        host.reserve_as(bytes, super::host::AllocationKind::Decoded)
                    }
                }
            })?;
        let bytes = pending.allocation_bytes();
        let complete = self
            .decoder
            .decode_into(source, time, pending.write_ticket()?, cancel)?;
        host.poll()?;
        // Submission internally retains BOTH the raw-barrier buffer and its
        // reservation, even if the cache/caller immediately drops this input.
        let ready = pending.submit(host.queue(), complete, reservation.clone())?;
        let input = Arc::new(NativeInput {
            buffer: ready.buffer,
            layout: Layout {
                dimensions: layout.dimensions,
                luma_stride: layout.stride,
                chroma_stride: layout.stride,
                chroma_offsets: [layout.uv_offset as u32, layout.uv_offset as u32 + 1],
                chroma_step: 2,
            },
            reservation,
            bytes,
        });
        while !self.cache.is_empty() && self.bytes + bytes > limit {
            let old = self.cache.pop_front().unwrap();
            self.bytes -= old.input.bytes;
        }
        if bytes <= limit {
            self.bytes += bytes;
            self.cache.push_back(Entry {
                fingerprint: source.fingerprint.clone(),
                info: source.info.clone(),
                frame,
                host: host.id(),
                input: input.clone(),
            });
        }
        Ok(input)
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> VideoSource {
        fold_media::inspect(
            std::path::Path::new(&std::env::var("FOLD_NATIVE_FIXTURE").unwrap()),
            &Cancel::default(),
        )
        .unwrap()
    }
    #[test]
    #[ignore = "requires NVIDIA GPU and native helper"]
    fn acquire_retains_cache_ineligible_and_cleared_inputs_until_completion() {
        let source = fixture();
        let layout = Nv12Layout::for_source(&source).unwrap();
        let (probe, _) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
        let pending = fold_native_video::PendingBuffer::new(probe.device(), layout.bytes).unwrap();
        let bytes = pending.allocation_bytes();
        drop(pending);
        drop(probe);
        for budget in [bytes - 1, bytes * 2, bytes * 16] {
            let (host, _) = pollster::block_on(Host::headless(budget)).unwrap();
            let mut native = NativeVideo::new().unwrap();
            let result = native.decode(
                &host,
                &source,
                source.info.time(0).unwrap(),
                &Cancel::default(),
            );
            if budget < bytes {
                assert!(result.is_err());
                assert_eq!(host.memory().allocated, 0);
                continue;
            }
            let input = result.unwrap();
            assert_eq!(native.cache.is_empty(), budget == bytes * 2);
            native.clear();
            drop(input); // caller aborts without ever encoding YUV
            assert_eq!(
                host.memory().allocated,
                bytes,
                "acquire submission must retain reservation"
            );
            host.device()
                .poll(wgpu::PollType::wait_indefinitely())
                .unwrap();
            host.check().unwrap();
            assert_eq!(host.memory().allocated, 0);
        }
    }
    #[test]
    #[ignore = "requires native helper; retained inputs must not recount helper work"]
    fn gpu_cache_hit_does_not_recount_native_telemetry() {
        let source = fixture();
        let (host, _) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
        let mut native = NativeVideo::new().unwrap();
        let first = native
            .decode(
                &host,
                &source,
                source.info.time(0).unwrap(),
                &Cancel::default(),
            )
            .unwrap();
        let before = native.statistics();
        assert_eq!(before.requests, 1);
        assert!(before.codec_call_nanoseconds > 0);
        assert!(before.copy_ready_nanoseconds > 0);
        assert_eq!(
            before.gpu_local_copy_bytes,
            Nv12Layout::for_source(&source).unwrap().bytes
        );
        let cached = native
            .decode(
                &host,
                &source,
                source.info.time(0).unwrap(),
                &Cancel::default(),
            )
            .unwrap();
        assert!(Arc::ptr_eq(&first, &cached));
        let after = native.statistics();
        assert_eq!(after.requests, before.requests);
        assert_eq!(after.codec_call_nanoseconds, before.codec_call_nanoseconds);
        assert_eq!(after.copy_ready_nanoseconds, before.copy_ready_nanoseconds);
        assert_eq!(after.gpu_local_copy_bytes, before.gpu_local_copy_bytes);
        native.clear();
        drop(first);
        drop(cached);
        host.device()
            .poll(wgpu::PollType::wait_indefinitely())
            .unwrap();
        assert_eq!(host.memory().allocated, 0);
    }
    #[test]
    #[ignore = "requires Vulkan; explicitly destroys parent device, not driver fault injection"]
    fn destroyed_parent_device_is_rejected_before_native_launch() {
        let source = fixture();
        let (host, _) = pollster::block_on(Host::headless(64 * 1024 * 1024)).unwrap();
        let mut native = NativeVideo::new().unwrap();
        host.device().destroy();
        let error = native
            .decode(
                &host,
                &source,
                source.info.time(0).unwrap(),
                &Cancel::default(),
            )
            .err()
            .expect("destroyed host rejected");
        assert!(
            error.contains("lost") || error.contains("stopped"),
            "{error}"
        );
        assert_eq!(native.decoder.statistics().launches, 0);
        assert_eq!(host.memory().allocated, 0);
    }
}
