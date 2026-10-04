use fold_compositor::{ChannelMapping, ChannelSource, Composite, Node, Parameters as P};
use fold_foundation::{DocumentId, Time};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
fn name(value: &str) -> fold_render::channels::ChannelName {
    value.to_owned().try_into().unwrap()
}
fn fixture(nodes: Vec<Node>) -> Composite {
    let output = Node::new(P::Output, vec![nodes.last().unwrap().id]);
    let mut nodes = nodes;
    let id = output.id;
    nodes.push(output);
    Composite {
        info: fold_media::VideoInfo {
            width: 3,
            height: 2,
            rate: [24, 1],
            frames: 24,
        },
        nodes,
        output: id,
        extensions: Default::default(),
    }
}
fn compile(composite: &Composite) -> Result<fold_render::RenderGraph, String> {
    let id = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(composite.document(id)?)],
        })
        .unwrap();
    let mut registry = fold_platform::packages::PackageRegistry::default();
    fold_compositor::package::register(&mut registry).unwrap();
    registry.video(
        &project.snapshot(),
        &DocumentRef {
            document: id,
            output: "video".into(),
            extensions: Default::default(),
        },
        Time::ZERO,
        3,
        2,
    )
}
#[test]
fn twenty_layers_survive_processing_and_shuffle_without_twenty_pixel_buffers() {
    let source = Node::new(
        P::Solid {
            rgba: [0.1, 0.2, 0.3, 1.],
        },
        vec![],
    );
    let mut layers = Node::disconnected(P::Shuffle {
        mappings: (0..20)
            .map(|i| ChannelMapping {
                destination: name(&format!("aov{i}.Z")),
                source: ChannelSource::A(name("rgba.blue")),
            })
            .collect(),
    });
    layers.inputs[0] = Some(source.id);
    let grade = Node::new(P::Grade { gain: [2.; 3] }, vec![layers.id]);
    let mut select = Node::disconnected(P::Shuffle {
        mappings: vec![ChannelMapping {
            destination: name("rgba.red"),
            source: ChannelSource::A(name("aov19.Z")),
        }],
    });
    select.inputs[0] = Some(grade.id);
    let composite = fixture(vec![source, layers, grade, select]);
    assert_eq!(composite.channel_names(composite.output).unwrap().len(), 24);
    let graph = compile(&composite).unwrap();
    assert!(graph.nodes.len() < 8);
    for pixel in fold_render::render(graph).unwrap().pixels() {
        assert_eq!(*pixel, [0.3, 0.4, 0.6, 1.]);
    }
}
#[test]
fn arbitrary_image_channel_masks_are_optional_and_cycles_rollback() {
    let source = Node::new(
        P::Solid {
            rgba: [0.2, 0.2, 0.2, 1.],
        },
        vec![],
    );
    let mask = Node::new(
        P::Solid {
            rgba: [0.25, 0., 0., 1.],
        },
        vec![],
    );
    let mut grade = Node::new(P::Grade { gain: [2.; 3] }, vec![source.id]);
    grade.effect.mask_channel = name("rgba.red");
    let grade_id = grade.id;
    let mask_id = mask.id;
    let mut composite = fixture(vec![source, mask, grade]);
    composite.connect(mask_id, grade_id, "mask").unwrap();
    let before = composite.clone();
    assert!(composite.connect(grade_id, grade_id, "mask").is_err());
    assert_eq!(before, composite);
    let result = fold_render::render(compile(&composite).unwrap()).unwrap();
    assert_eq!(result.pixels(), &[[0.25, 0.25, 0.25, 1.]; 6]);
    composite.node_mut(grade_id).unwrap().effect.mask_disabled = true;
    assert_eq!(
        fold_render::render(compile(&composite).unwrap())
            .unwrap()
            .pixels(),
        &[[0.4, 0.4, 0.4, 1.]; 6]
    );
    assert_eq!(composite.node(grade_id).unwrap().effect.mask, Some(mask_id));
    composite.node_mut(grade_id).unwrap().effect.mask_disabled = false;
    assert_eq!(
        fold_render::render(compile(&composite).unwrap())
            .unwrap()
            .pixels(),
        &[[0.25, 0.25, 0.25, 1.]; 6]
    );
    composite.disconnect(grade_id, "mask").unwrap();
    assert_eq!(
        fold_render::render(compile(&composite).unwrap())
            .unwrap()
            .pixels(),
        &[[0.4, 0.4, 0.4, 1.]; 6]
    );
}
#[test]
fn shuffle_swaps_simultaneously_and_requires_b_only_when_referenced() {
    let source = Node::new(
        P::Solid {
            rgba: [0.1, 0.2, 0.3, 1.],
        },
        vec![],
    );
    let mut shuffle = Node::disconnected(P::Shuffle {
        mappings: vec![
            ChannelMapping {
                destination: name("rgba.red"),
                source: ChannelSource::A(name("rgba.blue")),
            },
            ChannelMapping {
                destination: name("rgba.blue"),
                source: ChannelSource::A(name("rgba.red")),
            },
        ],
    });
    shuffle.inputs[0] = Some(source.id);
    let mut composite = fixture(vec![source, shuffle]);
    assert_eq!(
        fold_render::render(compile(&composite).unwrap())
            .unwrap()
            .pixels(),
        &[[0.3, 0.2, 0.1, 1.]; 6]
    );
    if let P::Shuffle { mappings } = &mut composite.nodes[1].parameters {
        mappings[0].source = ChannelSource::B(name("rgba.red"));
    }
    assert!(compile(&composite).unwrap_err().contains("connect B"));
}

#[test]
fn generated_shapes_connect_to_explicit_alpha_multiplication() {
    let source = Node::new(
        P::Solid {
            rgba: [0.2, 0.4, 0.6, 1.],
        },
        vec![],
    );
    let shape = Node::new(
        P::Shape {
            shape: fold_compositor::Shape::Rectangle,
            bounds: [0., 0., 1., 2.],
        },
        vec![],
    );
    let mut multiply = Node::disconnected(P::ApplyMask);
    multiply.inputs[0] = Some(source.id);
    let shape_id = shape.id;
    let multiply_id = multiply.id;
    let mut composite = fixture(vec![source, shape, multiply]);
    composite.connect(shape_id, multiply_id, "mask").unwrap();
    composite.validate().unwrap();
    let result = fold_render::render(compile(&composite).unwrap()).unwrap();
    assert_eq!(result.pixels()[0], [0.2, 0.4, 0.6, 1.]);
    assert_eq!(result.pixels()[2], [0.; 4]);
}

#[test]
fn data_rgb_without_alpha_can_be_processed_without_being_zeroed() {
    let source = Node::new(
        P::Solid {
            rgba: [0.1, 0.2, 0.3, 1.],
        },
        vec![],
    );
    let mut layers = Node::disconnected(P::Shuffle {
        mappings: ["red", "green", "blue"]
            .map(|component| ChannelMapping {
                destination: name(&format!("data.{component}")),
                source: ChannelSource::A(name(&format!("rgba.{component}"))),
            })
            .into(),
    });
    layers.inputs[0] = Some(source.id);
    let settings = fold_render::operations::Grade {
        gain: [2.; 3],
        ..Default::default()
    };
    let mut grade = Node::new(P::ColorGrade { settings }, vec![layers.id]);
    grade.effect.channels = ["data.red", "data.green", "data.blue"].map(name).into();
    let mut select = Node::disconnected(P::Shuffle {
        mappings: vec![ChannelMapping {
            destination: name("rgba.red"),
            source: ChannelSource::A(name("data.red")),
        }],
    });
    select.inputs[0] = Some(grade.id);
    let composite = fixture(vec![source, layers, grade, select]);
    assert_eq!(
        fold_render::render(compile(&composite).unwrap())
            .unwrap()
            .pixels()[0],
        [0.2, 0.2, 0.3, 1.]
    );
}

#[test]
fn read_does_not_infer_color_processing_for_rgb_utility_layers() {
    let image = fold_media::exr::Info {
        dimensions: [3, 2],
        display_origin: [0; 2],
        data_origin: [0; 2],
        data_dimensions: [3, 2],
        channels: ["rgba", "normal", "diffuse"]
            .into_iter()
            .flat_map(|layer| {
                ["red", "green", "blue"].map(|component| fold_media::exr::Channel {
                    name: format!("{layer}.{component}"),
                    source: format!("{layer}.{component}"),
                    color: true,
                })
            })
            .collect(),
    };
    let source = fold_compositor::source_from_profile(
        fold_foundation::AssetId::new(),
        fold_media::ingest::SourceProfile::Exr(image),
        &fold_media::VideoInfo {
            width: 3,
            height: 2,
            rate: [24, 1],
            frames: 24,
        },
    )
    .unwrap();
    let fold_compositor::Source::Exr { color_layers, .. } = source else {
        panic!("EXR source expected")
    };
    assert_eq!(color_layers, ["rgba"]);
}

#[test]
fn disabled_mask_does_not_evaluate_an_incomplete_mask_branch() {
    let source = Node::new(
        P::Solid {
            rgba: [0.2, 0.2, 0.2, 1.],
        },
        vec![],
    );
    let mask = Node::disconnected(P::Grade { gain: [1.; 3] });
    let mut grade = Node::new(P::Grade { gain: [2.; 3] }, vec![source.id]);
    grade.effect.mask = Some(mask.id);
    grade.effect.mask_disabled = true;
    let id = grade.id;
    let mut composite = fixture(vec![source, mask, grade]);
    assert_eq!(
        fold_render::render(compile(&composite).unwrap())
            .unwrap()
            .pixels(),
        &[[0.4, 0.4, 0.4, 1.]; 6]
    );
    composite.node_mut(id).unwrap().effect.mask_disabled = false;
    assert!(compile(&composite).unwrap_err().contains("connect"));
}
