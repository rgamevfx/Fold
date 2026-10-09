//! Cross-package image invocation through the production provider registry.
use fold_composition::{
    Composition, Layer,
    reusable::{Binding, Definition, KEY, Target},
};
use fold_compositor::{Composite, Node, OperationConfig as P};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::effects::{Control, Instance, Interface};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use std::collections::BTreeMap;
fn reference(document: DocumentId) -> DocumentRef {
    DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    }
}
#[test]
fn composition_effect_shares_input_and_promotes_layer_controls() {
    let effect = DocumentId::new();
    let root = DocumentId::new();
    let mut composition = Composition::default();
    let layer = Layer::new("Source", None, composition.info.clone()).unwrap();
    let definition = Definition {
        extensions: Default::default(),
        interface: Interface {
            version: 1,
            images: vec!["source".into()],
            controls: vec![Control {
                id: "opacity".into(),
                label: "Opacity".into(),
                default: vec![1.],
                range: Some([0., 1.]),
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        },
        graphs: Default::default(),
        images: BTreeMap::from([(layer.id, "source".into())]),
        bindings: vec![Binding {
            extensions: Default::default(),
            control: "opacity".into(),
            layer: layer.id,
            target: Target::Opacity,
        }],
    };
    composition.layers.push(layer);
    composition
        .extensions
        .insert(KEY.into(), serde_json::to_value(definition).unwrap());
    let source = Node::new(
        P::Solid {
            rgba: [0.5, 0.2, 0.1, 0.5],
        },
        vec![],
    );
    let instance = Node::new(
        P::Instance {
            instance: Instance {
                id: ObjectId::new(),
                definition: reference(effect),
                version: 1,
                values: BTreeMap::from([("opacity".into(), vec![0.5])]),
                animation: Default::default(),
                enabled: true,
                extensions: Default::default(),
            },
            images: vec!["source".into()],
        },
        vec![source.id],
    );
    let output = Node::new(P::Output, vec![instance.id]);
    let composite = Composite {
        info: composition.info.clone(),
        output: output.id,
        nodes: vec![source, instance, output],
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(composition.document(effect).unwrap()),
                Mutation::PutDocument(composite.document(root).unwrap()),
            ],
        })
        .unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    fold_composition::package::register(&mut registry).unwrap();
    let graph = registry
        .video(
            &project.snapshot(),
            &reference(root),
            Time::new(1, 1).unwrap(),
            2,
            2,
        )
        .unwrap();
    // One upstream source plus the composition's transparent accumulator.
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|n| matches!(n, fold_render::ImageOp::Solid { .. }))
            .count(),
        2
    );
    let image = fold_render::render(graph).unwrap();
    for pixel in image.pixels() {
        assert_eq!(*pixel, [0.25, 0.1, 0.05, 0.25]);
    }
    assert_eq!(composition.layers[0].opacity, 1.);
}

#[test]
fn composition_routes_transition_images_and_duration_into_nested_compositor() {
    let dissolve = DocumentId::new();
    let effect = DocumentId::new();
    let root = DocumentId::new();
    let dissolve_doc =
        fold_compositor::presets::create(fold_compositor::presets::Preset::Dissolve, dissolve)
            .unwrap();
    let mut composition = Composition::default();
    let layer = Layer::new(
        "Dissolve graph",
        Some(fold_platform::image_source::Source::Output(reference(
            dissolve,
        ))),
        composition.info.clone(),
    )
    .unwrap();
    let definition = Definition {
        extensions: Default::default(),
        interface: Interface {
            version: 1,
            images: vec!["outgoing".into(), "incoming".into()],
            controls: vec![],
            extensions: Default::default(),
        },
        images: Default::default(),
        bindings: vec![],
        graphs: BTreeMap::from([(
            layer.id,
            fold_composition::reusable::Graph {
                extensions: Default::default(),
                version: 1,
                images: BTreeMap::from([
                    ("outgoing".into(), "outgoing".into()),
                    ("incoming".into(), "incoming".into()),
                ]),
                controls: Default::default(),
            },
        )]),
    };
    composition.layers.push(layer);
    composition
        .extensions
        .insert(KEY.into(), serde_json::to_value(definition).unwrap());
    let a = Node::new(
        P::Solid {
            rgba: [1., 0., 0., 1.],
        },
        vec![],
    );
    let b = Node::new(
        P::Solid {
            rgba: [0., 0., 1., 1.],
        },
        vec![],
    );
    let instance = Node::new(
        P::Instance {
            instance: Instance {
                id: ObjectId::new(),
                definition: reference(effect),
                version: 1,
                values: Default::default(),
                animation: Default::default(),
                enabled: true,
                extensions: Default::default(),
            },
            images: vec!["outgoing".into(), "incoming".into()],
        },
        vec![a.id, b.id],
    );
    let output = Node::new(P::Output, vec![instance.id]);
    let mut info = composition.info.clone();
    info.frames = 240;
    let host = Composite {
        info,
        output: output.id,
        nodes: vec![a, b, instance, output],
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(dissolve_doc),
                Mutation::PutDocument(composition.document(effect).unwrap()),
                Mutation::PutDocument(host.document(root).unwrap()),
            ],
        })
        .unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    fold_composition::package::register(&mut registry).unwrap();
    let graph = registry
        .video(
            &project.snapshot(),
            &reference(root),
            Time::new(9, 1).unwrap(),
            2,
            2,
        )
        .unwrap();
    let image = fold_render::render(graph).unwrap();
    for pixel in image.pixels() {
        assert!((pixel[0] - 0.1).abs() < 1e-6);
        assert!((pixel[2] - 0.9).abs() < 1e-6);
        assert_eq!(pixel[3], 1.);
    }
}

fn opacity_definition(id: DocumentId) -> fold_project::Document {
    let mut c = Composition::default();
    let layer = Layer::new("Input", None, c.info.clone()).unwrap();
    let d = Definition {
        extensions: Default::default(),
        interface: Interface {
            version: 1,
            images: vec!["source".into()],
            controls: vec![Control {
                id: "opacity".into(),
                label: "Opacity".into(),
                default: vec![0.5],
                range: Some([0., 1.]),
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        },
        images: BTreeMap::from([(layer.id, "source".into())]),
        graphs: Default::default(),
        bindings: vec![Binding {
            extensions: Default::default(),
            control: "opacity".into(),
            layer: layer.id,
            target: Target::Opacity,
        }],
    };
    c.layers.push(layer);
    c.extensions
        .insert(KEY.into(), serde_json::to_value(d).unwrap());
    c.document(id).unwrap()
}
fn effect(id: DocumentId) -> Instance {
    Instance {
        id: ObjectId::new(),
        definition: reference(id),
        version: 1,
        values: Default::default(),
        animation: Default::default(),
        enabled: true,
        extensions: Default::default(),
    }
}
fn solid_document(id: DocumentId, rgba: [f32; 4]) -> fold_project::Document {
    let input = Node::new(P::Solid { rgba }, vec![]);
    let output = Node::new(P::Output, vec![input.id]);
    Composite {
        info: Composition::default().info,
        output: output.id,
        nodes: vec![input, output],
        extensions: Default::default(),
    }
    .document(id)
    .unwrap()
}
#[test]
fn timeline_adjustment_only_processes_lower_tracks_in_its_active_range() {
    use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
    let source = DocumentId::new();
    let adjustment = DocumentId::new();
    let root = DocumentId::new();
    let bottom = Track::new(TrackKind::Video, "Bottom");
    let middle = Track::new(TrackKind::Video, "Adjustment");
    let top = Track::new(TrackKind::Video, "Top");
    let upper = DocumentId::new();
    let info = Composition::default().info;
    let clip = |track, document| Clip {
        id: ObjectId::new(),
        track,
        link: None,
        asset: fold_foundation::AssetId::new(),
        info: SourceMedia::Document {
            source: reference(document),
            info: info.clone(),
        },
        start: Time::ZERO,
        source_start: Time::ZERO,
        duration: Time::new(1, 1).unwrap(),
        level: 1.,
        effects: vec![],
        extensions: Default::default(),
    };
    let mut adj = clip(middle.id, source);
    adj.info = SourceMedia::Adjustment;
    adj.start = Time::new(1, 4).unwrap();
    adj.duration = Time::new(1, 2).unwrap();
    adj.effects.push(effect(adjustment));
    let sequence = Sequence {
        dimensions: [2, 2],
        rate: [24, 1],
        clips: vec![clip(bottom.id, source), adj, clip(top.id, upper)],
        tracks: vec![bottom, middle, top],
        transitions: vec![],
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                solid_document(source, [1., 0., 0., 1.]),
                solid_document(upper, [0., 0., 0.25, 0.25]),
                opacity_definition(adjustment),
                sequence.document(root).unwrap(),
            ]
            .into_iter()
            .map(Mutation::PutDocument)
            .collect(),
        })
        .unwrap();
    let registry = fold_app::packages::builtins();
    for (time, red) in [
        (Time::ZERO, 0.75),
        (Time::new(1, 2).unwrap(), 0.375),
        (Time::new(3, 4).unwrap(), 0.75),
    ] {
        let image = fold_render::render(
            registry
                .video(&project.snapshot(), &reference(root), time, 2, 2)
                .unwrap(),
        )
        .unwrap();
        for p in image.pixels() {
            assert_eq!(p[0], red);
            assert_eq!(p[2], 0.25);
        }
    }
}
#[test]
fn composition_adjustment_is_scoped_to_lower_siblings_inside_group() {
    let source = DocumentId::new();
    let adjustment = DocumentId::new();
    let root = DocumentId::new();
    let upper = DocumentId::new();
    let mut c = Composition::default();
    let mut group = Layer::new("Group", None, c.info.clone()).unwrap();
    group.opacity = 0.5;
    let mut below = Layer::new(
        "Source",
        Some(fold_platform::image_source::Source::Output(reference(
            source,
        ))),
        c.info.clone(),
    )
    .unwrap();
    below.parent = Some(group.id);
    let mut adj = Layer::new("Adjustment", None, c.info.clone()).unwrap();
    adj.parent = Some(group.id);
    adj.adjustment = true;
    adj.effects.push(effect(adjustment));
    let outside = Layer::new(
        "Outside",
        Some(fold_platform::image_source::Source::Output(reference(
            upper,
        ))),
        c.info.clone(),
    )
    .unwrap();
    c.layers = vec![outside, group, adj, below];
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                solid_document(source, [1., 0., 0., 1.]),
                solid_document(upper, [0., 0., 0.25, 0.25]),
                opacity_definition(adjustment),
                c.document(root).unwrap(),
            ]
            .into_iter()
            .map(Mutation::PutDocument)
            .collect(),
        })
        .unwrap();
    let graph = fold_app::packages::builtins()
        .video(&project.snapshot(), &reference(root), Time::ZERO, 2, 2)
        .unwrap();
    for p in fold_render::render(graph).unwrap().pixels() {
        assert_eq!(*p, [0.1875, 0., 0.25, 0.4375]);
    }
}

#[test]
#[cfg(feature = "gpu")]
#[ignore = "requires native GPU; format and pipeline reuse qualification"]
fn circular_wipe_is_aspect_correct_across_independent_formats() {
    let effect_id = DocumentId::new();
    let definition =
        fold_compositor::presets::create(fold_compositor::presets::Preset::CircularWipe, effect_id)
            .unwrap();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(definition)],
        })
        .unwrap();
    let registry = fold_app::packages::builtins();
    let (host, info) =
        pollster::block_on(fold_render::gpu::Host::headless(512 * 1024 * 1024)).unwrap();
    eprintln!("Wipe adapter: {info:?}");
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let mut decoder = fold_media::Decoder::default();
    let cancel = fold_media::Cancel::default();
    let mut first = true;
    for (dimensions, raster) in [
        ([1080, 1920], [27, 48]),
        ([3840, 2160], [96, 54]),
        ([1024, 1024], [32, 32]),
        ([1080, 1920], [54, 96]),
    ] {
        for center in [[0.5, 0.5], [0.2, 0.7]] {
            let id = DocumentId::new();
            let a = Node::new(
                P::Solid {
                    rgba: [1., 0., 0., 1.],
                },
                vec![],
            );
            let b = Node::new(
                P::Solid {
                    rgba: [0., 0., 1., 1.],
                },
                vec![],
            );
            let mut instance = effect(effect_id);
            instance.values.insert("center".into(), center.to_vec());
            let mix = Node::new(
                P::Instance {
                    instance,
                    images: vec!["outgoing".into(), "incoming".into()],
                },
                vec![a.id, b.id],
            );
            let output = Node::new(P::Output, vec![mix.id]);
            let c = Composite {
                info: fold_media::VideoInfo {
                    width: dimensions[0],
                    height: dimensions[1],
                    rate: [24, 1],
                    frames: 24,
                },
                output: output.id,
                nodes: vec![a, b, mix, output],
                extensions: Default::default(),
            };
            project
                .commit(EditBatch {
                    base: project.snapshot().revision(),
                    mutations: vec![Mutation::PutDocument(c.document(id).unwrap())],
                })
                .unwrap();
            for (num, den) in [(0, 1), (1, 4), (1, 2), (999, 1000)] {
                let time = Time::new(num, den).unwrap();
                let graph = registry
                    .video(
                        &project.snapshot(),
                        &reference(id),
                        time,
                        raster[0],
                        raster[1],
                    )
                    .unwrap();
                let mut gpu = renderer
                    .evaluate(graph, None, &mut decoder, &cancel)
                    .unwrap();
                assert_eq!(gpu.statistics.shader_compilations, u32::from(first));
                first = false;
                let image = gpu.readback(&cancel).unwrap();
                let size = dimensions.map(f64::from);
                let c = [center[0] * size[0], center[1] * size[1]];
                let radius = c[0].max(size[0] - c[0]).hypot(c[1].max(size[1] - c[1])) * num as f64
                    / den as f64;
                for (i, p) in image.pixels().iter().enumerate() {
                    let x = (i as u32 % raster[0]) as f64 + 0.5;
                    let y = (i as u32 / raster[0]) as f64 + 0.5;
                    let distance = (x / raster[0] as f64 * size[0] - c[0])
                        .hypot(y / raster[1] as f64 * size[1] - c[1]);
                    let t = (distance - (radius - 0.5)).clamp(0., 1.);
                    let mask = if num == 0 {
                        0.
                    } else {
                        1. - t * t * (3. - 2. * t)
                    };
                    assert!(
                        (p[0] as f64 - (1. - mask)).abs() < 0.002,
                        "{dimensions:?} {center:?} {time:?} pixel {i}: {p:?}"
                    );
                    assert!((p[2] as f64 - mask).abs() < 0.002);
                    assert_eq!(p[3], 1.);
                }
            }
        }
    }
}
