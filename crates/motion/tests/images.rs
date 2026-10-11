use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_motion::{
    binding::Binding,
    evaluation::{GeometryData, Limits, Prepared, Value},
    expression::{Binary, Element, Expr},
    model::{Construction, Node, Reference},
    registry,
    types::{AssetReference, Literal, Space, Type, Unit},
};
use fold_platform::image_sampling::{Resolver, Service};
use fold_project::{Asset, EditBatch, Mutation, Project};
use std::sync::Arc;
fn val(value: Literal) -> Binding {
    Binding::Literal { value }
}
fn text(value: &str) -> Binding {
    val(Literal::Text {
        value: value.into(),
    })
}
fn link(node: ObjectId) -> Binding {
    Binding::Link {
        reference: Reference::Node { node }.into(),
    }
}
fn node(key: &str, mode: &str, args: Vec<(&str, Binding)>) -> Node {
    registry::resolve(
        &format!("fold.motion.{key}"),
        if matches!(key, "distribute" | "cloner") {
            2
        } else {
            1
        },
        mode,
    )
    .unwrap()
    .node(key, args.into_iter().map(|(k, v)| (k.into(), v)).collect())
    .unwrap()
}
fn graph(nodes: Vec<Node>, ty: Type) -> Construction {
    let mut graph =
        Construction::from_document(&fold_motion::package::create(DocumentId::new()).unwrap())
            .unwrap();
    graph.definitions[0].outputs[0].ty = ty;
    graph.definitions[0].outputs[0].value = Reference::Node {
        node: nodes.last().unwrap().id,
    }
    .into();
    graph.definitions[0].nodes = nodes;
    graph
}
fn source(asset: AssetId) -> Node {
    node(
        "image_texture",
        "asset",
        vec![(
            "asset",
            val(Literal::AssetRef {
                value: AssetReference::Resource(asset),
            }),
        )],
    )
}
fn sample(texture: &Node, mode: &str) -> Node {
    node("sample_image", mode, vec![("image", link(texture.id))])
}
fn set(node: &mut Node, key: &str, value: Binding) {
    node.parameters.get_mut(key).unwrap().value = serde_json::to_value(value).unwrap();
}
fn uv(x: f64, y: f64) -> Binding {
    val(Literal::Vector {
        value: vec![x, y],
        unit: Unit::Unitless,
        space: Space::World,
    })
}
fn run(graph: &Construction, images: &dyn Resolver, time: Time) -> Value {
    Prepared::new(graph.clone())
        .unwrap()
        .with_images(images)
        .evaluate(
            graph.definitions[0].outputs[0].id,
            time,
            Limits::default(),
            &|| Ok(()),
        )
        .unwrap()
}
fn color(value: Value) -> [f64; 4] {
    let Value::Primitive(Literal::Color { value, .. }) = value else {
        panic!("color")
    };
    value
}
fn fixture() -> (tempfile::TempDir, Project, AssetId) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("two.ppm");
    let mut bytes = b"P6\n2 1\n255\n".to_vec();
    bytes.extend([255, 0, 0, 0, 255, 0]);
    std::fs::write(&path, bytes).unwrap();
    let source = fold_media::ingest::inspect_linked(&path, &Default::default()).unwrap();
    let id = AssetId::new();
    let asset = Asset {
        id,
        location: source.path.to_string_lossy().into(),
        fingerprint: source.fingerprint,
        extensions: [(
            fold_media::ingest::METADATA_KEY.into(),
            serde_json::to_value(source.metadata).unwrap(),
        )]
        .into(),
    };
    let mut project = Project::new(20);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutAsset(asset)],
        })
        .unwrap();
    (directory, project, id)
}
#[test]
fn image_pixels_filter_wrap_and_frame_reuse() {
    let (_dir, project, asset) = fixture();
    let snapshot = project.snapshot();
    let cancel = Default::default();
    let images = Service::new(&snapshot, &cancel, &|_, _| Err("no output".into()), &|_| {
        Err("no metadata".into())
    });
    let texture = source(asset);
    let sample = sample(&texture, "color");
    let mut c = graph(
        vec![texture, sample],
        Type::Color {
            encoding: fold_motion::types::Encoding::LinearRec709,
            alpha: fold_motion::types::Alpha::Straight,
        },
    );
    for (x, filter, wrap, expected) in [
        (0.25, "nearest", "clamp", [1., 0., 0., 1.]),
        (0.5, "bilinear", "clamp", [0.5, 0.5, 0., 1.]),
        (1.25, "nearest", "repeat", [1., 0., 0., 1.]),
        (1.25, "nearest", "mirror", [0., 1., 0., 1.]),
        (-0.25, "nearest", "transparent", [0.; 4]),
    ] {
        let n = &mut c.definitions[0].nodes[1];
        set(n, "uv", uv(x, 0.5));
        set(n, "filter", text(filter));
        set(n, "wrap", text(wrap));
        assert_eq!(color(run(&c, &images, Time::ZERO)), expected);
    }
    let request = fold_platform::image_sampling::Request {
        source: fold_platform::image_source::Source::Asset(asset),
        time: Time::ZERO,
        outside: Default::default(),
        color_space: None,
    };
    let first = images.frame(&request).unwrap();
    let second = images.frame(&request).unwrap();
    assert!(Arc::ptr_eq(&first, &second));
    assert_eq!(images.provenance().as_array().unwrap().len(), 1);
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    let service = Service::new(&snapshot, &cancel, &|_, _| Err("no".into()), &|_| {
        Err("no".into())
    });
    assert!(service.frame(&request).is_err());
}
#[test]
fn image_drives_cloner_and_materialized_attributes_with_stable_ids() {
    let (_dir, project, asset) = fixture();
    let snapshot = project.snapshot();
    let cancel = Default::default();
    let images = Service::new(&snapshot, &cancel, &|_, _| Err("no".into()), &|_| {
        Err("no".into())
    });
    let texture = source(asset);
    let mut sample = sample(&texture, "color");
    // Convert an exact element index to a scalar through duration arithmetic.
    set(
        &mut sample,
        "uv",
        Binding::Expression {
            expression: Expr::Vector {
                components: vec![
                    Expr::Binary {
                        op: Binary::Divide,
                        left: Box::new(Expr::Binary {
                            op: Binary::Multiply,
                            left: Box::new(Expr::Element {
                                property: Element::Index,
                            }),
                            right: Box::new(Expr::Literal {
                                value: Literal::Duration {
                                    value: Time::new(1, 1).unwrap(),
                                },
                            }),
                        }),
                        right: Box::new(Expr::Literal {
                            value: Literal::Duration {
                                value: Time::new(1, 1).unwrap(),
                            },
                        }),
                    },
                    Expr::Literal {
                        value: Literal::Scalar { value: 0.5 },
                    },
                ],
                unit: Unit::Unitless,
                space: Space::World,
            },
        },
    );
    let points = node(
        "distribute",
        "grid",
        vec![
            ("columns", val(Literal::Integer { value: 2 })),
            ("rows", val(Literal::Integer { value: 1 })),
        ],
    );
    let sphere = node("mesh", "sphere", vec![]);
    let clones = node(
        "cloner",
        "points",
        vec![("points", link(points.id)), ("prototype", link(sphere.id))],
    );
    let animator = node(
        "animator",
        "color_replace",
        vec![("geometry", link(clones.id)), ("color", link(sample.id))],
    );
    let mut c = graph(
        vec![texture, sample, points, sphere, clones, animator],
        Type::Geometry,
    );
    let mut ids = None;
    for t in [
        Time::new(2, 1).unwrap(),
        Time::ZERO,
        Time::new(-1, 24).unwrap(),
    ] {
        let Value::Geometry(g) = run(&c, &images, t) else {
            panic!()
        };
        let GeometryData::Instances(instances) = &g.data else {
            panic!()
        };
        assert_eq!(
            instances.instances[0].color.as_ref().unwrap().value,
            [1., 0., 0., 1.]
        );
        assert_eq!(
            instances.instances[1].color.as_ref().unwrap().value,
            [0., 1., 0., 1.]
        );
        let current: Vec<_> = instances.instances.iter().map(|i| i.id).collect();
        if let Some(previous) = &ids {
            assert_eq!(&current, previous);
        }
        ids = Some(current);
    }
    let attr = node(
        "attribute_create",
        "set",
        vec![
            ("geometry", link(c.definitions[0].nodes[2].id)),
            ("attribute", text("sampled")),
            ("value", link(c.definitions[0].nodes[1].id)),
        ],
    );
    c.definitions[0].outputs[0].value = Reference::Node { node: attr.id }.into();
    c.definitions[0].nodes.push(attr);
    let Value::Geometry(g) = run(&c, &images, Time::ZERO) else {
        panic!()
    };
    let a = g
        .attributes
        .get(
            fold_motion::evaluation::attributes::Domain::Point,
            "sampled",
        )
        .unwrap();
    assert_eq!(a.values.len(), 2);
    assert_ne!(a.values[0], a.values[1]);
    assert_eq!(images.provenance().as_array().unwrap().len(), 1);
}
#[test]
fn source_dependencies_roundtrip_undo_and_missing_assets() {
    let (dir, mut project, asset) = fixture();
    let texture = source(asset);
    let sample = sample(&texture, "scalar");
    let c = graph(vec![texture, sample], Type::Scalar);
    let canonical = fold_motion::source::canonical(&c).unwrap();
    assert_eq!(
        fold_motion::source::parse(&canonical).unwrap().construction,
        c
    );
    let id = DocumentId::new();
    let document = c.document(id).unwrap();
    assert_eq!(document.assets, vec![asset]);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document.clone())],
        })
        .unwrap();
    project.undo().unwrap();
    assert!(!project.snapshot().state().documents.contains_key(&id));
    project.redo().unwrap();
    let path = dir.path().join("images.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 20).unwrap();
    assert_eq!(
        Construction::from_document(&reopened.snapshot().state().documents[&id]).unwrap(),
        c
    );
    let mut invalid = document;
    invalid.assets.clear();
    assert!(Construction::from_document(&invalid).is_err());
    let empty = Project::new(0).snapshot();
    let cancel = Default::default();
    let service = Service::new(&empty, &cancel, &|_, _| Err("no".into()), &|_| {
        Err("no".into())
    });
    assert!(
        Prepared::new(c.clone())
            .unwrap()
            .with_images(&service)
            .evaluate(
                c.definitions[0].outputs[0].id,
                Time::ZERO,
                Default::default(),
                &|| Ok(())
            )
            .unwrap_err()
            .contains("missing")
    );
}
#[test]
fn source_time_range_and_fractional_document_time_are_explicit() {
    let mut project = Project::new(0);
    let id = DocumentId::new();
    let document = fold_motion::package::create(id).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    let snapshot = project.snapshot();
    let cancel = Default::default();
    let calls = std::cell::RefCell::new(vec![]);
    let resolve = |_: &fold_project::DocumentRef, time: Time| {
        calls.borrow_mut().push(time);
        Ok(fold_render::RenderGraph {
            width: 1,
            height: 1,
            nodes: vec![fold_render::ImageOp::Solid {
                rgba: [
                    (time.numerator() as f64 / f64::from(time.denominator())) as f32,
                    0.,
                    0.,
                    1.,
                ],
            }],
            output: 0,
        })
    };
    let describe = |_: &fold_project::DocumentRef| {
        Ok(fold_media::VideoInfo {
            width: 1,
            height: 1,
            frames: 2,
            rate: [2, 1],
        })
    };
    let images = Service::new(&snapshot, &cancel, &resolve, &describe);
    let texture = node(
        "image_texture",
        "output",
        vec![
            ("document", text(&id.to_string())),
            ("output", text("video")),
        ],
    );
    let sample = sample(&texture, "scalar");
    let mut c = graph(vec![texture, sample], Type::Scalar);
    set(&mut c.definitions[0].nodes[1], "channel", text("red"));
    for (policy, time, expected) in [
        ("hold", Time::new(-1, 4).unwrap(), 0.),
        ("hold", Time::new(3, 2).unwrap(), 0.5),
        ("loop", Time::new(-1, 4).unwrap(), 0.75),
        ("transparent", Time::new(2, 1).unwrap(), 0.),
        ("hold", Time::new(1, 4).unwrap(), 0.25),
    ] {
        set(&mut c.definitions[0].nodes[0], "outside", text(policy));
        let Value::Primitive(Literal::Scalar { value }) = run(&c, &images, time) else {
            panic!()
        };
        assert_eq!(value, expected);
    }
    assert!(calls.borrow().contains(&Time::new(1, 4).unwrap()));
    set(&mut c.definitions[0].nodes[0], "outside", text("error"));
    assert!(
        Prepared::new(c.clone())
            .unwrap()
            .with_images(&images)
            .evaluate(
                c.definitions[0].outputs[0].id,
                Time::new(2, 1).unwrap(),
                Default::default(),
                &|| Ok(())
            )
            .is_err()
    );
}
struct Solid(Arc<fold_render::Frame>);
impl Resolver for Solid {
    fn frame(
        &self,
        _: &fold_platform::image_sampling::Request,
    ) -> Result<Arc<fold_render::Frame>, String> {
        Ok(self.0.clone())
    }
}
#[test]
fn transparent_filtering_preserves_straight_color_and_math_works() {
    let images = Solid(Arc::new(
        fold_render::render(fold_render::SolidPlan {
            width: 1,
            height: 1,
            rgba: [0.5, 0.25, 0.125, 0.5],
        })
        .unwrap(),
    ));
    let texture = source(AssetId::new());
    let mut sampled = sample(&texture, "color");
    set(&mut sampled, "wrap", text("transparent"));
    set(&mut sampled, "uv", uv(0., 0.5));
    let mut c = graph(
        vec![texture, sampled],
        Type::Color {
            encoding: fold_motion::types::Encoding::LinearRec709,
            alpha: fold_motion::types::Alpha::Straight,
        },
    );
    assert_eq!(color(run(&c, &images, Time::ZERO)), [1., 0.5, 0.25, 0.25]);
    let scalar = sample(&c.definitions[0].nodes[0], "scalar");
    let expr = node(
        "expression",
        "value",
        vec![(
            "value",
            Binding::Expression {
                expression: Expr::Binary {
                    op: Binary::Multiply,
                    left: Box::new(Expr::Reference {
                        reference: Reference::Node { node: scalar.id }.into(),
                    }),
                    right: Box::new(Expr::Literal {
                        value: Literal::Scalar { value: 2. },
                    }),
                },
            },
        )],
    );
    c = graph(
        vec![c.definitions[0].nodes[0].clone(), scalar, expr],
        Type::Signal {
            value: Box::new(Type::Scalar),
        },
    );
    let Value::Primitive(Literal::Scalar { value }) = run(&c, &images, Time::ZERO) else {
        panic!()
    };
    assert!((value - 2. * (0.2126 + 0.5 * 0.7152 + 0.25 * 0.0722)).abs() < 1e-7);
}
#[test]
fn parameter_sampling_uses_snapshot_resources_and_cancel() {
    let (_dir, mut project, asset) = fixture();
    let texture = source(asset);
    let sample = sample(&texture, "scalar");
    let expression = node("expression", "value", vec![("value", link(sample.id))]);
    let c = graph(
        vec![texture, sample, expression],
        Type::Signal {
            value: Box::new(Type::Scalar),
        },
    );
    let id = DocumentId::new();
    let doc = c.document(id).unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_motion::package::register(&mut registry).unwrap();
    let target = registry
        .parameters(&doc)
        .unwrap()
        .into_iter()
        .find(|e| {
            e.target.parameter.native() == Some(c.definitions[0].nodes[2].parameters["value"].id)
                && e.target.component.is_none()
        })
        .unwrap()
        .target;
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(doc)],
        })
        .unwrap();
    let value = registry
        .sample_placed_parameter(
            &project.snapshot(),
            &target,
            Time::ZERO,
            &Default::default(),
        )
        .unwrap();
    assert_eq!(value, Literal::Scalar { value: 0.4639 });
    let cancel = fold_media::Cancel::default();
    cancel.cancel();
    assert!(
        registry
            .sample_placed_parameter(&project.snapshot(), &target, Time::ZERO, &cancel)
            .is_err()
    );
}
#[test]
fn oversized_sources_fail_before_render_and_external_identity_is_static() {
    let texture = source(AssetId::new());
    let mut c = graph(vec![texture], Type::Image);
    set(
        &mut c.definitions[0].nodes[0],
        "asset",
        Binding::Expression {
            expression: Expr::Literal {
                value: Literal::AssetRef {
                    value: AssetReference::Resource(AssetId::new()),
                },
            },
        },
    );
    assert!(c.validate_semantics().unwrap_err().contains("literal"));
    let project = Project::new(0);
    let snapshot = project.snapshot();
    let cancel = Default::default();
    let images = Service::new(
        &snapshot,
        &cancel,
        &|_, _| panic!("oversized source must not render"),
        &|_| {
            Ok(fold_media::VideoInfo {
                width: 8192,
                height: 8193,
                rate: [24, 1],
                frames: 1,
            })
        },
    );
    let request = fold_platform::image_sampling::Request {
        source: fold_platform::image_source::Source::Output(fold_project::DocumentRef {
            document: DocumentId::new(),
            output: "video".into(),
            extensions: Default::default(),
        }),
        time: Time::ZERO,
        outside: Default::default(),
        color_space: None,
    };
    assert!(images.frame(&request).unwrap_err().contains("dimensions"));
}
#[test]
fn image_document_cycles_are_rejected_by_project_transactions() {
    let id = DocumentId::new();
    let texture = node(
        "image_texture",
        "output",
        vec![
            ("document", text(&id.to_string())),
            ("output", text("video")),
        ],
    );
    let c = graph(vec![texture], Type::Image);
    let doc = c.document(id).unwrap();
    assert_eq!(doc.dependencies.len(), 1);
    let mut project = Project::new(0);
    assert!(
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(doc)]
            })
            .is_err()
    );
    assert!(project.snapshot().state().documents.is_empty());
}
#[test]
fn scalar_channel_cannot_change_its_declared_type() {
    let images = Solid(Arc::new(
        fold_render::render(fold_render::SolidPlan {
            width: 1,
            height: 1,
            rgba: [0.2, 0.2, 0.2, 1.],
        })
        .unwrap(),
    ));
    let texture = source(AssetId::new());
    let mut sampled = sample(&texture, "scalar");
    set(&mut sampled, "channel", text("color"));
    let c = graph(vec![texture, sampled], Type::Scalar);
    assert!(
        Prepared::new(c.clone())
            .unwrap()
            .with_images(&images)
            .evaluate(
                c.definitions[0].outputs[0].id,
                Time::ZERO,
                Default::default(),
                &|| Ok(())
            )
            .unwrap_err()
            .contains("Unsupported image channel")
    );
}
