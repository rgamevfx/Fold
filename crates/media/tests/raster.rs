use fold_media::{
    Cancel,
    ingest::{SourceProfile, inspect_linked},
    still,
};
use image::{DynamicImage, ImageFormat, Rgba, RgbaImage};

#[test]
fn common_formats_decode_thumbnail_and_validate_source_identity() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = Cancel::default();
    for (extension, format) in [
        ("JPG", ImageFormat::Jpeg),
        ("jpeg", ImageFormat::Jpeg),
        ("png", ImageFormat::Png),
        ("webp", ImageFormat::WebP),
        ("tif", ImageFormat::Tiff),
        ("tiff", ImageFormat::Tiff),
        ("gif", ImageFormat::Gif),
        ("bmp", ImageFormat::Bmp),
    ] {
        let path = dir.path().join(format!("image.{extension}"));
        let rgba = RgbaImage::from_pixel(4, 2, Rgba([128, 64, 32, 128]));
        let mut image = DynamicImage::ImageRgba8(rgba);
        if format == ImageFormat::Jpeg {
            image = DynamicImage::ImageRgb8(image.to_rgb8());
        }
        image.save_with_format(&path, format).unwrap();
        let linked = inspect_linked(&path, &cancel).unwrap();
        let SourceProfile::Raster(info) = &linked.metadata.profile else {
            panic!()
        };
        assert_eq!(info.dimensions, [4, 2]);
        let (decoded, _) = still::load(&path, &linked.fingerprint, &cancel).unwrap();
        let pixel = decoded.encoded_pixels().next().unwrap();
        let alpha = if info.alpha && format != ImageFormat::Gif {
            128. / 255.
        } else {
            1.
        };
        assert!((pixel[3] - alpha).abs() < 0.01, "{extension}: {pixel:?}");
        assert!(
            (pixel[0] / pixel[3] - 128. / 255.).abs() < 0.03,
            "{extension}: {pixel:?}"
        );
        let expected = ((128f32 / 255. + 0.055) / 1.055).powf(2.4) * alpha;
        assert!((decoded.linear_pixels().next().unwrap()[0] - expected).abs() < 0.02);
        assert!(decoded.still_identity().is_some());
        let thumb = fold_media::thumbnail::thumbnail(
            &path,
            &linked.fingerprint,
            &linked.metadata.profile,
            &cancel,
        )
        .unwrap();
        assert_eq!(thumb.len(), 128 * 72 * 4);
        assert!((thumb[(36 * 128 + 64) * 4 + 3] as f32 / 255. - alpha).abs() < 0.01);
        std::fs::write(&path, b"broken").unwrap();
        assert!(still::load(&path, &linked.fingerprint, &cancel).is_err());
    }
}

#[test]
fn sixteen_bit_png_and_tiff_preserve_samples_and_alpha() {
    let dir = tempfile::tempdir().unwrap();
    for ext in ["png", "tiff"] {
        let path = dir.path().join(format!("precision.{ext}"));
        let image = image::ImageBuffer::from_pixel(1, 1, Rgba([12345u16, 23456, 34567, 45678]));
        DynamicImage::ImageRgba16(image).save(&path).unwrap();
        let source = inspect_linked(&path, &Cancel::default()).unwrap();
        let SourceProfile::Raster(info) = source.metadata.profile else {
            panic!()
        };
        assert_eq!(info.bit_depth, 16);
        let (decoded, _) = still::load(&path, &source.fingerprint, &Cancel::default()).unwrap();
        let p = decoded.encoded_pixels().next().unwrap();
        for (actual, expected) in p[..3].iter().zip([12345., 23456., 34567.]) {
            assert!((actual / p[3] - expected / 65535.).abs() < 0.000001);
        }
        assert!((p[3] - 45678. / 65535.).abs() < 0.000001);
    }
}

#[test]
fn animated_gif_uses_first_frame_and_bad_images_fail() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("animated.gif");
    {
        let mut encoder =
            image::codecs::gif::GifEncoder::new(std::fs::File::create(&path).unwrap());
        for color in [[255, 0, 0, 255], [0, 255, 0, 255]] {
            encoder
                .encode_frame(image::Frame::new(RgbaImage::from_pixel(2, 2, Rgba(color))))
                .unwrap();
        }
    }
    let source = inspect_linked(&path, &Cancel::default()).unwrap();
    let (decoded, _) = still::load(&path, &source.fingerprint, &Cancel::default()).unwrap();
    assert_eq!(decoded.encoded_pixels().next().unwrap(), [1., 0., 0., 1.]);
    for ext in ["jpg", "png", "webp", "tiff", "gif", "bmp"] {
        let path = dir.path().join(format!("broken.{ext}"));
        std::fs::write(&path, b"invalid").unwrap();
        assert!(inspect_linked(&path, &Cancel::default()).is_err());
    }
    let cancel = Cancel::default();
    cancel.cancel();
    assert!(inspect_linked(&path, &cancel).is_err());
}

#[test]
fn jpeg_exif_orientation_is_applied_to_dimensions() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rotated.jpg");
    DynamicImage::ImageRgb8(image::RgbImage::from_pixel(3, 2, image::Rgb([255, 0, 0])))
        .save(&path)
        .unwrap();
    let original = std::fs::read(&path).unwrap();
    // APP1 Exif: little-endian TIFF, one SHORT Orientation=6 (90 degrees CW).
    let payload=b"Exif\0\0II\x2a\x00\x08\x00\x00\x00\x01\x00\x12\x01\x03\x00\x01\x00\x00\x00\x06\x00\x00\x00\x00\x00\x00\x00";
    let mut bytes = original[..2].to_vec();
    bytes.extend([0xff, 0xe1]);
    bytes.extend(((payload.len() + 2) as u16).to_be_bytes());
    bytes.extend(payload);
    bytes.extend(&original[2..]);
    std::fs::write(&path, bytes).unwrap();
    let source = inspect_linked(&path, &Cancel::default()).unwrap();
    let SourceProfile::Raster(info) = source.metadata.profile else {
        panic!()
    };
    assert_eq!(info.dimensions, [2, 3]);
}

#[test]
fn associated_tiff_alpha_is_unpremultiplied_before_color_conversion() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("associated.tiff");
    {
        let file = std::fs::File::create(&path).unwrap();
        let mut encoder = tiff::encoder::TiffEncoder::new(file).unwrap();
        let mut image = encoder
            .new_image::<tiff::encoder::colortype::RGBA8>(1, 1)
            .unwrap();
        image
            .encoder()
            .write_tag(tiff::tags::Tag::ExtraSamples, &[1u16][..])
            .unwrap();
        image.write_data(&[64, 32, 16, 128]).unwrap();
    }
    let source = inspect_linked(&path, &Cancel::default()).unwrap();
    let SourceProfile::Raster(info) = source.metadata.profile else {
        panic!()
    };
    assert!(info.premultiplied);
    let (decoded, _) = still::load(&path, &source.fingerprint, &Cancel::default()).unwrap();
    let p = decoded.encoded_pixels().next().unwrap();
    assert!((p[0] - 64. / 255.).abs() < 1e-6);
    let expected = ((0.5f32 + 0.055) / 1.055).powf(2.4) * (128. / 255.);
    assert!((decoded.linear_pixels().next().unwrap()[0] - expected).abs() < 1e-6);
}
