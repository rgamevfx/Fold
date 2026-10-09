//! Bounded dependency experiment, not the application renderer or acceptance suite.
use bevy_ecs::prelude::*;
use bevy_render::renderer::RenderDevice;
use vello::{
    kurbo::{Affine, Rect},
    peniko::{Color, Fill},
};

#[derive(Component)]
struct SampleTime(i64, u64);

fn main() {
    let mut world = World::new();
    let entity = world.spawn(SampleTime(0, 1)).id();
    for numerator in [0, 3, 1, -1, 3] {
        *world.get_mut::<SampleTime>(entity).unwrap() = SampleTime(numerator, 24);
        let time = world.get::<SampleTime>(entity).unwrap();
        assert_eq!((time.0, time.1), (numerator, 24));
    }
    println!("retained ECS identity and explicit rational sample input: pass");
    // The packaged font prevents system-font substitution in the experiment.
    let mut fonts = parley::FontContext::new();
    let bytes = std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../../resources/fonts/NotoSans-Regular.ttf"
    ))
    .unwrap();
    fonts.collection.register_fonts(bytes.into(), None);
    let mut context = parley::LayoutContext::<()>::new();
    let text = "MAKE IT MOVE O B 8";
    let mut builder = context.ranged_builder(&mut fonts, text, 1.0, true);
    builder.push_default(parley::StyleProperty::FontSize(32.0));
    builder.push_default(parley::StyleProperty::FontStack("Noto Sans".into()));
    let mut layout = builder.build(text);
    layout.break_all_lines(None);
    assert!(layout.width() > 0.0);
    let glyphs: usize = layout
        .lines()
        .flat_map(|line| line.items())
        .map(|item| match item {
            parley::PositionedLayoutItem::GlyphRun(run) => run.glyphs().count(),
            _ => 0,
        })
        .sum();
    assert!(glyphs > 0);
    println!(
        "packaged font layout: {glyphs} glyphs, width {}",
        layout.width()
    );
    pollster::block_on(gpu());
}

async fn gpu() {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions::default())
        .await
        .unwrap();
    println!("adapter: {:?}", adapter.get_info());
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor::default())
        .await
        .unwrap();
    let bevy_device = RenderDevice::from(device.clone());
    let mut renderer = vello::Renderer::new(
        bevy_device.wgpu_device(),
        vello::RendererOptions {
            use_cpu: false,
            pipeline_cache: None,
            antialiasing_support: vello::AaSupport::area_only(),
            num_init_threads: std::num::NonZeroUsize::new(1),
        },
    )
    .unwrap();
    let target = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("rewrite probe"),
        size: wgpu::Extent3d {
            width: 64,
            height: 64,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8Unorm,
        usage: wgpu::TextureUsages::STORAGE_BINDING | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut scene = vello::Scene::new();
    scene.fill(
        Fill::NonZero,
        Affine::IDENTITY,
        Color::from_rgb8(255, 0, 0),
        None,
        &Rect::new(8.0, 8.0, 56.0, 56.0),
    );
    renderer
        .render_to_texture(
            &device,
            &queue,
            &scene,
            &target.create_view(&Default::default()),
            &vello::RenderParams {
                base_color: Color::TRANSPARENT,
                width: 64,
                height: 64,
                antialiasing_method: vello::AaConfig::Area,
            },
        )
        .unwrap();
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("readback"),
        size: 64 * 256,
        usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
        mapped_at_creation: false,
    });
    let mut encoder = device.create_command_encoder(&Default::default());
    encoder.copy_texture_to_buffer(
        target.as_image_copy(),
        wgpu::TexelCopyBufferInfo {
            buffer: &readback,
            layout: wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(256),
                rows_per_image: None,
            },
        },
        target.size(),
    );
    queue.submit([encoder.finish()]);
    let (sender, receiver) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |result| {
            sender.send(result).unwrap()
        });
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    receiver.recv().unwrap().unwrap();
    let pixels = readback.slice(..).get_mapped_range();
    assert_eq!(
        &pixels[32 * 256 + 32 * 4..32 * 256 + 32 * 4 + 4],
        &[255, 0, 0, 255]
    );
    assert_eq!(&pixels[..4], &[0, 0, 0, 0]);
    drop(pixels);
    readback.unmap();
    println!("shared wgpu/Bevy device + Vello submission/completion/readback: pass");
}
