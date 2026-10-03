//! Demand-driven retained FFmpeg range decoder. Pipes, not temporary RGB files.
//! At most one frame is being read per stream; the OS pipe backpressures FFmpeg.
use crate::{Cancel, process::Process};
use std::{
    io::Read,
    process::Command,
    sync::mpsc::{self, Receiver, SyncSender},
    thread::JoinHandle,
    time::{Duration, Instant},
};

#[cfg(test)]
#[path = "video_stream_tests.rs"]
mod tests;

pub(crate) struct Stream {
    process: Option<Process>,
    demand: Option<SyncSender<()>>,
    frames: Receiver<Result<(Vec<u8>, std::sync::Arc<crate::budget::Lease>), String>>,
    reader: Option<JoinHandle<()>>,
}
impl Stream {
    pub fn new(command: Command, frame_bytes: usize) -> Result<Self, String> {
        // A request token must not own a retained process's lifetime. Each read
        // observes its current caller's token; failure drops/kills the stream.
        let mut process = Process::retained(command)?;
        let mut stdout = process.stdout.take().ok_or("decoder stdout is not piped")?;
        let (demand, requests) = mpsc::sync_channel(1);
        let (send, frames) = mpsc::sync_channel(1);
        let reader = std::thread::spawn(move || {
            while requests.recv().is_ok() {
                let mut bytes = Vec::new();
                let reservation = crate::budget::PIPE
                    .reserve(frame_bytes as u64)
                    .map_err(str::to_owned);
                let result = reservation.and_then(|reservation| {
                    bytes
                        .try_reserve_exact(frame_bytes)
                        .map_err(|_| "decoder frame allocation failed".to_owned())
                        .and_then(|()| {
                            bytes.resize(frame_bytes, 0);
                            stdout
                                .read_exact(&mut bytes)
                                .map(|()| (bytes, reservation))
                                .map_err(|error| {
                                    format!("incomplete retained video decode: {error}")
                                })
                        })
                });
                let failed = result.is_err();
                if send.send(result).is_err() || failed {
                    break;
                }
            }
        });
        Ok(Self {
            process: Some(process),
            demand: Some(demand),
            frames,
            reader: Some(reader),
        })
    }
    pub fn read(
        &mut self,
        cancel: &Cancel,
    ) -> Result<(Vec<u8>, std::sync::Arc<crate::budget::Lease>), String> {
        let result = self.read_next(cancel);
        if result.is_err() {
            self.demand.take();
            self.process.take();
        }
        result
    }
    fn read_next(
        &mut self,
        cancel: &Cancel,
    ) -> Result<(Vec<u8>, std::sync::Arc<crate::budget::Lease>), String> {
        cancel.check()?;
        self.demand
            .as_ref()
            .ok_or("decoder stopped")?
            .send(())
            .map_err(|_| "decoder reader stopped")?;
        let start = Instant::now();
        loop {
            cancel.check()?;
            match self.frames.recv_timeout(Duration::from_millis(10)) {
                Ok(Ok(bytes)) => return Ok(bytes),
                Ok(Err(error)) => {
                    let details = self
                        .process
                        .take()
                        .and_then(|process| process.abort().err())
                        .unwrap_or_default();
                    return Err(format!("{error}; {details}"));
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    return Err("decoder reader disconnected".into());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {}
            }
            if start.elapsed() > Duration::from_secs(30) {
                return Err("decoder frame timeout (30s)".into());
            }
        }
    }
}
impl Drop for Stream {
    fn drop(&mut self) {
        // Kill/reap first to unblock an active pipe read, then join the reader.
        self.process.take();
        self.demand.take();
        if let Some(reader) = self.reader.take() {
            let _ = reader.join();
        }
    }
}
