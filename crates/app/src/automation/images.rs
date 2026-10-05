use super::*;
use base64::Engine;
use fold_media::Cancel;
pub fn rgba(width: u32, height: u32, bytes: &[u8], mut metadata: Value) -> Result<Value, String> {
    if width == 0 || height == 0 || bytes.len() != width as usize * height as usize * 4 {
        return Err("invalid captured image".into());
    }
    let scale = (1024.0 / f64::from(width.max(height))).min(1.0);
    let w = (f64::from(width) * scale).round().max(1.0) as u32;
    let h = (f64::from(height) * scale).round().max(1.0) as u32;
    let mut pixels = Vec::with_capacity((w * h * 4) as usize);
    for y in 0..h {
        for x in 0..w {
            let offset = (((u64::from(y) * u64::from(height) / u64::from(h)) * u64::from(width)
                + u64::from(x) * u64::from(width) / u64::from(w))
                * 4) as usize;
            pixels.extend_from_slice(&bytes[offset..offset + 4]);
        }
    }
    let mut output = Vec::new();
    {
        let mut encoder = png::Encoder::new(&mut output, w, h);
        encoder.set_color(png::ColorType::Rgba);
        encoder.set_depth(png::BitDepth::Eight);
        let mut writer = encoder.write_header().map_err(|e| e.to_string())?;
        writer
            .write_image_data(&pixels)
            .map_err(|e| e.to_string())?;
    }
    metadata["dimensions"] = json!([w, h]);
    metadata["original_dimensions"] = json!([width, height]);
    metadata["resampling"] = json!(if scale < 1.0 {
        "nearest diagnostic thumbnail"
    } else {
        "none"
    });
    Ok(
        json!({"metadata":metadata,"image":{"mimeType":"image/png","data":base64::engine::general_purpose::STANDARD.encode(output)}}),
    )
}
pub fn screenshot(cancel: &Cancel) -> Result<Value, String> {
    cancel.check()?;
    struct Wake(std::thread::Thread);
    impl std::task::Wake for Wake {
        fn wake(self: std::sync::Arc<Self>) {
            self.0.unpark();
        }
    }
    let waker = std::task::Waker::from(std::sync::Arc::new(Wake(std::thread::current())));
    let mut context = std::task::Context::from_waker(&waker);
    let mut future = std::pin::pin!(
        ashpd::desktop::screenshot::Screenshot::request()
            .interactive(true)
            .send()
    );
    let start = std::time::Instant::now();
    let request = loop {
        cancel.check()?;
        if start.elapsed().as_secs() > 120 {
            return Err("screenshot portal timed out awaiting permission".into());
        }
        if let std::task::Poll::Ready(result) =
            std::future::Future::poll(future.as_mut(), &mut context)
        {
            break result.map_err(|e| format!("Wayland screenshot portal: {e}"))?;
        }
        std::thread::park_timeout(std::time::Duration::from_millis(50));
    };
    let response = request.response().map_err(|e| e.to_string())?;
    cancel.check()?;
    let path = response
        .uri()
        .to_file_path()
        .map_err(|_| "portal returned a nonlocal screenshot URI")?;
    let meta = std::fs::metadata(&path).map_err(|e| e.to_string())?;
    if meta.len() > 16 * 1024 * 1024 {
        return Err("portal screenshot exceeds 16 MiB".into());
    }
    let bytes = std::fs::read(&path).map_err(|e| e.to_string())?;
    let mut decoder = png::Decoder::new(std::io::Cursor::new(bytes));
    decoder.set_transformations(png::Transformations::EXPAND | png::Transformations::STRIP_16);
    decoder.set_limits(png::Limits {
        bytes: 128 * 1024 * 1024,
    });
    let mut reader = decoder.read_info().map_err(|e| e.to_string())?;
    if reader.output_buffer_size() > 64 * 1024 * 1024 {
        return Err("portal image exceeds pixel budget".into());
    }
    let mut pixels = vec![0; reader.output_buffer_size()];
    let info = reader.next_frame(&mut pixels).map_err(|e| e.to_string())?;
    let pixels = &pixels[..info.buffer_size()];
    let rgba_pixels = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        _ => return Err("portal screenshot must be RGB/RGBA PNG".into()),
    };
    cancel.check()?;
    rgba(
        info.width,
        info.height,
        &rgba_pixels,
        json!({"kind":"desktop_capture","color":"portal-provided; physical display appearance unverified","permission":"user selected through XDG portal"}),
    )
}
