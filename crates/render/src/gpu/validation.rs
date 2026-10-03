//! Four-byte asynchronous status readback. No working-image pixels leave the
//! GPU. Every pass flags invalid intermediate values before later crop/mask
//! operations can hide them; publication waits for this status, not submission.
use super::{Host, host::Reservation};
use std::sync::{Arc, Mutex};

type ResultState = Arc<Mutex<Option<Result<Option<f64>, String>>>>;
pub(super) struct Status {
    pub gpu: wgpu::Buffer,
    readback: wgpu::Buffer,
    reservation: Arc<Reservation>,
    timestamps: Option<(wgpu::QuerySet, wgpu::Buffer, f32)>,
}
#[derive(Clone)]
pub(super) struct Completion(ResultState);
impl Completion {
    pub fn gpu_nanoseconds(&self) -> Result<Option<f64>, String> {
        match self.0.lock().unwrap().as_ref() {
            Some(Ok(time)) => Ok(*time),
            Some(Err(e)) => Err(e.clone()),
            None => Ok(None),
        }
    }
    pub fn ready(&self) -> Result<bool, String> {
        match self.0.lock().unwrap().as_ref() {
            None => Ok(false),
            Some(Ok(_)) => Ok(true),
            Some(Err(error)) => Err(error.clone()),
        }
    }
}
impl Status {
    pub fn new(host: &Host) -> Result<Self, String> {
        let timed = host
            .device()
            .features()
            .contains(wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS);
        let reservation = host.reserve(if timed { 44 } else { 8 })?;
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
            usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
            mapped_at_creation: false,
        });
        let readback = host.device().create_buffer(&wgpu::BufferDescriptor {
            label: Some("Fold validation status"),
            size: if timed { 24 } else { 4 },
            usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        Ok(Self {
            gpu,
            readback,
            reservation,
            timestamps,
        })
    }
    pub fn start(&self, encoder: &mut wgpu::CommandEncoder) {
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
        let result: ResultState = Arc::new(Mutex::new(None));
        let output = result.clone();
        let buffer = self.readback.clone();
        self.readback
            .slice(..)
            .map_async(wgpu::MapMode::Read, move |mapped| {
                let status = mapped.map_err(|e| e.to_string()).and_then(|()| {
                    let bytes = buffer.slice(..).get_mapped_range();
                    let valid = bytes[..4] == [0; 4];
                    let time = self.timestamps.as_ref().map(|(_, _, period)| {
                        let start = u64::from_le_bytes(bytes[8..16].try_into().unwrap());
                        let end = u64::from_le_bytes(bytes[16..24].try_into().unwrap());
                        end.wrapping_sub(start) as f64 * f64::from(*period)
                    });
                    drop(bytes);
                    buffer.unmap();
                    if valid {
                        Ok(time)
                    } else {
                        Err("GPU intermediate contains nonfinite RGB or invalid alpha".into())
                    }
                });
                drop(self.reservation);
                *output.lock().unwrap() = Some(status);
            });
        Completion(result)
    }
}
