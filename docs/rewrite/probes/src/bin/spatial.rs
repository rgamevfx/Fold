//! M4 executable experiment: Bevy's real 3D render graph on externally owned GPU
//! resources. This is not yet the Fold adapter or its memory/lifetime contract.
use bevy::{
    camera::{Exposure, ManualTextureViewHandle, RenderTarget},
    core_pipeline::tonemapping::{DebandDither, Tonemapping},
    prelude::*,
    render::{
        RenderApp, RenderPlugin,
        pipelined_rendering::PipelinedRenderingPlugin,
        renderer::{
            RenderAdapter, RenderAdapterInfo, RenderDevice, RenderInstance, RenderQueue,
            WgpuWrapper,
        },
        settings::RenderCreation,
        texture::{ManualTextureView, ManualTextureViews},
        view::{Hdr, ViewDepthTexture},
    },
    time::TimeUpdateStrategy,
};
use std::{sync::Arc, time::Duration};

fn main() {
    pollster::block_on(run());
}

async fn run() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter = instance.request_adapter(&Default::default()).await.unwrap();
    let info = adapter.get_info();
    println!("M4 Bevy spatial probe: {info:?}");
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            // Match the current Fold Host::headless device request.
            required_features: adapter.features()
                & (wgpu::Features::FLOAT32_FILTERABLE
                    | wgpu::Features::TIMESTAMP_QUERY
                    | wgpu::Features::TIMESTAMP_QUERY_INSIDE_ENCODERS
                    | wgpu::Features::TEXTURE_COMPRESSION_BC),
            ..Default::default()
        })
        .await
        .unwrap();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Fold externally owned spatial target"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba16Float,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let creation = RenderCreation::manual(
        RenderDevice::from(device.clone()),
        RenderQueue(Arc::new(WgpuWrapper::new(queue.clone()))),
        RenderAdapterInfo(WgpuWrapper::new(info)),
        RenderAdapter(Arc::new(WgpuWrapper::new(adapter))),
        RenderInstance(Arc::new(WgpuWrapper::new(instance))),
    );
    let mut app = App::new();
    app.add_plugins(
        DefaultPlugins
            .set(RenderPlugin {
                render_creation: creation,
                synchronous_pipeline_compilation: true,
                ..Default::default()
            })
            .set(WindowPlugin {
                primary_window: None,
                exit_condition: bevy::window::ExitCondition::DontExit,
                ..Default::default()
            })
            .disable::<PipelinedRenderingPlugin>(),
    );
    app.insert_resource(TimeUpdateStrategy::ManualDuration(Duration::ZERO));
    app.finish();
    app.cleanup();
    let handle = ManualTextureViewHandle(1);
    app.world_mut().resource_mut::<ManualTextureViews>().insert(
        handle,
        ManualTextureView {
            texture_view: target.create_view(&Default::default()).into(),
            size: UVec2::splat(64),
            view_format: wgpu::TextureFormat::Rgba16Float,
        },
    );
    let mesh = app
        .world_mut()
        .resource_mut::<Assets<Mesh>>()
        .add(Cuboid::new(1., 1., 1.));
    let red = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::linear_rgba(2., 0.25, 0.125, 1.),
            unlit: true,
            ..Default::default()
        });
    let blue = app
        .world_mut()
        .resource_mut::<Assets<StandardMaterial>>()
        .add(StandardMaterial {
            base_color: Color::linear_rgba(0.125, 0.25, 2., 1.),
            unlit: true,
            ..Default::default()
        });
    // Same retained geometry; the farther cube must lose the center depth test.
    let background = app
        .world_mut()
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(blue.clone()),
            Transform::from_xyz(0., 0., -1.),
        ))
        .id();
    let foreground = app
        .world_mut()
        .spawn((
            Mesh3d(mesh.clone()),
            MeshMaterial3d(red.clone()),
            Transform::default(),
        ))
        .id();
    app.world_mut().spawn((
        Camera3d {
            depth_texture_usages: (wgpu::TextureUsages::RENDER_ATTACHMENT
                | wgpu::TextureUsages::COPY_SRC)
                .into(),
            ..Default::default()
        },
        Camera {
            clear_color: ClearColorConfig::Custom(Color::NONE),
            ..Default::default()
        },
        Projection::Perspective(PerspectiveProjection {
            fov: std::f32::consts::FRAC_PI_4,
            near: 0.1,
            far: 100.,
            ..Default::default()
        }),
        RenderTarget::TextureView(handle),
        Hdr,
        Exposure { ev100: 8. },
        Msaa::Off,
        Tonemapping::None,
        DebandDither::Disabled,
        Transform::from_xyz(0., 0., 4.).looking_at(Vec3::ZERO, Vec3::Y),
    ));
    // Bound initial asset/pipeline readiness; record it rather than treating a
    // fixed arbitrary pre-roll count as acceptance of history-dependent effects.
    let mut warmup = None;
    for n in 1..=32 {
        app.update();
        let pixels = readback(&device, &queue, &target);
        if near(pixels[32 * 64 + 32], [2., 0.25, 0.125, 1.]) {
            warmup = Some(n);
            break;
        }
    }
    println!("initial asset/pipeline readiness updates: {warmup:?}");
    assert!(
        warmup.is_some(),
        "3D render graph never produced expected linear HDR center"
    );
    let mut near_depth = None;
    for sample in [0, 1, 0, -1, 1] {
        // Fold sample input controls placement; wall time and Bevy game time do not.
        app.world_mut()
            .entity_mut(foreground)
            .get_mut::<Transform>()
            .unwrap()
            .translation
            .x = if sample == 0 { 0. } else { 2. };
        app.update();
        let pixels = readback(&device, &queue, &target);
        let expected = if sample == 0 {
            [2., 0.25, 0.125, 1.]
        } else {
            [0.125, 0.25, 2., 1.]
        };
        assert!(
            near(pixels[32 * 64 + 32], expected),
            "sample={sample}, center={:?}",
            pixels[32 * 64 + 32]
        );
        assert_eq!(pixels[0], [0.; 4], "clear must preserve transparent black");
        let depth = {
            let world = app.sub_app_mut(RenderApp).world_mut();
            let mut query = world.query::<&ViewDepthTexture>();
            query.single(world).unwrap().texture.clone()
        };
        let depth_bytes = read_bytes(&device, &queue, &depth, 4, wgpu::TextureAspect::DepthOnly);
        let center = (32 * 64 + 32) * 4;
        let depth_value = f32::from_le_bytes(depth_bytes[center..center + 4].try_into().unwrap());
        assert!(depth_value.is_finite() && depth_value > 0. && depth_value <= 1.);
        let expected_depth = 0.1 / if sample == 0 { 3.5 } else { 4.5 };
        assert!((depth_value - expected_depth).abs() < 1e-6);
        if sample == 0 {
            if let Some(previous) = near_depth {
                assert_eq!(depth_value, previous);
            }
            near_depth = Some(depth_value);
        } else {
            assert!(
                depth_value < near_depth.unwrap(),
                "reverse-Z farther depth must decrease"
            );
        }
        assert_eq!(&depth_bytes[..4], &[0; 4], "reverse-Z clear depth");
        println!("sample={sample}: accessible depth={depth_value}");

        assert_eq!(
            app.world().get::<Mesh3d>(foreground).unwrap().0.id(),
            mesh.id()
        );
        assert_eq!(
            app.world()
                .get::<MeshMaterial3d<StandardMaterial>>(foreground)
                .unwrap()
                .0
                .id(),
            red.id()
        );
        assert_eq!(app.world().resource::<Time>().elapsed(), Duration::ZERO);
        println!(
            "sample={sample}: depth, linear HDR, alpha, retained handles, frozen clock pass; center={:?}",
            pixels[32 * 64 + 32]
        );
    }
    app.world_mut()
        .entity_mut(foreground)
        .get_mut::<Transform>()
        .unwrap()
        .translation
        .x = 0.;
    {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let material = materials.get_mut(&red).unwrap();
        material.base_color = Color::linear_rgba(2., 0.25, 0.125, 0.5);
        // StandardMaterial base_color is straight. Blend premultiplies at the
        // attachment boundary; Premultiplied would require premultiplied inputs.
        material.alpha_mode = AlphaMode::Blend;
    }
    settle(
        &mut app,
        &device,
        &queue,
        &target,
        [1.0625, 0.25, 1.0625, 1.],
        "premultiplied over opaque",
    );
    app.world_mut()
        .entity_mut(background)
        .insert(Visibility::Hidden);
    settle(
        &mut app,
        &device,
        &queue,
        &target,
        [1., 0.125, 0.0625, 0.5],
        "premultiplied over clear",
    );
    {
        let mut materials = app.world_mut().resource_mut::<Assets<StandardMaterial>>();
        let material = materials.get_mut(&red).unwrap();
        material.base_color = Color::linear_rgb(0.5, 0.5, 0.5);
        material.alpha_mode = AlphaMode::Opaque;
        material.unlit = false;
        material.perceptual_roughness = 1.;
        material.reflectance = 0.;
    }
    app.world_mut()
        .resource_mut::<GlobalAmbientLight>()
        .brightness = 0.;
    let light = app
        .world_mut()
        .spawn((
            DirectionalLight {
                illuminance: 0.,
                shadows_enabled: false,
                ..Default::default()
            },
            Transform::from_xyz(0., 0., 4.).looking_at(Vec3::ZERO, Vec3::Y),
        ))
        .id();
    settle(
        &mut app,
        &device,
        &queue,
        &target,
        [0., 0., 0., 1.],
        "zero light at EV100=8",
    );
    let mut radiance = vec![];
    for lux in [1000., 2000., 1000.] {
        app.world_mut()
            .entity_mut(light)
            .get_mut::<DirectionalLight>()
            .unwrap()
            .illuminance = lux;
        app.update();
        let center = readback(&device, &queue, &target)[32 * 64 + 32];
        assert!(center[0] > 0.01 && center[0].is_finite() && center[3] == 1.);
        assert!((center[0] - center[1]).abs() <= 0.002 && (center[0] - center[2]).abs() <= 0.002);
        println!("illuminance={lux} lux; exposure=EV100 8; linear center={center:?}");
        let lambert = lux * 0.5 / std::f32::consts::PI * 2f32.powf(-8.) / 1.2;
        assert!(
            (center[0] - lambert).abs() <= 0.002,
            "gray-card exposure calibration"
        );
        radiance.push(center[0]);
    }
    assert!(
        (radiance[1] / radiance[0] - 2.).abs() <= 0.005,
        "lighting must remain linear"
    );
    assert_eq!(radiance[0], radiance[2], "revisited illuminance must match");
    println!("Bevy 0.18.1 offscreen spatial graph, external device/queue/RGBA16F target: pass");
}

fn settle(
    app: &mut App,
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    expected: [f32; 4],
    label: &str,
) {
    let mut last = [0.; 4];
    for n in 1..=32 {
        app.update();
        last = readback(device, queue, target)[32 * 64 + 32];
        if near(last, expected) {
            println!("{label}: pass after {n} update(s); center={last:?}");
            return;
        }
    }
    panic!("{label}: expected {expected:?}, got {last:?}");
}

fn near(a: [f32; 4], b: [f32; 4]) -> bool {
    a.into_iter().zip(b).all(|(a, b)| (a - b).abs() <= 0.002)
}

fn readback(device: &wgpu::Device, queue: &wgpu::Queue, target: &wgpu::Texture) -> Vec<[f32; 4]> {
    read_bytes(device, queue, target, 8, wgpu::TextureAspect::All)
        .chunks_exact(8)
        .map(|bytes| {
            std::array::from_fn(|i| {
                half::f16::from_le_bytes(bytes[i * 2..i * 2 + 2].try_into().unwrap()).to_f32()
            })
        })
        .collect()
}

fn read_bytes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    target: &wgpu::Texture,
    pixel_bytes: u32,
    aspect: wgpu::TextureAspect,
) -> Vec<u8> {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("Spatial probe readback"),
        size: u64::from(64 * 64 * pixel_bytes),
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        wgpu::TexelCopyTextureInfo {
            aspect,
            ..target.as_image_copy()
        },
        wgpu::TexelCopyBufferInfo {
            buffer: &buffer,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(64 * pixel_bytes),
                rows_per_image: Some(64),
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let (send, receive) = std::sync::mpsc::channel();
    buffer
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            send.send(result).unwrap();
        });
    device
        .poll(wgpu::PollType::Wait {
            submission_index: None,
            timeout: Some(Duration::from_secs(30)),
        })
        .unwrap();
    receive
        .recv_timeout(Duration::from_secs(30))
        .unwrap()
        .unwrap();
    let mapped = buffer.slice(..).get_mapped_range();
    let pixels = mapped.to_vec();
    drop(mapped);
    buffer.unmap();
    pixels
}
