//! Process-wide bounded block workers. Reservations travel with queued work and
//! completed blocks; cancellation never releases memory still used by a worker.
use super::{Cancel, budget, retain_scratch};
use ::exr::prelude::*;
use ::exr::{
    block::{UncompressedBlock, chunk::Chunk, reader::ChunksReader},
    image::read::layers::{ChannelsReader, ReadChannels},
    meta::MetaData,
};
use std::result::Result;
use std::sync::{Arc, Mutex, OnceLock, mpsc};

const WORKERS: usize = 6;
struct Job {
    chunk: Chunk,
    metadata: Arc<MetaData>,
    cancel: Cancel,
    retained: u64,
    retained_lease: Arc<budget::Lease>,
    selection: Selection,
    lease: Arc<budget::Lease>,
    result: mpsc::Sender<Result<Block, fold_foundation::Failure>>,
}
struct Block {
    index: ::exr::block::BlockIndex,
    pixels: Vec<[f32; 4]>,
    _lease: Arc<budget::Lease>,
}
struct Pool {
    sender: mpsc::SyncSender<Job>,
    workers: usize,
}
fn pool() -> Result<&'static Pool, fold_foundation::Failure> {
    static POOL: OnceLock<Result<Pool, fold_foundation::Failure>> = OnceLock::new();
    POOL.get_or_init(|| {
        let workers = std::thread::available_parallelism()
            .map_or(1, usize::from)
            .min(WORKERS);
        let (sender, receiver) = mpsc::sync_channel::<Job>(WORKERS);
        let receiver = Arc::new(Mutex::new(receiver));
        for index in 0..workers {
            let receiver = receiver.clone();
            std::thread::Builder::new()
                .name(format!("fold-exr-block-{index}"))
                .spawn(move || {
                    loop {
                        let Ok(job) = receiver.lock().unwrap().recv() else {
                            break;
                        };
                        let result = job.result.clone();
                        let _ = result.send(job.decode());
                    }
                })
                .map_err(|e| format!("EXR worker creation failed: {e}"))?;
        }
        Ok(Pool { sender, workers })
    })
    .as_ref()
    .map_err(Clone::clone)
}

#[derive(Clone)]
pub(super) struct Selection {
    pub names: [Option<String>; 4],
    pub remap: [usize; 4],
    pub opaque: bool,
}
impl Job {
    fn decode(self) -> Result<Block, fold_foundation::Failure> {
        self.cancel.check()?;
        retain_scratch(self.retained, self.retained_lease);
        let mut data = UncompressedBlock::decompress_chunk(self.chunk, &self.metadata, true)
            .map_err(|e| e.to_string())?;
        let index = data.index;
        let mut header = self.metadata.headers[index.layer].clone();
        header.layer_size = index.pixel_size;
        let names = self.selection.names.each_ref().map(|n| n.as_deref());
        let specification = ::exr::prelude::read()
            .no_deep_data()
            .largest_resolution_level()
            .specific_channels()
            .optional(names[0].unwrap_or("fold.__absent_r__"), 0f32)
            .optional(names[1].unwrap_or("fold.__absent_g__"), 0f32)
            .optional(names[2].unwrap_or("fold.__absent_b__"), 0f32)
            .optional(names[3].unwrap_or("fold.__absent_a__"), 0f32)
            .collect_pixels(
                |size, _| (size.width(), vec![[0f32; 4]; size.area()]),
                |pixels, position, (r, g, b, a): (f32, f32, f32, f32)| {
                    pixels.1[position.y() * pixels.0 + position.x()] = [r, g, b, a];
                },
            );
        let mut collector = specification
            .create_channels_reader(&header)
            .map_err(|e| e.to_string())?;
        data.index.pixel_position = Vec2(0, 0);
        collector
            .read_block(&header, data)
            .map_err(|e| e.to_string())?;
        let mut pixels = collector.into_channels().pixels.1;
        for row in pixels.chunks_mut(index.pixel_size.0) {
            self.cancel.check()?;
            for pixel in row {
                let original = *pixel;
                for i in 0..4 {
                    pixel[i] = original[self.selection.remap[i]];
                    if i == 3 && self.selection.opaque {
                        pixel[i] = 1.;
                    }
                    if !pixel[i].is_finite() {
                        return Err("EXR contains nonfinite requested samples".into());
                    }
                }
            }
        }
        Ok(Block {
            index,
            pixels,
            _lease: self.lease,
        })
    }
}

pub(super) fn read(
    mut chunks: impl ChunksReader,
    selection: Selection,
    size: Vec2<usize>,
    scratch: u64,
    retained: u64,
    cancel: &Cancel,
) -> Result<Vec<[f32; 4]>, fold_foundation::Failure> {
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(size.area())
        .map_err(|_| "EXR samples allocation failed")?;
    pixels.resize(size.area(), [0.; 4]);
    let pool = pool()?;
    let meta = Arc::new(chunks.meta_data().clone());
    let (send, receive) = mpsc::channel();
    let mut pending = 0;
    let mut finished = false;
    while !finished || pending > 0 {
        cancel.check()?;
        while !finished && pending < pool.workers {
            if chunks.len() == 0 {
                finished = true;
                break;
            }
            // Under pressure, consume outstanding work before requesting more
            // scratch. One block still makes progress with a sequential budget.
            let lease = match budget::reserve_working(scratch) {
                Ok(lease) => lease,
                Err(_) if pending > 0 => break,
                Err(error) => return Err(error),
            };
            // Reserve retained dependency storage before admitting the block.
            // The worker transfers only its growth into thread-local ownership.
            // This prevents queued blocks from starving a running decoder.
            let retained_lease = match budget::reserve_working(retained) {
                Ok(lease) => lease,
                Err(_) if pending > 0 => break,
                Err(error) => return Err(error),
            };
            let Some(chunk) = chunks.next() else {
                finished = true;
                break;
            };
            let mut job = Job {
                chunk: chunk.map_err(|e| e.to_string())?,
                metadata: meta.clone(),
                cancel: cancel.clone(),
                retained,
                retained_lease,
                selection: selection.clone(),
                lease,
                result: send.clone(),
            };
            loop {
                cancel.check()?;
                match pool.sender.try_send(job) {
                    Ok(()) => break,
                    Err(mpsc::TrySendError::Full(returned)) => {
                        job = returned;
                        std::thread::sleep(std::time::Duration::from_millis(1));
                    }
                    Err(mpsc::TrySendError::Disconnected(_)) => {
                        return Err("EXR workers stopped".into());
                    }
                }
            }
            pending += 1;
        }
        if pending > 0 {
            let block = loop {
                cancel.check()?;
                match receive.recv_timeout(std::time::Duration::from_millis(2)) {
                    Ok(result) => break result?,
                    Err(mpsc::RecvTimeoutError::Timeout) => continue,
                    Err(_) => return Err("EXR block worker stopped".into()),
                }
            };
            let index = block.index;
            for (y, row) in block.pixels.chunks_exact(index.pixel_size.0).enumerate() {
                let start = (index.pixel_position.1 + y) * size.0 + index.pixel_position.0;
                pixels[start..start + row.len()].copy_from_slice(row);
            }
            pending -= 1;
        }
    }
    Ok(pixels)
}
