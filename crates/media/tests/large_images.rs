//! Explicit production-size qualification; kept out of quick unit-test runs.
use fold_media::{Cancel, ingest::SourceProfile};

#[test]
#[ignore = "allocates 8K images and writes a 400 MiB multilayer EXR"]
fn large_sources_keep_native_samples_and_exr_streams_with_bounded_memory() {
    let directory = tempfile::tempdir().unwrap();
    let cancel = Cancel::default();
    // A real >256 MiB 4K file, with selected data in its last AOV.
    let exr_path = directory.path().join("lighting.exr");
    {
        use exr::prelude::*;
        let channels = (0..12)
            .map(|i| {
                AnyChannel::new(
                    format!("aov{i}.Z").as_str(),
                    FlatSamples::F32(vec![i as f32 + 0.25; 4096 * 2160]),
                )
            })
            .collect();
        Image::from_layer(Layer::new(
            (4096, 2160),
            LayerAttributes::default(),
            Encoding {
                compression: Compression::Uncompressed,
                blocks: Blocks::ScanLines,
                line_order: LineOrder::Increasing,
            },
            AnyChannels::sort(channels),
        ))
        .write()
        .non_parallel()
        .to_file(&exr_path)
        .unwrap();
    }
    let file_bytes = std::fs::metadata(&exr_path).unwrap().len();
    assert!(file_bytes > 256 * 1024 * 1024);
    // The encoded file is larger than the available working allowance. Reading
    // selected compression blocks must still succeed without a whole-file copy.
    fold_media::budget::configure_image_memory(768 * 1024 * 1024);
    let pressure = fold_media::budget::reserve_working(300 * 1024 * 1024).unwrap();
    let imported = fold_media::ingest::inspect_linked(&exr_path, &cancel).unwrap();
    let SourceProfile::Exr(info) = imported.metadata.profile else {
        panic!()
    };
    assert_eq!(info.dimensions, [4096, 2160]);
    assert_eq!(info.channels.len(), 12);
    let source = fold_media::exr::Source {
        path: exr_path,
        fingerprint: imported.fingerprint,
        info,
    };
    let pixels = fold_media::exr::decode(
        &source,
        [Some("aov11.Z"), None, None, None],
        [4096, 2160],
        &cancel,
    )
    .unwrap();
    assert_eq!(pixels.pixels.len(), 4096 * 2160);
    assert!(pixels.pixels.iter().all(|p| *p == [11.25, 0., 0., 1.]));
    assert!(fold_media::budget::working_usage().peak < file_bytes);
    drop(pixels);
    drop(pressure);
    fold_media::budget::configure_image_memory(fold_media::budget::automatic_image_memory());

    // 6K RGB BMP exceeds the old 25 MiB source limit without compression tricks.
    let bmp = directory.path().join("six-k.bmp");
    image::RgbImage::from_pixel(6144, 4096, image::Rgb([17, 71, 193]))
        .save(&bmp)
        .unwrap();
    assert!(std::fs::metadata(&bmp).unwrap().len() > 25 * 1024 * 1024);
    let imported = fold_media::ingest::inspect_linked(&bmp, &cancel).unwrap();
    let (pixels, _) = fold_media::still::load(&bmp, &imported.fingerprint, &cancel).unwrap();
    assert_eq!(pixels.dimensions(), [6144, 4096]);
    assert_eq!(
        pixels.encoded_pixels().last().unwrap(),
        [17. / 255., 71. / 255., 193. / 255., 1.]
    );
    drop(pixels);

    // 8192 square, 16-bit RGBA. Keep a distinct last pixel to detect resizing.
    let png = directory.path().join("eight-k.png");
    {
        let mut image = image::ImageBuffer::from_pixel(
            8192,
            8192,
            image::Rgba([12345u16, 23456, 34567, 45678]),
        );
        image.put_pixel(8191, 8191, image::Rgba([45678u16, 34567, 23456, 12345]));
        image::DynamicImage::ImageRgba16(image).save(&png).unwrap();
    }
    let imported = fold_media::ingest::inspect_linked(&png, &cancel).unwrap();
    let SourceProfile::Raster(info) = imported.metadata.profile else {
        panic!()
    };
    assert_eq!(info.dimensions, [8192, 8192]);
    assert_eq!(info.bit_depth, 16);
    assert!(info.alpha);
    let (pixels, _) = fold_media::still::load(&png, &imported.fingerprint, &cancel).unwrap();
    assert_eq!(pixels.dimensions(), [8192, 8192]);
    assert_eq!(pixels.storage_bytes(), 8192 * 8192 * 16);
    let p = pixels.encoded_pixels().last().unwrap();
    assert!((p[0] / p[3] - 45678. / 65535.).abs() < 1e-6);
    assert!((p[3] - 12345. / 65535.).abs() < 1e-6);
    eprintln!(
        "EXR file={} MiB; CPU allowance={} MiB; decoded peak={} MiB; working peak={} MiB",
        file_bytes / 1048576,
        fold_media::budget::automatic_image_memory() / 1048576,
        fold_media::budget::decoded_usage().peak / 1048576,
        fold_media::budget::working_usage().peak / 1048576
    );
}
