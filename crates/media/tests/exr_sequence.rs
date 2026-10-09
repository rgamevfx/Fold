use exr::prelude::*;
use fold_media::{
    Cancel, exr as media,
    exr_sequence::{Options, Policy},
};
fn write(path: &std::path::Path, value: f32, multipart: bool, tiled: bool) {
    let encoding = Encoding {
        compression: Compression::ZIP16,
        blocks: if tiled {
            Blocks::Tiles(Vec2(8, 8))
        } else {
            Blocks::ScanLines
        },
        line_order: LineOrder::Increasing,
    };
    let layer = |name: &str, value: f32| {
        Layer::new(
            (16, 8),
            LayerAttributes::named(name),
            encoding,
            AnyChannels::sort(
                ["R", "G", "B", "A"]
                    .into_iter()
                    .map(|c| {
                        AnyChannel::new(
                            c,
                            FlatSamples::F32(vec![if c == "A" { 0.375 } else { value }; 128]),
                        )
                    })
                    .collect(),
            ),
        )
    };
    if multipart {
        Image::from_layers(
            ImageAttributes::new(IntegerBounds::from_dimensions((16, 8))),
            vec![layer("beauty", value), layer("diffuse", value + 2.)],
        )
        .write()
        .to_file(path)
        .unwrap();
    } else {
        Image::from_layer(layer("beauty", value))
            .write()
            .to_file(path)
            .unwrap();
    }
}
#[test]
fn multipart_scanline_and_tiled_parts_route_independently() {
    let dir = tempfile::tempdir().unwrap();
    for tiled in [false, true] {
        let path = dir.path().join("parts.exr");
        write(&path, -0.25, true, tiled);
        let source = media::Source {
            info: media::inspect(&path, &Cancel::default()).unwrap(),
            fingerprint: fold_media::fingerprint(&path, &Cancel::default()).unwrap(),
            path,
        };
        assert_eq!(source.info.channels.len(), 8);
        let pixels = media::decode(
            &source,
            [
                Some("diffuse.R"),
                Some("beauty.G"),
                Some("diffuse.R"),
                Some("beauty.A"),
            ],
            [16, 8],
            &Cancel::default(),
        )
        .unwrap();
        assert_eq!(pixels.pixels, vec![[1.75, -0.25, 1.75, 0.375]; 128]);
    }
}
#[test]
fn sequence_manifest_handles_numbering_holes_stills_and_replaced_bytes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shot.1001.exr");
    write(&path, 0.1, false, false);
    write(&dir.path().join("shot.1003.exr"), 0.3, false, true);
    write(&dir.path().join("unrelated.1002.exr"), 0.2, false, true);
    let linked = fold_media::ingest::inspect_linked(&path, &Cancel::default()).unwrap();
    assert!(linked.metadata.streams[0].facts.get("sequence").is_none());
    let fold_media::ingest::SourceProfile::Exr(info) = linked.metadata.profile else {
        panic!()
    };
    let source = media::Source {
        info,
        path: linked.path,
        fingerprint: linked.fingerprint,
    };
    let sequence = source.info.sequence.as_ref().unwrap();
    assert_eq!(
        (sequence.first(), sequence.last(), sequence.frames.len()),
        (1001, 1003, 2)
    );
    let mut options = Options::default();
    assert!(fold_media::exr_sequence::resolve(&source, 1002, &options, false).is_err());
    options.missing = Policy::Hold;
    assert_eq!(
        fold_media::exr_sequence::resolve(&source, 1002, &options, false)
            .unwrap()
            .unwrap()
            .path,
        path
    );
    options.missing = Policy::Black;
    assert!(
        fold_media::exr_sequence::resolve(&source, 1002, &options, false)
            .unwrap()
            .is_none()
    );
    assert!(
        fold_media::exr_sequence::resolve(&source, 0, &options, true)
            .unwrap()
            .is_none()
    );
    options.still = true;
    assert_eq!(
        fold_media::exr_sequence::resolve(&source, 1003, &options, false)
            .unwrap()
            .unwrap()
            .path,
        path
    );
    options.still = false;
    let selected = fold_media::exr_sequence::resolve(&source, 1003, &options, false)
        .unwrap()
        .unwrap();
    assert_eq!(
        media::decode(
            &selected,
            [Some("R"), None, None, Some("A")],
            [16, 8],
            &Cancel::default()
        )
        .unwrap()
        .pixels[0],
        [0.3, 0., 0., 0.375]
    );
    write(&selected.path, 0.9, false, true);
    assert!(
        media::decode(
            &selected,
            [Some("R"), None, None, None],
            [16, 8],
            &Cancel::default()
        )
        .unwrap_err()
        .message
        .contains("fingerprint")
    );
}

#[test]
fn dci_4k_named_channel_decode_fits_bounded_storage() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("4k.exr");
    let size = (4096, 2160);
    let channels = AnyChannels::sort(
        vec![AnyChannel::new(
            "depth.Z",
            FlatSamples::F32(vec![1234.5; size.0 * size.1]),
        )]
        .into_iter()
        .collect(),
    );
    Image::from_layer(Layer::new(
        size,
        LayerAttributes::default(),
        Encoding {
            compression: Compression::ZIP16,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        channels,
    ))
    .write()
    .non_parallel()
    .to_file(&path)
    .unwrap();
    let source = media::Source {
        info: media::inspect(&path, &Cancel::default()).unwrap(),
        fingerprint: fold_media::fingerprint(&path, &Cancel::default()).unwrap(),
        path,
    };
    let decoded = media::decode(
        &source,
        [Some("depth.Z"), None, None, None],
        [4096, 2160],
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(decoded.pixels.len(), 4096 * 2160);
    assert_eq!(decoded.pixels.last().unwrap(), &[1234.5, 0., 0., 1.]);
    assert!(
        fold_media::budget::decoded_usage().bytes <= fold_media::budget::decoded_usage().budget
    );
}

#[test]
fn missing_alpha_is_opaque_only_inside_the_data_window() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cropped.exr");
    let layer = Layer::new(
        (2, 2),
        LayerAttributes {
            layer_position: Vec2(1, 1),
            ..Default::default()
        },
        Encoding::SMALL_LOSSLESS,
        AnyChannels::sort(
            vec![AnyChannel::new("R", FlatSamples::F32(vec![0.25; 4]))]
                .into_iter()
                .collect(),
        ),
    );
    Image::new(
        ImageAttributes::new(IntegerBounds::from_dimensions((4, 4))),
        layer,
    )
    .write()
    .to_file(&path)
    .unwrap();
    let source = media::Source {
        info: media::inspect(&path, &Cancel::default()).unwrap(),
        fingerprint: fold_media::fingerprint(&path, &Cancel::default()).unwrap(),
        path,
    };
    let pixels = media::decode(
        &source,
        [Some("R"), None, None, None],
        [4, 4],
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(pixels.pixels[0], [0.; 4]);
    assert_eq!(pixels.pixels[5], [0.25, 0., 0., 1.]);
    assert_eq!(pixels.pixels[15], [0.; 4]);
}
