#![cfg(all(feature = "native-video", target_os = "linux"))]
use fold_media::{
    Cancel, Decoder, inspect,
    native::{NativeDecoder, Nv12Layout},
};
use fold_native_video::{PendingBuffer, WriteComplete, WriteTicket};
use std::os::fd::AsFd;
fn device() -> (wgpu::Device, wgpu::Queue) {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor {
        backends: wgpu::Backends::VULKAN,
        ..Default::default()
    });
    let adapter = pollster::block_on(instance.request_adapter(&Default::default())).unwrap();
    eprintln!("native qualification adapter {:?}", adapter.get_info());
    pollster::block_on(adapter.request_device(&Default::default())).unwrap()
}
fn fixture() -> fold_media::VideoSource {
    inspect(
        std::path::Path::new(&std::env::var("FOLD_NATIVE_FIXTURE").unwrap()),
        &Cancel::default(),
    )
    .unwrap()
}
fn decode(
    device: &wgpu::Device,
    decoder: &mut NativeDecoder,
    source: &fold_media::VideoSource,
    frame: u32,
    cancel: &Cancel,
) -> Result<(PendingBuffer, WriteComplete), String> {
    let layout = Nv12Layout::for_source(source)?;
    let mut pending = PendingBuffer::new(device, layout.bytes)?;
    let complete = decoder.decode_into(
        source,
        source.info.time(frame)?,
        pending.write_ticket()?,
        cancel,
    )?;
    Ok((pending, complete))
}
fn native_bytes(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    decoder: &mut NativeDecoder,
    source: &fold_media::VideoSource,
    frame: u32,
) -> Vec<u8> {
    let layout = Nv12Layout::for_source(source).unwrap();
    let (pending, complete) = decode(device, decoder, source, frame, &Cancel::default()).unwrap();
    let ready = pending.submit(queue, complete, ()).unwrap();
    let mut encoder = device.create_command_encoder(&Default::default());
    let readback = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("explicit native qualification readback"),
        size: layout.bytes,
        usage: wgpu::BufferUsages::MAP_READ | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });
    encoder.copy_buffer_to_buffer(&ready.buffer, 0, &readback, 0, layout.bytes);
    queue.submit([encoder.finish()]);
    let (tx, rx) = std::sync::mpsc::channel();
    readback
        .slice(..)
        .map_async(wgpu::MapMode::Read, move |r| tx.send(r).unwrap());
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
    rx.recv().unwrap().unwrap();
    let bytes = readback.slice(..).get_mapped_range().to_vec();
    readback.unmap();
    bytes
}
fn compare(actual: &[u8], source: &fold_media::VideoSource, frame: u32, software: &mut Decoder) {
    let layout = Nv12Layout::for_source(source).unwrap();
    let expected = software
        .decode_native(source, source.info.time(frame).unwrap(), &Cancel::default())
        .unwrap();
    let planes = expected.planes();
    let [w, h] = [source.info.width as usize, source.info.height as usize];
    let mut max = 0u8;
    for y in 0..h {
        for x in 0..w {
            max =
                max.max(actual[y * layout.stride as usize + x].abs_diff(
                    expected.bytes()[planes[0].offset + y * planes[0].stride as usize + x],
                ));
        }
    }
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            for plane in 1..=2 {
                max = max.max(
                    actual[layout.uv_offset as usize + y * layout.stride as usize + x * 2 + plane
                        - 1]
                    .abs_diff(
                        expected.bytes()
                            [planes[plane].offset + y * planes[plane].stride as usize + x],
                    ),
                );
            }
        }
    }
    eprintln!("frame {frame}: max native/SW sample difference {max}");
    assert_eq!(max, 0);
}
#[test]
#[ignore = "requires NVIDIA Vulkan/CUDA, pinned LGPL helper and source fixture"]
fn nvdec_matches_software_seeks_and_source_pin() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("source.mp4");
    std::fs::copy(fixture().path, &path).unwrap();
    let source = inspect(&path, &Cancel::default()).unwrap();
    let (device, queue) = device();
    let mut decoder = NativeDecoder::new().unwrap();
    let mut software = Decoder::default();
    for frame in [
        0,
        1,
        5,
        source.info.frames - 1,
        2,
        source.info.frames / 2,
        0,
    ]
    .into_iter()
    .filter(|f| *f < source.info.frames)
    {
        compare(
            &native_bytes(&device, &queue, &mut decoder, &source, frame),
            &source,
            frame,
            &mut software,
        );
    }
    std::fs::write(&path, b"replaced source").unwrap();
    native_bytes(&device, &queue, &mut decoder, &source, 1);
    let mut fresh = NativeDecoder::new().unwrap();
    let error = decode(&device, &mut fresh, &source, 0, &Cancel::default())
        .err()
        .unwrap();
    assert!(error.contains("fingerprint"), "{error}");
    std::fs::remove_file(&path).unwrap();
    native_bytes(&device, &queue, &mut decoder, &source, 2);
    assert!(decode(&device, &mut fresh, &source, 0, &Cancel::default()).is_err());
    let cancelled = Cancel::default();
    cancelled.cancel();
    assert!(
        decode(&device, &mut decoder, &source, 0, &cancelled)
            .err()
            .unwrap()
            .contains("cancel")
    );
    drop(decoder);
    drop(software);
    drop(fresh);
    device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
}
#[test]
#[ignore = "requires native helper and source fixture"]
fn wrong_device_is_rejected_without_fallback() {
    let source = fixture();
    let layout = Nv12Layout::for_source(&source).unwrap();
    let file = std::fs::File::open("/dev/null").unwrap();
    let error = NativeDecoder::new()
        .unwrap()
        .decode_into(
            &source,
            source.info.time(0).unwrap(),
            WriteTicket::new(file.as_fd(), [0; 16], layout.bytes, layout.bytes),
            &Cancel::default(),
        )
        .unwrap_err();
    assert!(error.contains("UUID"), "{error}");
}
#[test]
#[ignore = "requires >=62 frame fixture and native GPU helper"]
fn two_same_source_ranges_and_two_distinct_sources_retain_cursors() {
    let source = fixture();
    assert!(source.info.frames >= 62);
    let (device, queue) = device();
    let mut decoder = NativeDecoder::new().unwrap();
    let mut software = Decoder::default();
    for frame in [0, 60, 1, 61] {
        compare(
            &native_bytes(&device, &queue, &mut decoder, &source, frame),
            &source,
            frame,
            &mut software,
        );
    }
    let stats = decoder.statistics();
    assert_eq!(stats.launches, 1);
    assert_eq!(stats.seeks, 2);
    assert_eq!(stats.forward_reuses, 2);
    assert_eq!(stats.requests, 4);
    assert!(stats.codec_call_nanoseconds > 0);
    assert!(stats.copy_ready_nanoseconds > 0);
    assert_eq!(
        stats.gpu_local_copy_bytes,
        Nv12Layout::for_source(&source).unwrap().bytes * 4
    );
    eprintln!("two retained same-source ranges {stats:?}");
    drop(decoder);
    let other = inspect(
        std::path::Path::new(&std::env::var("FOLD_NATIVE_SECOND_FIXTURE").unwrap()),
        &Cancel::default(),
    )
    .unwrap();
    let mut decoder = NativeDecoder::new().unwrap();
    for (source, frame) in [
        (&source, 0),
        (&other, 0),
        (&source, 60),
        (&other, 1),
        (&source, 61),
    ] {
        compare(
            &native_bytes(&device, &queue, &mut decoder, source, frame),
            source,
            frame,
            &mut software,
        );
    }
    let stats = decoder.statistics();
    assert_eq!(stats.launches, 2);
    assert_eq!(stats.seeks, 3);
    assert_eq!(stats.forward_reuses, 2);
    assert!(stats.codec_call_nanoseconds > 0);
    assert!(stats.copy_ready_nanoseconds > 0);
    assert_eq!(
        stats.gpu_local_copy_bytes,
        Nv12Layout::for_source(&source).unwrap().bytes * 3
            + Nv12Layout::for_source(&other).unwrap().bytes * 2
    );
    eprintln!("two retained distinct-source ranges {stats:?}");
}

#[test]
#[ignore = "reference GPU performance regression; two >=270-frame native fixtures"]
fn alternating_viewers_preserve_two_source_decode_locality() {
    let source = fixture();
    let other = inspect(
        std::path::Path::new(&std::env::var("FOLD_NATIVE_SECOND_FIXTURE").unwrap()),
        &Cancel::default(),
    )
    .unwrap();
    assert!(source.info.frames >= 270 && other.info.frames >= 270);
    let (device, queue) = device();
    let mut decoder = NativeDecoder::new().unwrap();
    let mut elapsed = std::time::Duration::ZERO;
    for offset in 0..12 {
        for frame in [offset, 240 + offset] {
            let begin = std::time::Instant::now();
            for video in [&source, &other] {
                let (pending, complete) =
                    decode(&device, &mut decoder, video, frame, &Cancel::default()).unwrap();
                let ready = pending.submit(&queue, complete, ()).unwrap();
                device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
                drop(ready);
            }
            if offset > 0 {
                elapsed += begin.elapsed();
            }
        }
    }
    let mean_ms = elapsed.as_secs_f64() * 1000. / 22.;
    eprintln!(
        "alternating two-source decode mean {mean_ms:.3} ms; {:?}",
        decoder.statistics()
    );
    assert!(
        mean_ms < 1000. / 30.,
        "decode alone exceeds the frame budget: {mean_ms:.3} ms"
    );
}
#[test]
#[ignore = "requires native GPU helper; receipt mismatch and immediate drop regression"]
fn receipt_is_allocation_bound_and_submit_retains_early_dropped_buffer() {
    let source = fixture();
    let (device, queue) = device();
    let mut decoder = NativeDecoder::new().unwrap();
    let (a, _complete_a) = decode(&device, &mut decoder, &source, 0, &Cancel::default()).unwrap();
    let (b, complete_b) = decode(&device, &mut decoder, &source, 1, &Cancel::default()).unwrap();
    assert!(
        a.submit(&queue, complete_b, ())
            .err()
            .unwrap()
            .contains("another allocation")
    );
    drop(b);
    // The retention object checks that ownership passes into the submission even
    // when no encoded consumer/bind group ever references the imported buffer.
    for frame in 0..8 {
        let (pending, complete) = decode(
            &device,
            &mut decoder,
            &source,
            frame % source.info.frames,
            &Cancel::default(),
        )
        .unwrap();
        let lease = std::sync::Arc::new(());
        let weak = std::sync::Arc::downgrade(&lease);
        let ready = pending.submit(&queue, complete, lease).unwrap();
        if frame % 2 == 1 {
            ready.buffer.destroy();
        }
        drop(ready);
        assert!(weak.upgrade().is_some());
        device.poll(wgpu::PollType::wait_indefinitely()).unwrap();
        assert!(weak.upgrade().is_none());
    }
}
