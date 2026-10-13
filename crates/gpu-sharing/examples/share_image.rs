//! Actual separate-process Vulkan FD exchange; CPU readback only verifies output.
#![allow(unsafe_code)]
use fold_gpu_sharing::{Device, Imported, identity, socket};
use fold_media::process::{Cancel, Process};
use std::{
    io::{Read, Write},
    os::{
        fd::AsFd,
        unix::net::{UnixListener, UnixStream},
    },
    path::PathBuf,
    process::Command,
    time::{Duration, Instant},
};
const COLOR: [f32; 4] = [1.5, -0.125, 0.25, 0.5];
static GPU_ERROR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
struct Diagnostics;
static DIAGNOSTICS: Diagnostics = Diagnostics;
impl log::Log for Diagnostics {
    fn enabled(&self, metadata: &log::Metadata<'_>) -> bool {
        metadata.level() <= log::Level::Warn
    }
    fn log(&self, record: &log::Record<'_>) {
        if self.enabled(record.metadata()) {
            if record.level() == log::Level::Error {
                GPU_ERROR.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            eprintln!("{}: {}", record.level(), record.args());
        }
    }
    fn flush(&self) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = log::set_logger(&DIAGNOSTICS);
    log::set_max_level(log::LevelFilter::Warn);
    let args: Vec<_> = std::env::args().collect();
    if args.get(1).map(String::as_str) == Some("--producer") {
        let uuid = serde_json::from_str(&args[3])?;
        let mut device = Device::new(uuid)?;
        let mut image = device.image([32, 16], 1024 * 1024)?;
        image.clear(COLOR)?;
        let export = unsafe { image.publish(ash::vk::ImageLayout::GENERAL)? };
        let mut stream = UnixStream::connect(&args[2])?;
        stream.set_read_timeout(Some(Duration::from_secs(30)))?;
        socket::send(&stream, &export.description, export.fd.as_fd())?;
        let mut ack = [0u8; 1];
        stream.read_exact(&mut ack)?;
        if ack != [1] {
            return Err("Invalid consumer completion".into());
        }
        return Ok(());
    }
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))?;
    let (device, queue) =
        pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor::default()))?;
    let (uuid, _) = identity(&device)?;
    let temp = tempfile::tempdir()?;
    let path: PathBuf = temp.path().join("image.sock");
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    let mut command = Command::new(std::env::current_exe()?);
    command
        .arg("--producer")
        .arg(&path)
        .arg(serde_json::to_string(&uuid)?);
    let child = Process::timed(command, Cancel::default(), Duration::from_secs(30))?;
    let start = Instant::now();
    let (mut stream, _) = loop {
        match listener.accept() {
            Ok(socket) => break socket,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if child.is_finished() {
                    child.finish()?;
                    return Err("Producer exited before connecting".into());
                }
                if start.elapsed() > Duration::from_secs(30) {
                    return Err("Producer connection deadline exceeded".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e.into()),
        }
    };
    stream.set_read_timeout(Some(Duration::from_secs(30)))?;
    let (description, fd) = socket::receive(&stream)?;
    let imported = unsafe { Imported::new(&device, description.clone(), fd, 1024 * 1024)? };
    let (texture, _) = imported.copy(&queue)?;
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size: 256 * 16,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor::default());
    encoder.copy_texture_to_buffer(
        texture.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: None,
            },
        },
        texture.size(),
    );
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
    device.poll(wgpu::PollType::Wait {
        submission_index: None,
        timeout: Some(Duration::from_secs(30)),
    })?;
    rx.recv_timeout(Duration::from_secs(30))??;
    let bytes = buffer.slice(..).get_mapped_range();
    for pixel in bytes.chunks_exact(8) {
        for (i, expected) in COLOR.iter().enumerate() {
            let value =
                half::f16::from_bits(u16::from_le_bytes([pixel[i * 2], pixel[i * 2 + 1]])).to_f32();
            if (value - *expected).abs() > 0.001 {
                return Err(format!("Shared pixel {i} is {value}, expected {expected}").into());
            }
        }
    }
    drop(bytes);
    buffer.unmap();
    stream.write_all(&[1])?;
    let diagnostics = child.diagnostics();
    if !diagnostics.is_empty() {
        eprintln!("Producer diagnostics: {diagnostics}");
    }
    if diagnostics.contains("Validation Error") {
        return Err("Vulkan producer validation failed".into());
    }
    child.finish()?;
    if GPU_ERROR.load(std::sync::atomic::Ordering::Relaxed) {
        return Err("GPU validation reported an error".into());
    }
    println!(
        "{}",
        serde_json::json!({"gpu":adapter.get_info().name,"dimensions":description.dimensions,"allocation_bytes":description.allocation_bytes,"cross_process_fd":true,"gpu_copy":true,"signed_hdr_alpha_preserved":true})
    );
    Ok(())
}
