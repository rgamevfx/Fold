use exr::prelude::*;
use fold_media::{Cancel, exr as media};
fn fixture(path: &std::path::Path) {
    let mut channels: Vec<_> = ["R", "G", "B", "A"]
        .into_iter()
        .map(|name| {
            AnyChannel::new(
                name,
                FlatSamples::F32(vec![if name == "A" { 1. } else { -0.5 }; 6]),
            )
        })
        .collect();
    channels.extend((0..20).map(|i| {
        AnyChannel::new(
            format!("aov{i}.Z").as_str(),
            FlatSamples::F32(vec![1000. + i as f32; 6]),
        )
    }));
    Image::from_layer(Layer::new(
        (3, 2),
        LayerAttributes::default(),
        Encoding::SMALL_LOSSLESS,
        AnyChannels::sort(channels.into_iter().collect()),
    ))
    .write()
    .to_file(path)
    .unwrap();
}
#[test]
fn flat_twenty_layer_exr_preserves_depth_and_negative_values_selectively() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layers.exr");
    fixture(&path);
    let cancel = Cancel::default();
    let info = media::inspect(&path, &cancel).unwrap();
    assert_eq!(info.channels.len(), 24);
    assert_eq!(info.dimensions, [3, 2]);
    let source = media::Source {
        fingerprint: fold_media::fingerprint(&path, &cancel).unwrap(),
        path,
        info,
    };
    let decoded = media::decode(
        &source,
        [Some("aov19.Z"), Some("R"), None, None],
        [3, 2],
        &cancel,
    )
    .unwrap();
    assert_eq!(decoded.pixels, vec![[1019., -0.5, 0., 1.]; 6]);
    assert!(
        media::decode(
            &source,
            [Some("missing"), None, None, None],
            [3, 2],
            &cancel
        )
        .unwrap_err()
        .message
        .contains("Missing EXR channel")
    );
    let mut changed = source.clone();
    changed.fingerprint = "sha256:changed".into();
    assert!(
        media::decode(&changed, [None; 4], [3, 2], &cancel)
            .unwrap_err()
            .message
            .contains("fingerprint")
    );
    cancel.cancel();
    assert!(media::decode(&source, [None; 4], [3, 2], &cancel).is_err());
}
#[test]
fn linked_ingest_exposes_the_complete_channel_catalog() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("layers.exr");
    fixture(&path);
    let linked = fold_media::ingest::inspect_linked(&path, &Cancel::default()).unwrap();
    let fold_media::ingest::SourceProfile::Exr(info) = linked.metadata.profile else {
        panic!("EXR profile missing")
    };
    assert_eq!(info.channels.len(), 24);
    assert!(
        info.channels
            .iter()
            .any(|c| c.name == "aov19.Z" && !c.color)
    );
    assert!(
        info.channels
            .iter()
            .any(|c| c.name == "rgba.red" && c.color)
    );
}

#[test]
fn twenty_rgb_layers_decode_one_tuple_under_working_memory_pressure() {
    // Full source planes require 120 MiB, but selected tuples plus sequential
    // ZIP blocks fit into the 64 MiB left for the decoder.
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("lighting.exr");
    let dimensions = [1024, 512];
    let channels = (0..20)
        .flat_map(|layer| {
            ["R", "G", "B"].into_iter().map(move |component| {
                AnyChannel::new(
                    format!("lighting{layer}.{component}").as_str(),
                    FlatSamples::F32(vec![layer as f32; 1024 * 512]),
                )
            })
        })
        .collect();
    Image::from_layer(Layer::new(
        (1024, 512),
        LayerAttributes::default(),
        Encoding {
            compression: Compression::ZIP16,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        AnyChannels::sort(channels),
    ))
    .write()
    .non_parallel()
    .to_file(&path)
    .unwrap();
    let cancel = Cancel::default();
    let source = media::Source {
        fingerprint: fold_media::fingerprint(&path, &cancel).unwrap(),
        info: media::inspect(&path, &cancel).unwrap(),
        path,
    };
    let pressure = fold_media::budget::reserve_working(448 * 1024 * 1024).unwrap();
    let decoded = media::decode(
        &source,
        [
            Some("lighting19.R"),
            Some("lighting19.G"),
            Some("lighting19.B"),
            None,
        ],
        dimensions,
        &cancel,
    )
    .unwrap();
    assert_eq!(decoded.pixels.len(), 1024 * 512);
    assert!(
        decoded
            .pixels
            .iter()
            .all(|pixel| *pixel == [19., 19., 19., 1.])
    );
    drop(pressure);
}

#[test]
fn parallel_blocks_match_sequential_decoder_for_lossy_and_lossless_sources() {
    let dir = tempfile::tempdir().unwrap();
    for compression in [
        Compression::DWAA(Some(45.)),
        Compression::ZIP16,
        Compression::PIZ,
    ] {
        let path = dir.path().join("blocks.exr");
        Image::from_layer(Layer::new(
            (128, 96),
            LayerAttributes::default(),
            Encoding {
                compression,
                blocks: Blocks::ScanLines,
                line_order: LineOrder::Increasing,
            },
            SpecificChannels::rgba(|Vec2(x, y)| {
                (
                    f16::from_f32(x as f32 / 64. - 0.5),
                    f16::from_f32(y as f32 / 48.),
                    f16::from_f32((x + y) as f32 / 128.),
                    f16::from_f32((x % 5) as f32 / 4.),
                )
            }),
        ))
        .write()
        .non_parallel()
        .to_file(&path)
        .unwrap();
        let expected = read()
            .no_deep_data()
            .largest_resolution_level()
            .rgba_channels(
                |size, _| (size.width(), vec![[0f32; 4]; size.area()]),
                |pixels, pos, value: (f32, f32, f32, f32)| {
                    pixels.1[pos.y() * pixels.0 + pos.x()] = [value.0, value.1, value.2, value.3]
                },
            )
            .first_valid_layer()
            .all_attributes()
            .non_parallel()
            .pedantic()
            .from_file(&path)
            .unwrap();
        let cancel = Cancel::default();
        let source = media::Source {
            path: path.clone(),
            info: media::inspect(&path, &cancel).unwrap(),
            fingerprint: fold_media::fingerprint(&path, &cancel).unwrap(),
        };
        let actual = media::decode(
            &source,
            [Some("R"), Some("G"), Some("B"), Some("A")],
            [128, 96],
            &cancel,
        )
        .unwrap();
        assert_eq!(
            actual.pixels, expected.layer_data.channel_data.pixels.1,
            "{compression:?}"
        );
        let mapped = media::decode(
            &source,
            [Some("R"), Some("R"), None, None],
            [64, 48],
            &cancel,
        )
        .unwrap();
        for y in 0..48 {
            for x in 0..64 {
                let value = actual.pixels[y * 2 * 128 + x * 2][0];
                assert_eq!(mapped.pixels[y * 64 + x], [value, value, 0., 1.]);
            }
        }
    }
}

#[test]
fn shared_source_tickets_isolate_cancellation_and_reject_replaced_files() {
    use media::cache::{self, Request};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shared.exr");
    fixture(&path);
    let cancel = Cancel::default();
    let source = media::Source {
        info: media::inspect(&path, &cancel).unwrap(),
        fingerprint: fold_media::fingerprint(&path, &cancel).unwrap(),
        path: path.clone(),
    };
    let request = Request {
        source,
        channels: [
            Some("R".into()),
            Some("G".into()),
            Some("B".into()),
            Some("A".into()),
        ],
        dimensions: [3, 2],
    };
    let first = cache::request(request.clone(), false, &cancel).unwrap();
    let second = cache::request(request.clone(), true, &cancel).unwrap();
    drop(first);
    let held = second.wait(&cancel).unwrap();
    let again = cache::request(request.clone(), true, &cancel)
        .unwrap()
        .wait(&cancel)
        .unwrap();
    assert!(std::sync::Arc::ptr_eq(&held, &again));
    cache::clear();
    assert_eq!(held.pixels, vec![[-0.5, -0.5, -0.5, 1.]; 6]);
    // A cache hit cannot hide replacement, even when the old result is held.
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    *bytes.last_mut().unwrap() ^= 1;
    std::fs::write(&path, bytes).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    let changed = cache::request(request, true, &cancel)
        .unwrap()
        .wait(&cancel)
        .unwrap_err();
    assert!(changed.message.contains("fingerprint"), "{changed}");
    assert_eq!(held.pixels.len(), 6);
}
