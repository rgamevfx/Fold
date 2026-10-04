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
        .contains("Missing EXR channel")
    );
    let mut changed = source.clone();
    changed.fingerprint = "sha256:changed".into();
    assert!(
        media::decode(&changed, [None; 4], [3, 2], &cancel)
            .unwrap_err()
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
