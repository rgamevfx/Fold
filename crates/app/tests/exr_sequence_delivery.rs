//! A real multilayer sequence, authored Shuffle, archive and encoded alpha movie.
use fold_app::{media_workflow, packages};
use fold_compositor::{ChannelMapping, ChannelSource, Composite, Node, Parameters as P, Source};
use fold_foundation::{DocumentId, Time};
use fold_media::Cancel;
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use std::{path::Path, process::Command};

fn fixture(path: &Path, index: usize, multipart: bool) {
    use exr::prelude::*;
    let alpha = [0., 0.375, 1.][index];
    let mut channels: Vec<_> = ["R", "G", "B", "A"]
        .into_iter()
        .map(|c| {
            AnyChannel::new(
                c,
                FlatSamples::F32(vec![if c == "A" { 1. } else { 0. }; 512]),
            )
        })
        .collect();
    let layer = |channels| {
        Layer::new(
            (32, 16),
            LayerAttributes::named("diffuse"),
            Encoding::SMALL_LOSSLESS,
            AnyChannels::sort(channels),
        )
    };
    let aov = |name: &str| {
        AnyChannel::new(
            name,
            FlatSamples::F32(vec![
                if name.ends_with('A') {
                    alpha
                } else {
                    (0.2 + index as f32 * 0.15) * alpha
                };
                512
            ]),
        )
    };
    if multipart {
        Image::from_layers(
            ImageAttributes::new(IntegerBounds::from_dimensions((32, 16))),
            vec![
                Layer::new(
                    (32, 16),
                    LayerAttributes::named("rgba"),
                    Encoding::SMALL_LOSSLESS,
                    AnyChannels::sort(channels.into_iter().collect()),
                ),
                layer(["R", "G", "B", "A"].into_iter().map(aov).collect()),
            ],
        )
        .write()
        .to_file(path)
        .unwrap();
    } else {
        channels.extend(
            ["diffuse.R", "diffuse.G", "diffuse.B", "diffuse.A"]
                .into_iter()
                .map(aov),
        );
        Image::from_layer(layer(channels.into_iter().collect()))
            .write()
            .to_file(path)
            .unwrap();
    }
}
fn build(path: &Path) -> (Project, DocumentRef, fold_foundation::ObjectId) {
    let info = fold_media::VideoInfo {
        width: 32,
        height: 16,
        rate: [24000, 1001],
        frames: 1010,
    };
    let read = Node::new(
        P::Read {
            source: Source::Unassigned { info: info.clone() },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: info.time(1010).unwrap(),
        },
        vec![],
    );
    let id = DocumentId::new();
    let read_id = read.id;
    let mut shuffle = Node::new(
        P::Shuffle {
            mappings: ["red", "green", "blue", "alpha"]
                .into_iter()
                .map(|c| ChannelMapping {
                    destination: format!("rgba.{c}").try_into().unwrap(),
                    source: ChannelSource::A(format!("diffuse.{c}").try_into().unwrap()),
                })
                .collect(),
        },
        vec![read.id],
    );
    shuffle.inputs.push(None);
    let output = Node::new(P::Output, vec![shuffle.id]);
    let graph = Composite {
        info,
        nodes: vec![read, shuffle, output.clone()],
        output: output.id,
        extensions: Default::default(),
    };
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(id).unwrap())],
        })
        .unwrap();
    let before = project.snapshot();
    let request = fold_platform::packages::CommandRequest {
        id: "fold.app.read-source".into(),
        base: before.revision(),
        arguments: serde_json::to_vec(
            &serde_json::json!({"document":id,"node":read_id,"path":path}),
        )
        .unwrap(),
    };
    let batch = packages::builtins()
        .stage(&before, &request, &Cancel::default())
        .unwrap();
    project.commit(batch).unwrap();
    let after = project.snapshot();
    project.undo().unwrap();
    assert_eq!(project.snapshot().state().assets, before.state().assets);
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
    project.redo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        after.state().documents
    );
    project
        .commit(media_workflow::select_output(&project.snapshot(), id).unwrap())
        .unwrap();
    (
        project,
        DocumentRef {
            document: id,
            output: "video".into(),
            extensions: Default::default(),
        },
        read_id,
    )
}
#[test]
fn multilayer_sequence_shuffle_save_reopen_and_prores_alpha_delivery() {
    let dir = tempfile::tempdir().unwrap();
    for multipart in [false, true] {
        let folder = dir
            .path()
            .join(if multipart { "multipart" } else { "single" });
        std::fs::create_dir(&folder).unwrap();
        for index in 0..3 {
            fixture(
                &folder.join(format!("shot.{:04}.exr", 1001 + index)),
                index,
                multipart,
            );
        }
        let (project, source, _) = build(&folder.join("shot.1001.exr"));
        let saved = folder.join("sequence.fold");
        fold_project::save(&project.snapshot(), &saved).unwrap();
        let project = fold_project::load(&saved, 16).unwrap();
        let info = media_workflow::output(&project.snapshot()).unwrap().1;
        // Nonsequential requests must select the same source frames as delivery.
        for index in [2, 0, 1] {
            let request = media_workflow::SceneRequest {
                source: source.clone(),
                time: info.time(1001 + index).unwrap(),
                dimensions: [32, 16],
                region: None,
                preview: None,
            };
            let frame = media_workflow::evaluate_scene(
                &project.snapshot(),
                &request,
                &mut Default::default(),
                &Cancel::default(),
            )
            .unwrap();
            let alpha = [0., 0.375, 1.][index as usize];
            let rgb = (0.2 + index as f32 * 0.15) * alpha;
            assert!(frame.pixels().iter().all(|p| *p == [rgb, rgb, rgb, alpha]));
            let mut renderer = fold_app::output::OutputRenderer::from_environment().unwrap();
            let rgba = renderer
                .evaluate_rgba16(&project.snapshot(), &request, &Cancel::default())
                .unwrap();
            assert_eq!(
                u16::from_le_bytes([rgba.bytes()[6], rgba.bytes()[7]]),
                (alpha * 65535.).round() as u16
            );
            let expected = if alpha == 0. {
                0.
            } else {
                1.055 * (rgb / alpha).powf(1. / 2.4) - 0.055
            };
            let actual = u16::from_le_bytes([rgba.bytes()[0], rgba.bytes()[1]]) as f32 / 65535.;
            assert!(
                (actual - expected).abs() < 0.001,
                "straight output {actual}, expected {expected}"
            );
        }
        let movie = folder.join("result.mov");
        let command = Command::new(env!("CARGO_BIN_EXE_fold-cli"))
            .arg("export-video")
            .arg(&saved)
            .arg(&movie)
            .args(["1001", "1004"])
            .output()
            .unwrap();
        assert!(
            command.status.success(),
            "{}",
            String::from_utf8_lossy(&command.stderr)
        );
        let probe = Command::new("ffprobe")
            .args(["-v", "error", "-show_streams", "-of", "json"])
            .arg(&movie)
            .output()
            .unwrap();
        let probe: serde_json::Value = serde_json::from_slice(&probe.stdout).unwrap();
        assert_eq!(probe["streams"][0]["profile"], "4444");
        assert_eq!(probe["streams"][0]["nb_frames"], "3");
        assert_eq!(probe["streams"][0]["r_frame_rate"], "24000/1001");
        let decoded = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&movie)
            .args([
                "-vf",
                "alphaextract,format=gray16le",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(decoded.status.success());
        assert_eq!(decoded.stdout.len(), 3072);
        for (index, alpha) in [0i32, 24576, 65535].into_iter().enumerate() {
            for p in decoded.stdout[index * 1024..(index + 1) * 1024].chunks_exact(2) {
                let value = i32::from(u16::from_le_bytes([p[0], p[1]]));
                assert!(
                    (value - alpha).abs() <= 128,
                    "alpha {value}, expected {alpha}"
                );
            }
        }
        // Compare decoded color in its tagged Rec.709 signal, independent of alpha.
        let rgb = Command::new("ffmpeg").args(["-v","error","-i"]).arg(&movie).args(["-vf","zscale=matrixin=709:transferin=709:primariesin=709:rangein=limited:matrix=gbr:transfer=709:primaries=709:range=full,format=gbrpf32le","-f","rawvideo","pipe:1"]).output().unwrap();
        assert!(
            rgb.status.success(),
            "{}",
            String::from_utf8_lossy(&rgb.stderr)
        );
        for index in 1..3 {
            let expected =
                1.09929682680944f64 * (0.2 + index as f64 * 0.15).powf(0.45) - 0.09929682680944;
            let offset = index * 512 * 12;
            let value = f32::from_le_bytes(rgb.stdout[offset..offset + 4].try_into().unwrap());
            assert!(
                (f64::from(value) - expected).abs() < 0.006,
                "color {value}, expected {expected}"
            );
        }
        // A later changed source must fail without publishing a partial movie.
        fixture(&folder.join("shot.1003.exr"), 0, multipart);
        let failed = folder.join("changed.mov");
        assert!(
            media_workflow::export(&project.snapshot(), &failed, 1001, 1004, &Cancel::default())
                .is_err()
        );
        assert!(!failed.exists());
    }
}

#[test]
fn sequence_offset_gap_policies_still_mode_and_relink_use_exact_frames() {
    use fold_media::exr_sequence::Policy;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("shot.1001.exr");
    fixture(&path, 1, false);
    fixture(&dir.path().join("shot.1003.exr"), 2, false);
    let (mut project, source, read_id) = build(&path);
    let mut graph =
        Composite::from_document(&project.snapshot().state().documents[&source.document]).unwrap();
    let P::Read { start, .. } = &mut graph.node_mut(read_id).unwrap().parameters else {
        panic!()
    };
    *start = Time::ZERO; // frame 1001 appears at composition frame 0.
    let commit = |project: &mut Project, graph: &Composite| {
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(
                    graph.document(source.document).unwrap(),
                )],
            })
            .unwrap();
    };
    commit(&mut project, &graph);
    let evaluation_info = graph.info.clone();
    let evaluate = |project: &Project, frame: u32| {
        let request = media_workflow::SceneRequest {
            source: source.clone(),
            time: evaluation_info.time(frame).unwrap(),
            dimensions: [32, 16],
            region: None,
            preview: None,
        };
        media_workflow::evaluate_scene(
            &project.snapshot(),
            &request,
            &mut Default::default(),
            &Cancel::default(),
        )
        .map(|f| f.pixels()[0])
    };
    assert!((evaluate(&project, 0).unwrap()[3] - 0.375).abs() < 0.001);
    assert!(evaluate(&project, 1).unwrap_err().contains("missing"));
    assert_eq!(evaluate(&project, 3).unwrap(), [0.; 4]);
    if let P::Read {
        source: Source::Exr { options, .. },
        ..
    } = &mut graph.node_mut(read_id).unwrap().parameters
    {
        options.missing = Policy::Hold;
        options.outside = Policy::Hold;
    }
    commit(&mut project, &graph);
    assert!((evaluate(&project, 1).unwrap()[3] - 0.375).abs() < 0.001);
    assert_eq!(evaluate(&project, 3).unwrap(), [0.5, 0.5, 0.5, 1.]);
    if let P::Read {
        source: Source::Exr { options, .. },
        ..
    } = &mut graph.node_mut(read_id).unwrap().parameters
    {
        options.still = true;
    }
    commit(&mut project, &graph);
    assert!((evaluate(&project, 2).unwrap()[3] - 0.375).abs() < 0.001);
    project.undo().unwrap();
    assert_eq!(evaluate(&project, 2).unwrap()[3], 1.);
    let replacement = dir.path().join("replacement");
    std::fs::create_dir(&replacement).unwrap();
    fixture(&replacement.join("new.1001.exr"), 2, false);
    fixture(&replacement.join("new.1003.exr"), 1, false);
    let asset = *project.snapshot().state().assets.keys().next().unwrap();
    let request = fold_app::ingest::RelinkRequest {
        base: project.snapshot().revision(),
        asset,
        path: replacement.join("new.1001.exr"),
    };
    fold_app::ingest::relink(&project.snapshot(), &request, &Cancel::default())
        .unwrap()
        .commit(&mut project)
        .unwrap();
    assert_eq!(evaluate(&project, 0).unwrap()[3], 1.);
    assert!((evaluate(&project, 2).unwrap()[3] - 0.375).abs() < 0.001);
    project.undo().unwrap();
    assert_eq!(evaluate(&project, 2).unwrap()[3], 1.);
}

#[test]
#[ignore = "requires packaged OCIO runtime; set FOLD_COLOR_ROOT"]
fn aces_output_transform_preserves_straight_alpha_before_and_after_encoding() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("still.exr");
    fixture(&path, 1, false);
    let (mut project, source, read) = build(&path);
    let mut settings = project.snapshot().state().settings.clone();
    settings.insert(
        fold_color::settings::PROJECT_KEY.into(),
        serde_json::to_value(fold_color::settings::ProjectColor::default()).unwrap(),
    );
    let mut graph =
        Composite::from_document(&project.snapshot().state().documents[&source.document]).unwrap();
    if let P::Read {
        source: Source::Exr { color_layers, .. },
        ..
    } = &mut graph.node_mut(read).unwrap().parameters
    {
        color_layers.push("diffuse".into());
    }
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::SetSettings(settings),
                Mutation::PutDocument(graph.document(source.document).unwrap()),
            ],
        })
        .unwrap();
    let request = media_workflow::SceneRequest {
        source: source.clone(),
        time: Time::ZERO,
        dimensions: [32, 16],
        region: None,
        preview: None,
    };
    for display in ["sRGB - Display", "Rec.1886 Rec.709 - Display"] {
        project
            .commit(
                fold_app::color::set_output(
                    &project.snapshot(),
                    source.document,
                    fold_platform::color::OutputTransform {
                        display: display.into(),
                        ..Default::default()
                    },
                )
                .unwrap(),
            )
            .unwrap();
        let scene = media_workflow::evaluate_scene(
            &project.snapshot(),
            &request,
            &mut Default::default(),
            &Cancel::default(),
        )
        .unwrap();
        let expected = fold_app::color::with_config(&project.snapshot(), |config| {
            let processor = config.unwrap().display(
                fold_color::WORKING_SPACE,
                &fold_platform::color::output(&project.snapshot(), source.document)?
                    .display_transform(),
            )?;
            let mut pixels = scene.pixels().to_vec();
            processor.apply(&mut pixels)?;
            Ok(pixels[0])
        })
        .unwrap();
        let mut renderer = fold_app::output::OutputRenderer::from_environment().unwrap();
        let output = renderer
            .evaluate_rgba16(&project.snapshot(), &request, &Cancel::default())
            .unwrap();
        for channel in 0..4 {
            let actual =
                u16::from_le_bytes([output.bytes()[channel * 2], output.bytes()[channel * 2 + 1]])
                    as f32
                    / 65535.;
            let wanted = if channel == 3 {
                expected[3]
            } else {
                expected[channel] / expected[3]
            }
            .clamp(0., 1.);
            assert!(
                (actual - wanted).abs() < 0.002,
                "channel {channel}: {actual} vs {wanted}"
            );
        }
        let movie = dir.path().join(format!("{display}.mov"));
        media_workflow::export(&project.snapshot(), &movie, 0, 1, &Cancel::default()).unwrap();
        let alpha = Command::new("ffmpeg")
            .args(["-v", "error", "-i"])
            .arg(&movie)
            .args([
                "-vf",
                "alphaextract,format=gray16le",
                "-f",
                "rawvideo",
                "pipe:1",
            ])
            .output()
            .unwrap();
        assert!(alpha.status.success());
        let actual = u16::from_le_bytes(alpha.stdout[..2].try_into().unwrap());
        assert!((i32::from(actual) - 24576).abs() <= 128);
    }
}
