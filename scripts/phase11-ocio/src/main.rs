//! OCIO-generated GLSL through Fold's wgpu version, on a real Vulkan adapter.
//! Writes an inspectable CPU/GPU PPM pair and fails on nonfinite/tolerance errors.
use std::{borrow::Cow, path::Path, time::Instant};
use wgpu::util::DeviceExt;

fn pixels(value: &serde_json::Value) -> Vec<f32> {
    value
        .as_array()
        .unwrap()
        .iter()
        .flat_map(|p| {
            p.as_array()
                .unwrap()
                .iter()
                .map(|v| v.as_f64().unwrap() as f32)
        })
        .collect()
}
fn ppm(path: &Path, data: &[f32]) {
    let mut bytes = b"P6\n64 16\n255\n".to_vec();
    bytes.extend(data.chunks_exact(4).flat_map(|p| {
        p[..3]
            .iter()
            .map(|v| (v.clamp(0., 1.) * 255.).round() as u8)
    }));
    std::fs::write(path, bytes).unwrap();
}
fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("expected generated fixture.json");
    let path = Path::new(&path);
    let fixture: serde_json::Value = serde_json::from_slice(&std::fs::read(path).unwrap()).unwrap();
    let input = pixels(&fixture["pixels"]);
    let expected = pixels(&fixture["expected"]);
    assert_eq!(input.len(), 64 * 16 * 4);
    assert_eq!(expected.len(), input.len());
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter =
        pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions::default()))
            .unwrap();
    let info = adapter.get_info();
    assert!(
        info.name.contains("GTX 1070"),
        "reference adapter required: {info:?}"
    );
    let (device, queue) = pollster::block_on(adapter.request_device(&wgpu::DeviceDescriptor {
        required_features: wgpu::Features::FLOAT32_FILTERABLE,
        ..Default::default()
    }))
    .unwrap();
    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("OCIO GLSL"),
        source: wgpu::ShaderSource::Glsl {
            shader: Cow::Borrowed(fixture["shader"].as_str().unwrap()),
            stage: wgpu::naga::ShaderStage::Compute,
            defines: Default::default(),
        },
    });
    let pipeline = device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
        label: None,
        layout: None,
        module: &shader,
        entry_point: Some("main"),
        compilation_options: Default::default(),
        cache: None,
    });
    let upload_begin = Instant::now();
    let source = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
        label: None,
        contents: bytemuck::cast_slice(&input),
        usage: wgpu::BufferUsages::STORAGE,
    });
    let size = (input.len() * 4) as u64;
    let output = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
        mapped_at_creation: false,
    });
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: None,
        size,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    let mut textures = Vec::new();
    let mut samplers = Vec::new();
    for lut in fixture["textures"].as_array().unwrap() {
        let edge = lut["edge"].as_u64().unwrap() as u32;
        let extent = wgpu::Extent3d {
            width: edge,
            height: edge,
            depth_or_array_layers: edge,
        };
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: None,
            size: extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D3,
            format: wgpu::TextureFormat::Rgba32Float,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let rgb: Vec<f32> = lut["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_f64().unwrap() as f32)
            .collect();
        let rgba: Vec<f32> = rgb
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 1.])
            .collect();
        queue.write_texture(
            texture.as_image_copy(),
            bytemuck::cast_slice(&rgba),
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(edge * 16),
                rows_per_image: Some(edge),
            },
            extent,
        );
        textures.push(texture.create_view(&Default::default()));
        samplers.push(device.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        }));
    }
    let mut entries = vec![
        wgpu::BindGroupEntry {
            binding: 0,
            resource: source.as_entire_binding(),
        },
        wgpu::BindGroupEntry {
            binding: 1,
            resource: output.as_entire_binding(),
        },
    ];
    for (i, (texture, sampler)) in textures.iter().zip(&samplers).enumerate() {
        entries.push(wgpu::BindGroupEntry {
            binding: 2 + i as u32 * 2,
            resource: wgpu::BindingResource::TextureView(texture),
        });
        entries.push(wgpu::BindGroupEntry {
            binding: 3 + i as u32 * 2,
            resource: wgpu::BindingResource::Sampler(sampler),
        });
    }
    let bindings = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: None,
        layout: &pipeline.get_bind_group_layout(0),
        entries: &entries,
    });
    let upload_host_ms = upload_begin.elapsed().as_secs_f64() * 1000.;
    let begin = Instant::now();
    let mut encoder = device.create_command_encoder(&Default::default());
    {
        let mut pass = encoder.begin_compute_pass(&Default::default());
        pass.set_pipeline(&pipeline);
        pass.set_bind_group(0, &bindings, &[]);
        pass.dispatch_workgroups(16, 1, 1);
    }
    encoder.copy_buffer_to_buffer(&output, 0, &readback, 0, size);
    queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receive
        .recv_timeout(std::time::Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let dispatch_readback_ms = begin.elapsed().as_secs_f64() * 1000.;
    let mapped = readback.slice(..).get_mapped_range();
    let actual: &[f32] = bytemuck::cast_slice(&mapped);
    // Hardware trilinear weights have finite precision; allow < 1/4 SDR code
    // value for the supplemental LUT path, independently of the analytic view.
    let tolerance = if textures.is_empty() { 0.0002 } else { 0.001 };
    let mut max_error = 0f32;
    let mut worst = 0;
    for (i, (&a, &b)) in actual.iter().zip(&expected).enumerate() {
        assert!(a.is_finite() && b.is_finite(), "nonfinite output");
        if (a - b).abs() > max_error {
            max_error = (a - b).abs();
            worst = i;
        }
    }
    ppm(&path.with_extension("cpu.ppm"), &expected);
    ppm(&path.with_extension("gpu.ppm"), actual);
    println!(
        "{}",
        serde_json::json!({
            "adapter": info.name, "backend": format!("{:?}", info.backend), "driver": info.driver_info,
            "fixture": path, "pixels": input.len()/4, "max_absolute_error": max_error,
            "worst_input": &input[worst / 4 * 4..worst / 4 * 4 + 4],
            "worst_actual": actual[worst], "worst_expected": expected[worst],
            "tolerance": tolerance, "upload_host_ms": upload_host_ms,
            "dispatch_and_readback_ms": dispatch_readback_ms,
            "note": "host wall times, not GPU timestamps or native presentation"
        })
    );
    assert!(max_error <= tolerance, "CPU/GPU tolerance exceeded");
}
