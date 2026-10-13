//! Persistent worker entry for compiled fframes script programs. Source changes
//! build a new program; controls and native objects arrive as runtime requests.
use crate::{Conversion, Renderer, Scripts, document};
use fold_motion_canvas::protocol::{
    self, Alpha, CaptureResponse, Encoding, Operation, Request, Response,
};
use std::io::{BufRead, Read, Write};
use std::sync::Arc;
use std::time::Instant;
use std::{
    os::{fd::AsFd, unix::net::UnixStream},
    time::Duration,
};

enum Frame {
    Pixels(Vec<[f32; 4]>),
    Shared(fold_gpu_sharing::Export),
}

/// One bounded JSON demand and correlated response. GPU mode publishes an
/// immutable image FD and retains it until completion; standalone qualification
/// returns little-endian RGBA32F pixels instead.
pub fn run(mut scripts: impl Scripts) -> Result<(), String> {
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let stdout = std::io::stdout();
    let mut output = std::io::BufWriter::with_capacity(64 * 1024, stdout.lock());
    let mut renderer: Option<([u32; 2], Arc<Vec<Vec<u8>>>, Renderer)> = None;
    let mut conversion: Option<(Arc<Vec<Vec<u8>>>, Conversion)> = None;
    let mut font_data: Option<Arc<Vec<Vec<u8>>>> = None;
    let mut resources = Conversion::new([]);
    let mut resource_limit = 0;
    let profile = std::env::var_os("FOLD_MOTION_PROFILE").is_some();
    let mut shared = std::env::var_os("FOLD_MOTION_GPU_SOCKET")
        .map(|path| UnixStream::connect(path).map_err(|e| e.to_string()))
        .transpose()?;
    let uuid: Option<[u8; 16]> = match std::env::var("FOLD_MOTION_GPU_DEVICE") {
        Ok(uuid) => Some(serde_json::from_str(&uuid).map_err(|e| e.to_string())?),
        Err(_) if shared.is_some() => {
            return Err("GPU sharing requires the receiving device UUID".into());
        }
        Err(_) => None,
    };
    if let Some(socket) = &shared {
        socket
            .set_read_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
        socket
            .set_write_timeout(Some(Duration::from_secs(30)))
            .map_err(|e| e.to_string())?;
    }
    loop {
        let mut line = Vec::new();
        let bytes = input
            .by_ref()
            .take((protocol::MAX_REQUEST_BYTES + 1) as u64)
            .read_until(b'\n', &mut line)
            .map_err(|e| e.to_string())?;
        if bytes == 0 {
            return Ok(());
        }
        if bytes > protocol::MAX_REQUEST_BYTES || line.last() != Some(&b'\n') {
            return Err("Motion worker request exceeds its transport allowance".into());
        }
        let start = Instant::now();
        let request: Request = serde_json::from_slice(&line).map_err(|e| e.to_string())?;
        let resource_update = (|| -> Result<(), String> {
            if let Some(snapshot) = request.images {
                let images = snapshot
                    .images
                    .into_iter()
                    .map(|(name, raster)| {
                        (
                            name,
                            std::sync::Arc::new(fframes::usvgr::PreloadedImageData {
                                id: raster.identity,
                                width: raster.dimensions[0],
                                height: raster.dimensions[1],
                                data: std::borrow::Cow::Owned(raster.rgba8_premultiplied_srgb),
                            }),
                        )
                    })
                    .collect();
                resources.set_images(images, snapshot.byte_limit)?;
                resource_limit = snapshot.byte_limit;
            }
            for asset in request.canvas.assets() {
                if !resources.images.contains_key(&asset.to_string()) {
                    return Err(format!("Missing resolved Motion image asset {asset}"));
                }
            }
            Ok(())
        })();
        if let Some(fonts) = request.fonts
            && font_data
                .as_ref()
                .is_none_or(|retained| **retained != fonts)
        {
            font_data = Some(Arc::new(fonts));
        }
        if let Operation::Capture { object } = request.operation {
            let result = (|| -> Result<Vec<u8>, String> {
                resource_update.clone()?;
                let changed = conversion.as_ref().is_none_or(|(fonts, _)| {
                    font_data
                        .as_ref()
                        .is_none_or(|retained| !Arc::ptr_eq(retained, fonts))
                });
                if changed {
                    let fonts = font_data
                        .clone()
                        .ok_or("Initialize capture with explicit font bytes")?;
                    conversion = Some((fonts.clone(), Conversion::new(fonts.iter().cloned())));
                }
                let frame =
                    crate::script_output(&request.canvas, object, request.time, &mut scripts)?;
                conversion
                    .as_mut()
                    .expect("created conversion")
                    .1
                    .set_images(resources.images.clone(), resource_limit)?;
                let tree = conversion
                    .as_mut()
                    .expect("created conversion")
                    .1
                    .evaluate(frame)?;
                let captured = crate::capture::tree(&tree, &resources.images)?;
                let body = serde_json::to_vec(&captured).map_err(|error| error.to_string())?;
                if body.len() > protocol::MAX_REQUEST_BYTES {
                    return Err("Captured artwork exceeds its transport allowance".into());
                }
                Ok(body)
            })();
            let (body, error) = match result {
                Ok(body) => (body, None),
                Err(error) => (
                    Vec::new(),
                    Some(error.chars().take(2048).collect::<String>()),
                ),
            };
            serde_json::to_writer(
                &mut output,
                &CaptureResponse {
                    id: request.id,
                    bytes: body.len(),
                    error,
                },
            )
            .map_err(|error| error.to_string())?;
            output.write_all(b"\n").map_err(|error| error.to_string())?;
            output.write_all(&body).map_err(|error| error.to_string())?;
            output.flush().map_err(|error| error.to_string())?;
            continue;
        }
        let parse_ms = start.elapsed().as_secs_f64() * 1000.;
        let mut document_ms = 0.;
        let mut initialize_ms = 0.;
        let mut draw = crate::DrawTimings::default();
        let mut output_ms = 0.;
        let result = (|| -> Result<_, String> {
            resource_update?;
            let start = Instant::now();
            let frame = document(&request.canvas, request.time, &mut scripts)?;
            document_ms = start.elapsed().as_secs_f64() * 1000.;
            let start = Instant::now();
            let changed = renderer.as_ref().is_none_or(|(size, fonts, _)| {
                *size != request.dimensions
                    || font_data
                        .as_ref()
                        .is_none_or(|retained| !Arc::ptr_eq(retained, fonts))
            });
            if changed {
                renderer = None; // retire the old cache before allocating its replacement
                let fonts = font_data
                    .clone()
                    .ok_or("Initialize Motion with explicit font bytes")?;
                let next = Renderer::new(
                    uuid.map(Ok).unwrap_or_else(|| {
                        fold_gpu_sharing::Device::first_uuid().map_err(|error| error.to_string())
                    })?,
                    request.dimensions,
                    fonts.iter().cloned(),
                    request.allocation_limit,
                    request.cache_limit,
                )?;
                renderer = Some((request.dimensions, fonts, next));
            }
            initialize_ms = start.elapsed().as_secs_f64() * 1000.;
            let renderer = &mut renderer.as_mut().expect("created renderer").2;
            renderer.set_allocation_limit(request.allocation_limit)?;
            renderer.set_cache_limit(request.cache_limit)?;
            renderer.set_images(resources.images.clone(), resource_limit)?;
            draw = renderer.draw(frame)?;
            let start = Instant::now();
            let frame = if shared.is_some() {
                Frame::Shared(renderer.publish()?)
            } else {
                Frame::Pixels(renderer.readback()?)
            };
            output_ms = start.elapsed().as_secs_f64() * 1000.;
            Ok(frame)
        })();
        let cache_bytes = renderer
            .as_ref()
            .map_or(0, |(_, _, renderer)| renderer.cache_bytes());
        if result.is_err() {
            renderer = None; // release retained GPU resources before reporting failure
        }
        let (dimensions, frame, error) = match result {
            Ok(frame) => (request.dimensions, Some(frame), None),
            Err(error) => (
                [0, 0],
                None,
                Some(error.chars().take(2048).collect::<String>()),
            ),
        };
        let pixels = match &frame {
            Some(Frame::Pixels(pixels)) => pixels.as_slice(),
            _ => &[],
        };
        let header = Response {
            id: request.id,
            dimensions,
            bytes: pixels.len() * 16,
            encoding: Encoding::LinearSrgb,
            alpha: Alpha::Premultiplied,
            error,
        };
        let start = Instant::now();
        serde_json::to_writer(&mut output, &header).map_err(|e| e.to_string())?;
        output.write_all(b"\n").map_err(|e| e.to_string())?;
        #[cfg(target_endian = "little")]
        output
            .write_all(bytemuck::cast_slice(&pixels))
            .map_err(|e| e.to_string())?;
        #[cfg(target_endian = "big")]
        for component in pixels.iter().flatten() {
            output
                .write_all(&component.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        output.flush().map_err(|e| e.to_string())?;
        if let Some(Frame::Shared(image)) = frame {
            let socket = shared.as_mut().expect("shared output");
            fold_gpu_sharing::socket::send(socket, &image.description, image.fd.as_fd())
                .map_err(|e| e.to_string())?;
            let mut ack = [0u8; 8];
            socket.read_exact(&mut ack).map_err(|e| e.to_string())?;
            if u64::from_le_bytes(ack) != request.id {
                return Err("GPU completion does not match its frame".into());
            }
            drop(image);
            socket
                .write_all(&request.id.to_le_bytes())
                .map_err(|e| e.to_string())?;
        }
        if profile {
            eprintln!(
                "{}",
                serde_json::json!({"motion_profile": {"id": request.id,
                "parse_ms": parse_ms, "document_ms": document_ms,
                "initialize_ms": initialize_ms, "draw": draw,
                "output_mode": if shared.is_some() { "gpu_publish" } else { "cpu_readback" },
                "cache_bytes": cache_bytes, "cache_limit": request.cache_limit, "output_ms": output_ms, "transport_ms": start.elapsed().as_secs_f64()*1000.}})
            );
        }
    }
}
