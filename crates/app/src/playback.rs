//! Desktop audio transport. One worker owns decode and the device; the callback
//! only consumes a bounded SPSC ring and publishes atomic clock/diagnostic data.
//! Each seek changes generation immediately, silences the old callback, then
//! drops the old stream, primes a fresh ring, and starts a fresh device stream.
#[path = "playback_output.rs"]
mod output;
use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use fold_foundation::Rounding;
use fold_media::{
    Cancel,
    audio::{AUDIO_RATE, AudioDecoder},
};
use fold_project::Snapshot;
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, AtomicI64, AtomicU8, AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};

struct Request {
    snapshot: Snapshot,
    document: fold_foundation::DocumentId,
    frame: u32,
    end: u32,
    generation: u64,
    cancel: Cancel,
}
#[derive(Default)]
struct Queue {
    pending: Option<Request>,
    stop: bool,
    error: Option<String>,
}
struct Shared {
    generation: AtomicU64,
    mode: AtomicU8, // 0 paused, 1 priming, 2 playing, 3 ended, 4 failed
    sample: AtomicI64,
    stamp: AtomicU64,
    clock_version: AtomicU64,
    start: AtomicU64,
    end: AtomicU64,
    underruns: AtomicU64,
    device_error: AtomicBool,
    audio_clock: AtomicBool,
    epoch: Instant,
    queue: Mutex<Queue>,
    ready: Condvar,
}
pub struct Playback {
    shared: Arc<Shared>,
    last_sample: AtomicU64,
    cancel: Cancel,
    thread: Option<std::thread::JoinHandle<()>>,
}
impl Default for Playback {
    fn default() -> Self {
        let shared = Arc::new(Shared {
            generation: AtomicU64::new(0),
            mode: AtomicU8::new(0),
            sample: AtomicI64::new(0),
            stamp: AtomicU64::new(0),
            clock_version: AtomicU64::new(0),
            start: AtomicU64::new(0),
            end: AtomicU64::new(0),
            underruns: AtomicU64::new(0),
            device_error: AtomicBool::new(false),
            audio_clock: AtomicBool::new(false),
            epoch: Instant::now(),
            queue: Mutex::new(Queue::default()),
            ready: Condvar::new(),
        });
        let state = shared.clone();
        let thread = std::thread::spawn(move || {
            loop {
                let request = {
                    let mut queue = state.queue.lock().unwrap();
                    while queue.pending.is_none() && !queue.stop {
                        queue = state.ready.wait(queue).unwrap();
                    }
                    if queue.stop {
                        break;
                    }
                    queue.pending.take().unwrap()
                };
                let result = run(&state, &request);
                // Serialize final status against request replacement.
                let mut queue = state.queue.lock().unwrap();
                if state.generation.load(Ordering::Acquire) == request.generation {
                    match result {
                        Ok(()) => {
                            state.mode.store(3, Ordering::Release);
                        }
                        Err(error) => {
                            queue.error = Some(error);
                            state.mode.store(4, Ordering::Release);
                        }
                    }
                }
            }
        });
        Self {
            shared,
            last_sample: AtomicU64::new(0),
            cancel: Cancel::default(),
            thread: Some(thread),
        }
    }
}
impl Playback {
    pub fn play(
        &mut self,
        snapshot: Snapshot,
        document: fold_foundation::DocumentId,
        frame: u32,
        end: u32,
    ) {
        self.pause();
        self.last_sample.store(0, Ordering::Release);
        self.cancel = Cancel::default();
        let mut queue = self.shared.queue.lock().unwrap();
        let generation = self.shared.generation.load(Ordering::Acquire);
        queue.error = None;
        queue.pending = Some(Request {
            snapshot,
            document,
            frame,
            end,
            generation,
            cancel: self.cancel.clone(),
        });
        self.shared.underruns.store(0, Ordering::Release);
        self.shared.mode.store(1, Ordering::Release);
        self.shared.ready.notify_one();
    }
    pub fn pause(&mut self) {
        let mut queue = self.shared.queue.lock().unwrap();
        self.shared.generation.fetch_add(1, Ordering::AcqRel);
        self.cancel.cancel();
        queue.pending = None;
        self.shared.mode.store(0, Ordering::Release);
    }
    pub fn playing(&self) -> bool {
        matches!(self.shared.mode.load(Ordering::Acquire), 1 | 2)
    }
    pub fn audio_clock(&self) -> bool {
        self.shared.audio_clock.load(Ordering::Acquire)
    }
    pub fn priming(&self) -> bool {
        self.shared.mode.load(Ordering::Acquire) == 1
    }
    pub fn underruns(&self) -> u64 {
        self.shared.underruns.load(Ordering::Acquire)
    }
    pub fn take_error(&self) -> Option<String> {
        self.shared.queue.lock().unwrap().error.take()
    }
    pub fn sample(&self) -> Option<u64> {
        if !matches!(self.shared.mode.load(Ordering::Acquire), 2 | 3) {
            return None;
        }
        loop {
            let version = self.shared.clock_version.load(Ordering::Acquire);
            if !version.is_multiple_of(2) {
                continue;
            }
            let sample = self.shared.sample.load(Ordering::Relaxed);
            let stamp = self.shared.stamp.load(Ordering::Relaxed);
            std::sync::atomic::fence(Ordering::Acquire);
            if version != self.shared.clock_version.load(Ordering::Relaxed) {
                continue;
            }
            let elapsed = self.shared.epoch.elapsed().as_nanos() as u64;
            let delta =
                elapsed.saturating_sub(stamp) as u128 * u128::from(AUDIO_RATE) / 1_000_000_000;
            let position = (sample + delta.min(i64::MAX as u128) as i64)
                .max(self.shared.start.load(Ordering::Acquire) as i64)
                .min(self.shared.end.load(Ordering::Acquire) as i64)
                as u64;
            // Device latency estimates can jitter by a few samples. Hold until
            // the corrected estimate catches up; only explicit seeks go back.
            return Some(
                self.last_sample
                    .fetch_max(position, Ordering::AcqRel)
                    .max(position),
            );
        }
    }
}
impl Drop for Playback {
    fn drop(&mut self) {
        self.pause();
        self.shared.queue.lock().unwrap().stop = true;
        self.shared.ready.notify_one();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

#[cfg(test)]
#[path = "playback_tests.rs"]
mod tests;

fn silent_transport(
    shared: &Shared,
    request: &Request,
    start: u64,
    end: u64,
) -> Result<(), String> {
    request.cancel.check()?;
    let began = Instant::now();
    {
        let _queue = shared.queue.lock().unwrap();
        if shared.generation.load(Ordering::Acquire) != request.generation {
            return Err("obsolete transport request".into());
        }
        shared.audio_clock.store(false, Ordering::Release);
        shared.start.store(start, Ordering::Release);
        shared.end.store(end, Ordering::Release);
        shared.sample.store(start as i64, Ordering::Release);
        shared
            .stamp
            .store(shared.epoch.elapsed().as_nanos() as u64, Ordering::Release);
        shared.mode.store(2, Ordering::Release);
    }
    let duration = Duration::from_secs_f64((end - start) as f64 / f64::from(AUDIO_RATE));
    while began.elapsed() < duration {
        request.cancel.check()?;
        std::thread::sleep(Duration::from_millis(2));
    }
    Ok(())
}

fn run(shared: &Arc<Shared>, request: &Request) -> Result<(), String> {
    let registry = crate::packages::builtins();
    let info = registry.output(&request.snapshot, request.document)?;
    let end_frame = request.end.min(info.frames);
    if request.frame >= end_frame {
        return Err("play position outside marked range".into());
    }
    let sample = |frame| {
        info.time(frame)?
            .to_ticks(AUDIO_RATE, 1, Rounding::Ceil)
            .map(|v| v as u64)
            .map_err(|e| e.to_string())
    };
    let start = sample(request.frame)?;
    let end = sample(end_frame)?;
    if !registry.supports_audio(&request.snapshot, request.document) {
        return silent_transport(shared, request, start, end);
    }
    let mut plan = registry.audio(&request.snapshot, request.document)?;
    plan.end = end;
    if plan.regions.is_empty() {
        return silent_transport(shared, request, start, end);
    }
    shared.audio_clock.store(true, Ordering::Release);
    let mut decoder = AudioDecoder::default();
    plan.preflight(&mut decoder, &request.cancel)?;
    request.cancel.check()?;
    let device = cpal::default_host()
        .default_output_device()
        .ok_or("no audio output device")?;
    let supported = device
        .supported_output_configs()
        .map_err(|e| e.to_string())?
        .any(|c| {
            c.channels() == 2
                && c.sample_format() == cpal::SampleFormat::F32
                && c.min_sample_rate().0 <= AUDIO_RATE
                && c.max_sample_rate().0 >= AUDIO_RATE
        });
    if !supported {
        return Err("audio device must support stereo float32 at 48 kHz".into());
    }
    let config = cpal::StreamConfig {
        channels: 2,
        sample_rate: cpal::SampleRate(AUDIO_RATE),
        buffer_size: cpal::BufferSize::Default,
    };
    let (mut producer, consumer) = rtrb::RingBuffer::<(u64, [f32; 2])>::new(AUDIO_RATE as usize);
    let mut next = start;
    let count = (plan.end - next).min(24_000) as usize;
    for frame in plan.evaluate(next, count, &mut decoder, &request.cancel)? {
        producer
            .push((next, frame))
            .map_err(|_| "audio prime ring full")?;
        next += 1;
    }
    shared.start.store(start, Ordering::Release);
    shared.end.store(plan.end, Ordering::Release);
    shared.device_error.store(false, Ordering::Release);
    shared.sample.store(start as i64, Ordering::Release);
    shared
        .stamp
        .store(shared.epoch.elapsed().as_nanos() as u64, Ordering::Release);
    let generation = request.generation;
    let state = shared.clone();
    let error_state = shared.clone();
    let needed = Arc::new(AtomicU64::new(start));
    let callback_needed = needed.clone();
    let mut callback = output::AudioOutput {
        consumer,
        cursor: start,
        end: plan.end,
    };
    let stream = device
        .build_output_stream(
            &config,
            move |output: &mut [f32], info: &cpal::OutputCallbackInfo| {
                let block_start = callback.cursor;
                let underrun = callback.fill(
                    output,
                    state.generation.load(Ordering::Acquire) == generation,
                );
                let timestamp = info.timestamp();
                let latency = timestamp
                    .playback
                    .duration_since(&timestamp.callback)
                    .unwrap_or_default();
                let latency_samples = (latency.as_nanos() * u128::from(AUDIO_RATE) / 1_000_000_000)
                    .min(i64::MAX as u128) as i64;
                if state.generation.load(Ordering::Acquire) != generation {
                    output.fill(0.0);
                    return;
                }
                if underrun {
                    state.underruns.fetch_add(1, Ordering::Relaxed);
                }
                state.clock_version.fetch_add(1, Ordering::AcqRel);
                state
                    .sample
                    .store(block_start as i64 - latency_samples, Ordering::Relaxed);
                state
                    .stamp
                    .store(state.epoch.elapsed().as_nanos() as u64, Ordering::Relaxed);
                state.clock_version.fetch_add(1, Ordering::Release);
                callback_needed.store(callback.cursor, Ordering::Release);
            },
            move |_| {
                error_state.device_error.store(true, Ordering::Release);
            },
            Some(Duration::from_secs(2)),
        )
        .map_err(|e| e.to_string())?;
    request.cancel.check()?;
    stream.play().map_err(|e| e.to_string())?;
    let mut progress = (start, Instant::now());
    loop {
        request.cancel.check()?;
        let cursor = needed.load(Ordering::Acquire);
        if cursor != progress.0 {
            progress = (cursor, Instant::now());
            let _queue = shared.queue.lock().unwrap();
            if shared.generation.load(Ordering::Acquire) == generation {
                shared.mode.store(2, Ordering::Release);
            }
        } else if progress.1.elapsed() > Duration::from_secs(2) {
            return Err("audio callback stopped advancing".into());
        }
        if shared.device_error.load(Ordering::Acquire) {
            return Err("audio output device failed".into());
        }
        let audible = shared.sample.load(Ordering::Acquire);
        if audible >= plan.end as i64 {
            break;
        }
        // Don't enqueue audio whose presentation deadline has passed.
        next = next.max(needed.load(Ordering::Acquire)).min(plan.end);
        let count = (plan.end - next).min(4096) as usize;
        if count > 0 && producer.slots() >= count {
            for frame in plan.evaluate(next, count, &mut decoder, &request.cancel)? {
                producer
                    .push((next, frame))
                    .map_err(|_| "audio ring unexpectedly full")?;
                next += 1;
            }
        } else {
            std::thread::sleep(Duration::from_millis(2));
        }
    }
    drop(stream);
    Ok(())
}
