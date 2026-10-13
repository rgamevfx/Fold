//! Four-byte asynchronous status readback. No working-image pixels leave the
//! GPU. Every pass flags invalid intermediate values before later crop/mask
//! operations can hide them; publication waits for this status, not submission.
use super::{Host, host::Reservation};
use std::sync::{Arc, Mutex};

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};
    use wgpu::util::DeviceExt;

    fn complete(host: &Host, status: Status, invalid: bool) -> Completion {
        let mut encoder = host.device().create_command_encoder(&Default::default());
        status.start(&mut encoder);
        if invalid {
            let value = host
                .device()
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: None,
                    contents: bytemuck::bytes_of(&1u32),
                    usage: wgpu::BufferUsages::COPY_SRC,
                });
            encoder.copy_buffer_to_buffer(&value, 0, &status.gpu, 0, 4);
        }
        status.encode(&mut encoder);
        host.submit(encoder, vec![], vec![]);
        let ready = status.submitted();
        let deadline = Instant::now() + Duration::from_secs(10);
        while matches!(ready.ready(), Ok(false)) {
            host.poll().unwrap();
            assert!(Instant::now() < deadline);
        }
        assert_eq!(ready.ready().is_err(), invalid);
        ready
    }

    #[test]
    #[ignore = "requires native Vulkan"]
    fn completed_readbacks_reuse_storage_without_hiding_failure() {
        let (host, _) = pollster::block_on(Host::headless(1024 * 1024)).unwrap();
        let first = Status::new(&host).unwrap();
        let buffer = first.readback.clone();
        complete(&host, first, true);
        let second = Status::new(&host).unwrap();
        assert_eq!(second.readback, buffer, "completed readback must be reused");
        complete(&host, second, false);
        let third = Status::new(&host).unwrap();
        let pending = Status::new(&host).unwrap();
        assert_ne!(
            third.readback, pending.readback,
            "live status cannot be reused"
        );
        complete(&host, third, false);
        complete(&host, pending, true);
        assert_eq!(host.memory().allocated, host.memory().reusable);
    }
}

/// Only unmapped, completed buffers enter this bounded host-local pool.
#[derive(Default)]
pub(super) struct Readbacks {
    idle: Vec<(wgpu::Buffer, Arc<Reservation>)>,
}
impl Readbacks {
    pub fn bytes(&self) -> u64 {
        self.idle.iter().map(|(buffer, _)| buffer.size()).sum()
    }
    pub fn clear(&mut self) {
        self.idle.clear();
    }
}

type ResultState = Arc<Mutex<State>>;
enum State {
    Pending,
    Mapped(Status, Result<(), wgpu::BufferAsyncError>),
    Complete(Result<Option<f64>, fold_foundation::Failure>),
}
pub(super) struct Status {
    pub gpu: wgpu::Buffer,
    pub failure: &'static str,
    host_failure: Arc<Mutex<Option<String>>>,
    completion_poll: Arc<Mutex<()>>,
    readback: wgpu::Buffer,
    _reservation: Arc<Reservation>,
    readback_reservation: Arc<Reservation>,
    readbacks: Arc<Mutex<Readbacks>>,
    timestamps: Option<(wgpu::QuerySet, wgpu::Buffer, f32)>,
}
#[derive(Clone)]
pub(super) struct Completion(ResultState, Option<Arc<Mutex<()>>>);
impl Completion {
    pub(super) fn submitted(queue: &wgpu::Queue, image: Arc<super::host::Image>) -> Self {
        let state = Arc::new(Mutex::new(State::Pending));
        let done = state.clone();
        queue.on_submitted_work_done(move || {
            drop(image);
            *done.lock().unwrap() = State::Complete(Ok(None));
        });
        Self(state, None)
    }

    fn result(&self) -> Option<Result<Option<f64>, fold_foundation::Failure>> {
        // Serialize status decoding against other workers pumping loss/map
        // callbacks, so a partially delivered poll cannot publish readiness.
        let _poll = self.1.as_ref().map(|poll| poll.lock().unwrap());
        let mut state = self.0.lock().unwrap();
        if matches!(*state, State::Mapped(..)) {
            let State::Mapped(status, mapped) = std::mem::replace(&mut *state, State::Pending)
            else {
                unreachable!()
            };
            *state = State::Complete(status.read(mapped));
        }
        match &*state {
            State::Complete(result) => Some(result.clone()),
            _ => None,
        }
    }
    pub fn gpu_nanoseconds(&self) -> Result<Option<f64>, fold_foundation::Failure> {
        self.result().unwrap_or(Ok(None))
    }
    pub fn ready(&self) -> Result<bool, fold_foundation::Failure> {
        self.result()
            .map_or(Ok(false), |result| result.map(|_| true))
    }
}
impl Status {
    pub fn new(host: &Host) -> Result<Self, fold_foundation::Failure> {
        let timed = host
            .device()
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        let reservation = host.reserve(if timed { 20 } else { 4 })?;
        let readbacks = host.readbacks();
        let cached = readbacks.lock().unwrap().idle.pop();
        let (readback, readback_reservation) = match cached {
            Some(value) => value,
            None => {
                let size = if timed { 24 } else { 4 };
                let lease = host.reserve(size)?;
                let buffer = host.device().create_buffer(&wgpu::BufferDescriptor {
                    label: Some("Fold validation status"),
                    size,
                    usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                });
                (buffer, lease)
            }
        };
        let timestamps = timed.then(|| {
            (
                host.device().create_query_set(&wgpu::QuerySetDescriptor {
                    label: Some("Fold GPU time"),
                    ty: wgpu::QueryType::Timestamp,
                    count: 2,
                }),
                host.device().create_buffer(&wgpu::BufferDescriptor {
                    label: None,
                    size: 16,
                    usage: wgpu::BufferUsages::QUERY_RESOLVE | wgpu::BufferUsages::COPY_SRC,
                    mapped_at_creation: false,
                }),
                host.queue().get_timestamp_period(),
            )
        });
        // wgpu guarantees zero initialization, including when allocations recycle.
        let gpu = host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fold intermediate validation"),
            size: 4,
            usage: wgpu::BufferUsages::STORAGE
                | wgpu::BufferUsages::COPY_SRC
                | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            host_failure: host.failure_state(),
            completion_poll: host.completion_poll(),
            failure: "GPU intermediate contains nonfinite RGB or invalid alpha",
            gpu,
            readback,
            _reservation: reservation,
            readback_reservation,
            readbacks,
            timestamps,
        })
    }
    pub fn start(&self, encoder: &mut wgpu::CommandEncoder) {
        encoder.clear_buffer(&self.gpu, 0, None);
        if let Some((queries, _, _)) = &self.timestamps {
            encoder.write_timestamp(queries, 0);
        }
    }
    pub fn readback_bytes(&self) -> u64 {
        self.readback.size()
    }
    pub fn encode(&self, encoder: &mut wgpu::CommandEncoder) {
        if let Some((queries, buffer, _)) = &self.timestamps {
            encoder.write_timestamp(queries, 1);
            encoder.resolve_query_set(queries, 0..2, buffer, 0);
            encoder.copy_buffer_to_buffer(buffer, 0, &self.readback, 8, 16);
        }
        encoder.copy_buffer_to_buffer(&self.gpu, 0, &self.readback, 0, 4);
    }
    /// Call after submission. Callback retains the mapped buffer and reservation
    /// even if the consumer cancels/drops its frame before completion.
    pub fn submitted(self) -> Completion {
        let result: ResultState = Arc::new(Mutex::new(State::Pending));
        let output = result.clone();
        let poll = self.completion_poll.clone();
        self.readback
            .clone()
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |mapped| {
                // Device-loss notification can follow a successful map callback
                // in the same poll. Decode only after the caller's poll returns,
                // when the shared host failure state has been updated.
                *output.lock().unwrap() = State::Mapped(self, mapped);
            });
        Completion(result, Some(poll))
    }
    fn read(
        self,
        mapped: Result<(), wgpu::BufferAsyncError>,
    ) -> Result<Option<f64>, fold_foundation::Failure> {
        if let Some(error) = self.host_failure.lock().unwrap().as_ref() {
            return Err(error.clone().into());
        }
        mapped.map_err(|e| fold_foundation::Failure::from(e.to_string()))?;
        let bytes = self.readback.slice(..).get_mapped_range();
        let valid = bytes[..4] == [0; 4];
        let time = self.timestamps.as_ref().map(|(_, _, period)| {
            let start = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
            let end = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
            end.wrapping_sub(start) as f64 * f64::from(*period)
        });
        drop(bytes);
        self.readback.unmap();
        // Never recycle on map/device failure. Abandoned completions drop their
        // storage; their callback still owns it until mapping completes.
        let mut pool = self.readbacks.lock().unwrap();
        if pool.idle.len() < 8 {
            pool.idle.push((self.readback, self.readback_reservation));
        }
        if valid {
            Ok(time)
        } else {
            Err(self.failure.into())
        }
    }
}
