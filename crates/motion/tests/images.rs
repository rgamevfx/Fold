use fold_foundation::{AssetId, DocumentId, Time};
use fold_motion::{
    Motion,
    evaluation::{Evaluator, Value},
    fields::Datum,
    geometry::{Content, Geometry},
    graph::{Link, Node},
    nodes::image::{Entry, Settings},
};
use fold_platform::image_source::{Outside, Request, Source};
use fold_render::{ImageOp, RenderGraph};
fn setup(kind: &str) -> (Motion, fold_foundation::ObjectId, fold_foundation::ObjectId) {
    let mut m = Motion::empty();
    m.info.width = 128;
    m.info.height = 96;
    let mut source = Node::new("fold.motion.image").unwrap();
    source.settings = serde_json::to_value(Settings {
        sources: vec![Entry {
            source: Source::Asset(AssetId::new()),
            info: m.info.clone(),
            offset: Time::ZERO,
            extensions: Default::default(),
        }],
        ..Default::default()
    })
    .unwrap();
    let mut node = Node::new(kind).unwrap();
    node.connect("image", source.id, "image");
    node.set("size", Datum::Vector([128., 96.]));
    let ids = (source.id, node.id);
    m.graph.nodes.push(source);
    m.graph.nodes.push(node);
    m.graph.output = Link {
        node: ids.1,
        socket: "content".into(),
    };
    (m, ids.0, ids.1)
}
fn evaluate(m: &Motion) -> Content {
    let Value::Content(c) = Evaluator::new(m, Time::ZERO).unwrap().scene().unwrap() else {
        panic!()
    };
    c
}
fn compile(m: &Motion) -> RenderGraph {
    let mut c = evaluate(m).as_ref().clone();
    for e in &mut c {
        e.transform[4] += 64.;
        e.transform[5] += 48.;
    }
    fold_motion::geometry::render::compile_images(
        &std::sync::Arc::new(c),
        128,
        96,
        [1.; 2],
        &Default::default(),
        &|_| {
            Ok(RenderGraph {
                width: 128,
                height: 96,
                nodes: vec![ImageOp::Solid { rgba: [1.; 4] }],
                output: 0,
            })
        },
    )
    .unwrap()
}
#[test]
fn image_dependencies_round_trip_and_reject_tampered_envelopes() {
    let (mut m, source, _) = setup("fold.motion.image_card");
    let mut settings = m
        .graph
        .node(source)
        .unwrap()
        .settings::<Settings>()
        .unwrap();
    let reference = fold_project::DocumentRef {
        document: DocumentId::new(),
        output: "video".into(),
        extensions: Default::default(),
    };
    settings.sources.push(Entry {
        source: Source::Output(reference.clone()),
        info: m.info.clone(),
        offset: Time::ZERO,
        extensions: Default::default(),
    });
    fold_motion::authoring::node(&mut m, source)
        .unwrap()
        .settings = serde_json::to_value(settings).unwrap();
    let mut d = m.document(DocumentId::new()).unwrap();
    assert_eq!(d.assets.len(), 1);
    assert_eq!(d.dependencies, vec![reference]);
    assert_eq!(Motion::from_document(&d).unwrap(), m);
    d.assets.clear();
    assert!(Motion::from_document(&d).is_err());
}
#[test]
fn grid_uvs_survive_deformation_and_cell_mapping_is_explicit() {
    let (mut m, _, node) = setup("fold.motion.image_grid");
    fold_motion::authoring::node(&mut m, node)
        .unwrap()
        .set("offset", Datum::Vector([7., 13.]));
    let c = evaluate(&m);
    let Geometry::Image(image) = &c[0].geometry else {
        panic!()
    };
    let v = image.draw.mesh.vertices[0];
    assert_eq!(v.uv, [0., 0.]);
    assert_eq!(v.position, [-57., -35.]);
    assert_eq!(image.draw.mesh.vertices[1].uv, [1. / 16., 0.]);
    fold_motion::authoring::node(&mut m, node)
        .unwrap()
        .set("per_cell", Datum::Bool(true));
    let c = evaluate(&m);
    let Geometry::Image(image) = &c[0].geometry else {
        panic!()
    };
    assert_eq!(image.draw.mesh.vertices[1].uv, [1., 0.]);
}
#[test]
fn repeated_cards_resolve_one_source_and_batch_one_mesh() {
    let (mut m, _, cards) = setup("fold.motion.image_cards");
    let mut points = Node::new("fold.motion.grid_points").unwrap();
    points.set("columns", Datum::Scalar(32.));
    points.set("rows", Datum::Scalar(16.));
    fold_motion::authoring::node(&mut m, cards)
        .unwrap()
        .connect("points", points.id, "points");
    m.graph.nodes.push(points);
    let graph = compile(&m);
    let draws = graph
        .nodes
        .iter()
        .filter_map(|op| {
            if let ImageOp::Textured { draw, .. } = op {
                Some(draw)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].instances.len(), 512);
    assert_eq!(graph.nodes.len(), 2);
}
#[test]
fn ascii_builds_one_atlas_and_a_deferred_luminance_response() {
    let (mut m, _, node) = setup("fold.motion.image_ascii");
    let font = fold_foundation::ObjectId::new();
    m.fonts.insert(font, fold_motion::document::Font::builtin());
    fold_motion::authoring::node(&mut m, node).unwrap().settings = serde_json::json!({"font":font});
    let graph = compile(&m);
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|v| matches!(v, ImageOp::Vector(_)))
            .count(),
        1
    );
    let ImageOp::Textured { draw, .. } = &graph.nodes[graph.output] else {
        panic!()
    };
    assert_eq!(draw.mesh.vertices.len(), 80 * 30 * 4);
    assert_eq!(draw.atlas[0], 10.);
    let frame = fold_render::render(graph).unwrap();
    assert!(frame.pixels().iter().any(|p| p[3] > 0.));
    assert!(frame.pixels().iter().any(|p| p[3] == 0.));
}
#[test]
fn sampling_is_deferred_and_cannot_silently_enter_cpu_fields() {
    let (mut m, source, grid) = setup("fold.motion.image_grid");
    let mut sample = Node::new("fold.motion.sample_image").unwrap();
    sample.connect("image", source, "image");
    let mut response = Node::new("fold.motion.image_response").unwrap();
    response.connect("content", grid, "content");
    response.connect("sample", sample.id, "sample");
    response.set("displacement", Datum::Vector([10., 0.]));
    let output = response.id;
    m.graph.nodes.extend([sample.clone(), response]);
    m.graph.output = Link {
        node: output,
        socket: "content".into(),
    };
    let graph = compile(&m);
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|v| matches!(v, ImageOp::Solid { .. }))
            .count(),
        1
    );
    let ImageOp::Textured { draw, .. } = &graph.nodes[graph.output] else {
        panic!()
    };
    assert_eq!(draw.displacement, [10., 0.]);
    let mut math = Node::new("fold.motion.math").unwrap();
    math.connect("a", sample.id, "sample");
    m.graph.nodes.push(math);
    assert!(m.validate().is_err());
}
#[test]
fn exact_source_time_policies_include_negative_loop_and_boundaries() {
    let mut r = Request {
        bake: None,
        source: Source::Asset(AssetId::new()),
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24000, 1001],
            frames: 24,
        },
        time: Time::new(-1001, 24000).unwrap(),
        outside: Outside::Loop,
        color_space: None,
    };
    assert_eq!(r.mapped_time().unwrap(), Some(r.info.time(23).unwrap()));
    r.outside = Outside::Hold;
    assert_eq!(r.mapped_time().unwrap(), Some(Time::ZERO));
    r.outside = Outside::Black;
    assert_eq!(r.mapped_time().unwrap(), None);
    r.outside = Outside::Error;
    assert!(r.mapped_time().is_err());
    r.time = r.info.time(24).unwrap();
    r.outside = Outside::Loop;
    assert_eq!(r.mapped_time().unwrap(), Some(Time::ZERO));
}
#[test]
fn placement_is_undoable_and_rejects_document_cycles() {
    use fold_platform::browser::{Placement, PlacementSource};
    use fold_project::{Asset, EditBatch, Mutation, Project};
    let mut project = Project::new(8);
    let id = DocumentId::new();
    let asset = AssetId::new();
    let m = Motion::new_scene();
    let metadata = fold_media::ingest::SourceMetadata {
        profile: fold_media::ingest::SourceProfile::Ppm {
            dimensions: [32, 24],
        },
        streams: vec![],
        raw_format: serde_json::Value::Null,
        timing_policy: "still".into(),
    };
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutAsset(Asset {
                    id: asset,
                    location: "/tmp/phase49-fixture.ppm".into(),
                    fingerprint: "sha256:test".into(),
                    extensions: std::collections::BTreeMap::from([(
                        fold_media::ingest::METADATA_KEY.into(),
                        serde_json::to_value(metadata).unwrap(),
                    )]),
                }),
                Mutation::PutDocument(m.document(id).unwrap()),
            ],
        })
        .unwrap();
    let old = project.snapshot().state().documents[&id].clone();
    let placed = fold_motion::nodes::image::place(
        &project.snapshot(),
        &Placement {
            graph: None,
            source: PlacementSource::Asset(asset),
            target: id,
            track: None,
            at: Time::ZERO,
            source_start: Time::ZERO,
            duration: None,
            audio_only: false,
            new_track: false,
            mode: Default::default(),
        },
        None,
    )
    .unwrap();
    assert_eq!(placed.assets, vec![asset]);
    let m = Motion::from_document(&placed).unwrap();
    assert!(
        compile(&m)
            .nodes
            .iter()
            .any(|v| matches!(v, ImageOp::Textured { .. }))
    );
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(placed)],
        })
        .unwrap();
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents[&id].payload,
        old.payload
    );
    project.redo().unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    let request = Placement {
        graph: None,
        source: PlacementSource::Output(fold_project::DocumentRef {
            document: id,
            output: "video".into(),
            extensions: Default::default(),
        }),
        target: id,
        track: None,
        at: Time::ZERO,
        source_start: Time::ZERO,
        duration: None,
        audio_only: false,
        new_track: false,
        mode: Default::default(),
    };
    assert!(registry.place(&project.snapshot(), &request).is_err());
}
#[test]
fn native_shape_instances_receive_gpu_image_responses_with_shared_atlas() {
    let (mut m, source, _) = setup("fold.motion.image_card");
    let shape = Node::new("fold.motion.rectangle").unwrap();
    let mut points = Node::new("fold.motion.grid_points").unwrap();
    points.set("columns", Datum::Scalar(10.));
    points.set("rows", Datum::Scalar(10.));
    let mut instances = Node::new("fold.motion.instance_on_points").unwrap();
    instances.connect("source", shape.id, "content");
    instances.connect("points", points.id, "points");
    let mut sample = Node::new("fold.motion.sample_image").unwrap();
    sample.connect("image", source, "image");
    let mut response = Node::new("fold.motion.image_response").unwrap();
    response.connect("content", instances.id, "content");
    response.connect("sample", sample.id, "sample");
    response.set("scale", Datum::Scalar(0.5));
    let output = response.id;
    m.graph
        .nodes
        .extend([shape, points, instances, sample, response]);
    m.graph.output = Link {
        node: output,
        socket: "content".into(),
    };
    let graph = compile(&m);
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|n| matches!(n, ImageOp::Vector(_)))
            .count(),
        1
    );
    let draws = graph
        .nodes
        .iter()
        .filter_map(|n| {
            if let ImageOp::Textured { draw, .. } = n {
                Some(draw)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].instances.len(), 100);
    assert!(draws[0].document_samples);
}

#[test]
fn bake_node_preserves_sources_and_rejects_changed_settings() {
    let (mut motion, source, card) = setup("fold.motion.image_card");
    let mut bake = Node::new("fold.motion.image_bake").unwrap();
    bake.connect("image", source, "image");
    let id = bake.id;
    motion
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == card)
        .unwrap()
        .connect("image", id, "image");
    motion.graph.nodes.push(bake);
    let requests =
        fold_motion::nodes::image::bake::requests(&motion, id, Time::ZERO, &Default::default())
            .unwrap();
    assert_eq!(requests.len(), 1);
    assert!(requests[0].bake.is_none());
    let doc = motion.document(DocumentId::new()).unwrap();
    assert_eq!(doc.schema_version, 6);
    assert_eq!(doc.assets.len(), 1);
    let loaded = Motion::from_document(&doc).unwrap();
    assert_eq!(evaluate(&loaded).len(), 1);
    let node = motion.graph.nodes.iter_mut().find(|n| n.id == id).unwrap();
    let settings = node
        .settings::<fold_motion::nodes::image::bake::Settings>()
        .unwrap();
    node.settings["bake"] = serde_json::json!({"directory":"/tmp/test-bake","manifest":"not-read-by-evaluator","options":settings.options()});
    assert_eq!(evaluate(&motion).len(), 1);
    motion
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == id)
        .unwrap()
        .settings["resolution"] = 512.into();
    assert!(
        Evaluator::new(&motion, Time::ZERO)
            .unwrap()
            .scene()
            .unwrap_err()
            .contains("settings changed")
    );
}

#[test]
fn baked_card_collection_batches_distinct_textures_into_one_array() {
    let (mut motion, source, card) = setup("fold.motion.image_cards");
    let mut points = Node::new("fold.motion.grid_points").unwrap();
    points.set("columns", Datum::Scalar(5.));
    points.set("rows", Datum::Scalar(2.));
    let index = Node::new("fold.motion.index").unwrap();
    let object = motion
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == card)
        .unwrap();
    object.connect("points", points.id, "points");
    object.connect("index", index.id, "value");
    motion.graph.nodes.extend([points, index]);
    let source = motion
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == source)
        .unwrap();
    let mut settings = source.settings::<Settings>().unwrap();
    settings.sources = (0..10)
        .map(|_| {
            let mut e = settings.sources[0].clone();
            e.source = Source::Asset(AssetId::new());
            e
        })
        .collect();
    source.settings = serde_json::to_value(settings).unwrap();
    let count = std::cell::Cell::new(0u32);
    let graph = fold_motion::geometry::render::compile_images(
        &evaluate(&motion),
        128,
        96,
        [1.; 2],
        &Default::default(),
        &|_| {
            let i = count.get();
            count.set(i + 1);
            let frame = fold_render::render(RenderGraph {
                width: 8,
                height: 8,
                nodes: vec![ImageOp::Solid {
                    rgba: [i as f32 / 10., 0., 0., 1.],
                }],
                output: 0,
            })?;
            let texture = fold_render::baked::Texture::encode(
                &frame,
                fold_render::baked::Format::Bc7,
                &Default::default(),
            )?;
            Ok(RenderGraph {
                width: 128,
                height: 96,
                nodes: vec![ImageOp::Baked(std::sync::Arc::new(texture))],
                output: 0,
            })
        },
    )
    .unwrap();
    let draws = graph
        .nodes
        .iter()
        .filter_map(|op| {
            if let ImageOp::BakedTextured { batch, .. } = op {
                Some(batch)
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    assert_eq!(draws.len(), 1);
    assert_eq!(draws[0].textures.len(), 10);
    assert_eq!(draws[0].draw.instances.len(), 10);
    assert_eq!(
        graph
            .nodes
            .iter()
            .filter(|n| matches!(n, ImageOp::Over { .. }))
            .count(),
        0
    );
}
