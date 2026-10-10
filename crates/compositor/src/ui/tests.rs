use super::*;
use crate::{Composite, Node, OperationConfig};
use fold_foundation::DocumentId;
use fold_platform::{desktop::*, packages::PackageRegistry};
use fold_project::{CommittedSnapshot, EditBatch, Mutation, Project};
use fold_ui::sdk::{ExtensionUi, Panel, graph_canvas::*, imgui};
pub(super) struct Host {
    pub(super) project: Project,
    state: DesktopState,
    registry: PackageRegistry,
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        match command {
            DesktopCommand::Select(selection) => self.state.selection = selection,
            DesktopCommand::CancelPreviewEdit => {}
            DesktopCommand::PreviewExtension(request) => {
                self.registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .unwrap();
            }
            DesktopCommand::Extension(request) => {
                let batch = self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .unwrap();
                self.project.commit(batch).unwrap();
            }
            other => panic!("unexpected command {other:?}"),
        }
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
pub(super) fn setup() -> (Host, state::State) {
    let mut registry = PackageRegistry::default();
    crate::package::register(&mut registry).unwrap();
    let mut host = Host {
        project: Project::new(16),
        state: DesktopState::default(),
        registry,
    };
    let source = Node::new(
        OperationConfig::Solid {
            rgba: [0.5, 0.2, 0.1, 1.],
        },
        vec![],
    );
    let output = Node::new(OperationConfig::Output, vec![source.id]);
    let graph = Composite {
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![source, output],
        extensions: Default::default(),
    };
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                graph.document(DocumentId::new()).unwrap(),
            )],
        })
        .unwrap();
    let mut state = state::State::default();
    state.sync(&mut host);
    (host, state)
}
#[test]
fn compositor_graph_composition_and_inspector_draw_without_authoring() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let (mut host, state) = setup();
    let before = host.project.snapshot();
    let state = Rc::new(RefCell::new(state));
    let selected = state.borrow().graph.as_ref().unwrap().nodes[0].id;
    state.borrow_mut().selected = vec![selected];
    let mut context = imgui::Context::create();
    let typography = fold_ui::sdk::typography::Typography::install(&mut context);
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = canvas::Canvas::new(state.clone());
    canvas.initialize(&context, Some(&typography));
    let mut inspector = inspector::Inspector(
        state,
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    for frame in 0..40 {
        let width = if frame % 20 >= 10 { 280. } else { 850. };
        context.io_mut().add_mouse_pos_event([width - 20., 700.]);
        let ui = context.frame();
        ui.window("graph")
            .position([0.; 2], imgui::Condition::Always)
            .size([width, 800.], imgui::Condition::Always)
            .build(|| {
                let top = ui.cursor_screen_pos()[1];
                if frame < 20 {
                    canvas.draw_network(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
                } else {
                    canvas.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
                }
                if frame >= 20 && frame % 10 >= 3 {
                    assert_eq!(
                        ui.scroll_max_x(),
                        0.,
                        "composition controls must fit the panel"
                    );
                }
                if frame < 20 && frame % 10 >= 3 {
                    assert_eq!(ui.scroll_max_x(), 0., "toolbar must fit the panel");
                    let y = top + ui.frame_height_with_spacing() * 2. + 8.;
                    assert!(
                        (10..width as i32 - 10)
                            .any(|x| canvas.accepts_background_pan([x as f32, y])),
                        "graph must start below at most two toolbar rows"
                    );
                }
            });
        ui.window("inspector")
            .position([850., 0.], imgui::Condition::Always)
            .size([350., 800.], imgui::Condition::Always)
            .build(|| {
                inspector.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                })
            });
        assert!(context.render_legacy().total_vtx_count() > 100);
    }
    assert_eq!(host.project.snapshot().revision(), before.revision());
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    drop(canvas);
    drop(inspector);
    drop(context);
}
#[test]
fn shared_editor_events_use_compositor_validation_and_atomic_undo() {
    let (mut host, mut state) = setup();
    let source = state.graph.as_ref().unwrap().nodes[0].id;
    let output = state.graph.as_ref().unwrap().output;
    let before = host.project.snapshot();
    let mut adapter = graph::Context {
        state: &mut state,
        host: &mut host,
    };
    adapter
        .event(GraphEvent::Select(vec![source, output]))
        .unwrap();
    assert_eq!(adapter.state.selected, vec![source, output]);
    assert!(
        adapter
            .validate(&GraphChange::Delete(vec![output]))
            .is_err()
    );
    for x in [10., 20., 30.] {
        adapter
            .event(GraphEvent::PreviewPositions(vec![(source, [x, 10.])]))
            .unwrap();
    }
    assert_eq!(
        adapter.host.snapshot().unwrap().revision(),
        before.revision()
    );
    adapter.event(GraphEvent::Cancel).unwrap();
    assert_eq!(
        adapter
            .state
            .graph
            .as_ref()
            .unwrap()
            .node(source)
            .unwrap()
            .position,
        None
    );
    adapter
        .event(GraphEvent::PreviewPositions(vec![(source, [50., 60.])]))
        .unwrap();
    adapter.event(GraphEvent::Commit(vec![])).unwrap();
    assert_eq!(
        adapter.host.snapshot().unwrap().revision().0,
        before.revision().0 + 1
    );
    let committed = adapter.state.graph.clone();
    assert!(
        adapter
            .event(GraphEvent::Commit(vec![GraphChange::Delete(vec![
                source, output
            ])]))
            .is_err()
    );
    assert_eq!(
        adapter.state.graph, committed,
        "failed batch must not partially delete nodes"
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn inspector_does_not_finish_an_animation_gesture() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let (mut host, mut state) = setup();
    let before = host.project.snapshot().revision();
    let node = &mut state.graph.as_mut().unwrap().nodes[0];
    state.selected = vec![node.id];
    let mut curve = fold_animation::Curve::default();
    curve.insert(fold_foundation::Time::ZERO, 0.5);
    node.animation.insert("color.3".into(), curve).unwrap();
    state.preview(&mut host);
    assert!(state.editing && !state.property_editing);
    let state = Rc::new(RefCell::new(state));
    let mut inspector = inspector::Inspector(
        state.clone(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([400., 400.]);
    context.io_mut().set_delta_time(1. / 60.);
    for _ in 0..3 {
        let ui = context.frame();
        ui.window("Inspector").build(|| {
            inspector.draw(ExtensionUi {
                ui,
                host: &mut host,
            })
        });
        drop(context.render_legacy());
    }
    assert_eq!(host.project.snapshot().revision(), before);
    assert!(state.borrow().editing);
    state.borrow_mut().commit(&mut host);
    assert_ne!(host.project.snapshot().revision(), before);
    host.project.undo().unwrap();
    assert!(
        Composite::from_document(
            host.project
                .snapshot()
                .state()
                .documents
                .values()
                .next()
                .unwrap()
        )
        .unwrap()
        .nodes[0]
            .animation
            .is_empty()
    );
}

#[test]
fn production_node_inspectors_fit_normal_and_narrow_panels_without_edits() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let (mut host, initial) = setup();
    let document = initial.document.unwrap();
    let mut composite = initial.graph.unwrap();
    let image = composite.nodes[0].id;
    for parameters in [
        OperationConfig::ColorGrade {
            settings: Default::default(),
        },
        OperationConfig::TransformImage {
            settings: Default::default(),
        },
        OperationConfig::Blur {
            filter: Default::default(),
            size: [3.; 2],
            edges: Default::default(),
        },
        OperationConfig::Composite {
            mode: Default::default(),
        },
        OperationConfig::Shuffle { mappings: vec![] },
        OperationConfig::RemoveChannels {
            channels: vec![],
            keep: false,
        },
        OperationConfig::Unary {
            operation: fold_render::operations::Unary::Exposure { stops: 0. },
        },
        OperationConfig::Shape {
            shape: crate::Shape::Ellipse,
            bounds: [0., 0., 16., 16.],
        },
        OperationConfig::Read {
            source: crate::Source::Unassigned {
                info: composite.info.clone(),
            },
            start: fold_foundation::Time::ZERO,
            source_start: fold_foundation::Time::ZERO,
            duration: composite.info.time(24).unwrap(),
        },
    ] {
        let mut node = Node::disconnected(parameters);
        for slot in 0..node.inputs.len() {
            node.set_input(slot, image).unwrap();
        }
        if node
            .prepare()
            .unwrap()
            .config()
            .operator()
            .supports_effect()
        {
            node.effect.set_mask(image);
        }
        composite.nodes.push(node);
    }
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(composite.document(document).unwrap())],
        })
        .unwrap();
    let before = host.project.snapshot().revision();
    let state = Rc::new(RefCell::new(state::State::default()));
    let mut inspector = inspector::Inspector(
        state.clone(),
        Default::default(),
        Default::default(),
        Default::default(),
        Default::default(),
    );
    state.borrow_mut().sync(&mut host);
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 900.]);
    context.io_mut().set_delta_time(1. / 60.);
    for node in &composite.nodes {
        state.borrow_mut().selected = vec![node.id];
        for width in [420., 280.] {
            for frame in 0..3 {
                let ui = context.frame();
                ui.window("Inspector")
                    .position([0.; 2], imgui::Condition::Always)
                    .size([width, 850.], imgui::Condition::Always)
                    .build(|| {
                        inspector.draw(ExtensionUi {
                            ui,
                            host: &mut host,
                        });
                        if frame == 2 {
                            assert_eq!(
                                ui.scroll_max_x(),
                                0.,
                                "{} at {width}px",
                                node.prepare().unwrap().config().operator().label()
                            );
                        }
                    });
                drop(context.render_legacy());
            }
        }
    }
    assert_eq!(host.project.snapshot().revision(), before);
}

#[test]
fn merge_insertion_uses_b_and_undo_restores_position_and_original_pipe() {
    let (mut host, mut state) = setup();
    let source = state.graph.as_ref().unwrap().nodes[0].id;
    let output = state.graph.as_ref().unwrap().output;
    {
        let mut context = graph::Context {
            state: &mut state,
            host: &mut host,
        };
        context
            .event(GraphEvent::Commit(vec![GraphChange::Add {
                template: "Merge".into(),
                position: [320., 160.],
            }]))
            .unwrap();
    }
    let merge = state.selected[0];
    let before = host.project.snapshot();
    let mut context = graph::Context {
        state: &mut state,
        host: &mut host,
    };
    assert_eq!(context.presentation(), GraphPresentation::VerticalCompact);
    let view = context.graph();
    let node = view.nodes.iter().find(|n| n.id == merge).unwrap();
    assert_eq!(context.primary_input(node), Some("background"));
    let pipe = GraphChange::Rewire {
        remove: vec![Socket {
            node: output,
            key: "image".into(),
        }],
        add: vec![
            WireView {
                source: Socket {
                    node: source,
                    key: "image".into(),
                },
                target: Socket {
                    node: merge,
                    key: "background".into(),
                },
            },
            WireView {
                source: Socket {
                    node: merge,
                    key: "image".into(),
                },
                target: Socket {
                    node: output,
                    key: "image".into(),
                },
            },
        ],
    };
    context.validate(&pipe).unwrap();
    context
        .event(GraphEvent::PreviewPositions(vec![(merge, [0., 80.])]))
        .unwrap();
    context.event(GraphEvent::Commit(vec![pipe])).unwrap();
    let composite = context.state.graph.as_ref().unwrap();
    assert_eq!(
        composite.node(merge).unwrap().input_ids(),
        [None, Some(source)]
    );
    assert_eq!(composite.node(output).unwrap().input_ids(), [Some(merge)]);
    assert_eq!(composite.node(merge).unwrap().position, Some([0., 80.]));
    let saved = Composite::from_document(
        &host.project.snapshot().state().documents[&state.document.unwrap()],
    )
    .unwrap();
    assert_eq!(saved.node(merge).unwrap().input_ids(), [None, Some(source)]);
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn invalid_rewire_batch_never_partially_disconnects() {
    let (mut host, mut state) = setup();
    let before = state.graph.clone();
    let graph = state.graph.as_ref().unwrap();
    let source = graph.nodes[0].id;
    let target = graph.output;
    let mut context = graph::Context {
        state: &mut state,
        host: &mut host,
    };
    let change = GraphChange::Rewire {
        remove: vec![Socket {
            node: target,
            key: "image".into(),
        }],
        add: vec![WireView {
            source: Socket {
                node: source,
                key: "image".into(),
            },
            target: Socket {
                node: target,
                key: "invalid".into(),
            },
        }],
    };
    assert!(context.validate(&change).is_err());
    assert!(context.event(GraphEvent::Commit(vec![change])).is_err());
    assert_eq!(state.graph, before);
}
