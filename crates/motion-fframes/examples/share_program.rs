//! Verify native typography and actual compiled script dots through GPU FD transport.
#![allow(unsafe_code)]
use fold_gpu_sharing::{Imported, identity, socket};
use fold_media::process::{Cancel, Process};
use fold_motion_canvas::{
    model::{Content, ControlValue},
    protocol::{Alpha, Encoding, Request, Response},
};
use fold_render::{WorkingSpace, gpu::Host};
use std::{
    io::{BufRead, BufReader, Read, Write},
    os::unix::net::UnixListener,
    path::PathBuf,
    process::{Command, Stdio},
    time::{Duration, Instant},
};
struct Diagnostics;
static DIAGNOSTICS: Diagnostics = Diagnostics;
static GPU_ERROR: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
impl log::Log for Diagnostics {
    fn enabled(&self, m: &log::Metadata<'_>) -> bool {
        m.level() <= log::Level::Warn
    }
    fn log(&self, r: &log::Record<'_>) {
        if self.enabled(r.metadata()) {
            if r.level() == log::Level::Error {
                GPU_ERROR.store(true, std::sync::atomic::Ordering::Relaxed);
            }
            eprintln!("{}: {}", r.level(), r.args());
        }
    }
    fn flush(&self) {}
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let _ = log::set_logger(&DIAGNOSTICS);
    log::set_max_level(log::LevelFilter::Warn);
    let directory = PathBuf::from(std::env::args().nth(1).ok_or("Expected scene directory")?);
    let program: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("prepared-program.json"))?)?;
    let (host, adapter) = pollster::block_on(Host::headless(256 * 1024 * 1024))?;
    let (uuid, _) = identity(host.device())?;
    let temporary = tempfile::tempdir()?;
    let path = temporary.path().join("motion.sock");
    let listener = UnixListener::bind(&path)?;
    listener.set_nonblocking(true)?;
    let mut command = Command::new(
        program["executable"]
            .as_str()
            .ok_or("Missing prepared binary")?,
    );
    command
        .env("FOLD_MOTION_GPU_SOCKET", &path)
        .env("FOLD_MOTION_GPU_DEVICE", serde_json::to_string(&uuid)?)
        .env("FOLD_MOTION_PROFILE", "1")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped());
    let mut process = Process::timed(command, Cancel::default(), Duration::from_secs(90))?;
    let start = Instant::now();
    let mut socket = loop {
        match listener.accept() {
            Ok((socket, _)) => break socket,
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                if process.is_finished() {
                    process.finish()?;
                    return Err("Renderer exited before connecting".into());
                }
                if start.elapsed() > Duration::from_secs(30) {
                    return Err("Renderer connection timeout".into());
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Err(e) => return Err(e.into()),
        }
    };
    socket.set_read_timeout(Some(Duration::from_secs(30)))?;
    socket.set_write_timeout(Some(Duration::from_secs(30)))?;
    let mut input = process.stdin.take().ok_or("Missing renderer input")?;
    let mut output = BufReader::new(process.stdout.take().ok_or("Missing renderer output")?);
    let requests = BufReader::new(std::fs::File::open(directory.join("requests.jsonl"))?);
    let mut pixels_out =
        std::io::BufWriter::new(std::fs::File::create(directory.join("gpu-frames.bin"))?);
    let mut timings = vec![];
    for line in requests.lines() {
        let mut request: Request = serde_json::from_str(&line?)?;
        if request.id == 2 {
            for object in request.canvas.objects.values_mut() {
                if let Content::Script { controls, .. } = &mut object.content {
                    for control in controls.values_mut() {
                        if control.name == "Count" {
                            if let ControlValue::Number(value) = &mut control.value {
                                value.value = 8000.;
                            }
                        }
                    }
                }
            }
        }
        let start = Instant::now();
        serde_json::to_writer(&mut input, &request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut header = String::new();
        output.by_ref().take(16 * 1024).read_line(&mut header)?;
        let response: Response = serde_json::from_str(&header)?;
        if let Some(error) = response.error {
            return Err(error.into());
        }
        if response.id != request.id
            || response.dimensions != request.dimensions
            || response.bytes != 0
        {
            return Err("GPU response does not match demand".into());
        }
        let (description, fd) = socket::receive(&socket)?;
        if description.dimensions != request.dimensions {
            return Err("GPU allocation dimensions mismatch".into());
        }
        let permit = host.reserve_shared_image(&description)?;
        let imported = unsafe { Imported::new(host.device(), description, fd, 256 * 1024 * 1024)? };
        let mut frame = permit.receive(imported, WorkingSpace::LegacyLinearSrgb, None)?;
        if frame.statistics.upload_bytes != 0
            || frame.statistics.readback_bytes != 0
            || frame.statistics.shared_gpu_copy_bytes
                != u64::from(request.dimensions[0]) * u64::from(request.dimensions[1]) * 8
        {
            return Err("Shared frame transfer statistics mismatch".into());
        }
        let pixels = frame.readback(&Cancel::default())?;
        if !frame.is_ready()? || host.memory().allocated == 0 {
            return Err("Shared frame lost readiness or allocation ownership".into());
        }
        socket.write_all(&request.id.to_le_bytes())?;
        let mut released = [0u8; 8];
        socket.read_exact(&mut released)?;
        if u64::from_le_bytes(released) != request.id {
            return Err("GPU release confirmation mismatch".into());
        }
        timings.push(start.elapsed().as_secs_f64() * 1000.);
        let header = Response {
            id: request.id,
            dimensions: request.dimensions,
            bytes: request.dimensions[0] as usize * request.dimensions[1] as usize * 16,
            encoding: Encoding::LinearSrgb,
            alpha: Alpha::Premultiplied,
            error: None,
        };
        serde_json::to_writer(&mut pixels_out, &header)?;
        pixels_out.write_all(b"\n")?;
        for pixel in pixels.pixels() {
            for value in pixel {
                pixels_out.write_all(&value.to_le_bytes())?;
            }
        }
        drop(frame);
        drop(pixels);
        host.poll()?;
        if host.memory().allocated != 0 {
            return Err("Shared frame retained GPU accounting after completion and drop".into());
        }
    }
    pixels_out.flush()?;
    drop(input);
    // Drain stdout to prove no hidden CPU pixel body was sent after GPU headers.
    let mut remaining = vec![];
    output.read_to_end(&mut remaining)?;
    if !remaining.is_empty() {
        return Err("Unexpected GPU worker stdout payload".into());
    }
    let diagnostics = process.diagnostics();
    eprintln!("{diagnostics}");
    std::fs::write(directory.join("gpu-worker-diagnostics.txt"), &diagnostics)?;
    process.finish()?;
    let ingress = qualify_ingress(&host, uuid)?;
    if diagnostics.contains("Validation Error")
        || GPU_ERROR.load(std::sync::atomic::Ordering::Relaxed)
    {
        return Err("Vulkan validation failed".into());
    }
    std::fs::write(
        directory.join("gpu-transfer-evidence.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
        "device":adapter.name,"frame_request_ms_including_qualification_readback":timings,"cpu_pixel_transport":false,
        "render_host_frame":true,"host_memory_released_after_completed_frame_drop":true,
        "ingress":ingress,
        "fframes_script_worker":true,"native_typography":true,"performance_qualified":false }))?,
    )?;
    Ok(())
}

fn qualify_ingress(
    host: &Host,
    uuid: [u8; 16],
) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let mut producer = fold_gpu_sharing::Device::new(uuid)?;
    for (color, invalid) in [
        ([1.5, -0.125, 0.25, 0.5], false),
        ([0.0, 0.0, 0.0, 1.5], true),
    ] {
        let mut image = producer.image([32, 16], 1024 * 1024)?;
        image.clear(color)?;
        // The clear operation completes in GENERAL; export is kept alive until
        // the host finishes every submitted copy/validation referencing it.
        let export = unsafe { image.publish(ash::vk::ImageLayout::GENERAL)? };
        let mut oversized = export.description.clone();
        oversized.allocation_bytes = host.memory().budget + 1;
        if host.reserve_shared_image(&oversized).is_ok() || host.memory().allocated != 0 {
            return Err("Oversized ingress reservation was accepted or leaked".into());
        }
        let mut wrong_device = export.description.clone();
        wrong_device.device[0] ^= 1;
        if host.reserve_shared_image(&wrong_device).is_ok() {
            return Err("Wrong-device ingress reservation was accepted".into());
        }
        let permit = host.reserve_shared_image(&export.description)?;
        let imported = unsafe {
            Imported::new(
                host.device(),
                export.description.clone(),
                export.fd.try_clone()?,
                1024 * 1024,
            )?
        };
        let mut frame = permit.receive(imported, WorkingSpace::LegacyLinearSrgb, None)?;
        let result = frame.readback(&Cancel::default());
        if invalid {
            if result.is_ok() {
                return Err("GPU ingress published invalid alpha".into());
            }
        } else {
            let pixels = result?;
            if pixels.pixels().iter().any(|pixel| *pixel != color) {
                return Err("GPU ingress changed signed/HDR pixels".into());
            }
        }
        drop(frame);
        host.poll()?;
        if host.memory().allocated != 0 {
            return Err("Completed ingress retained an allocation".into());
        }
    }
    let mut image = producer.image([32, 16], 1024 * 1024)?;
    image.clear([0.0, 0.0, 0.0, 0.5])?;
    let export = unsafe { image.publish(ash::vk::ImageLayout::GENERAL)? };
    let permit = host.reserve_shared_image(&export.description)?;
    let imported = unsafe {
        Imported::new(
            host.device(),
            export.description.clone(),
            export.fd.try_clone()?,
            1024 * 1024,
        )?
    };
    let frame = permit.receive(imported, WorkingSpace::LegacyLinearSrgb, None)?;
    drop(frame); // cancellation before any caller wait/readback
    let start = Instant::now();
    while host.memory().allocated != 0 {
        host.poll()?;
        if start.elapsed() > Duration::from_secs(30) {
            return Err("Abandoned ingress retained an allocation after completion".into());
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    Ok(
        serde_json::json!({"budget_rejection":true,"wrong_device_rejection":true,"signed_hdr_preserved":true,"invalid_alpha_rejected_by_gpu_status":true,"memory_released":true,"abandoned_frame_released_after_completion":true}),
    )
}
