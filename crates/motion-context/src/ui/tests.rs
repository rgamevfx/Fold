use super::*;
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion_canvas::{
    editing::Edit,
    model::{Canvas, Content, Contour, Object, Point, Scalar},
};
use fold_platform::{
    desktop::{DesktopCommand, DesktopState},
    packages::PackageRegistry,
};
use fold_project::{EditBatch, EditSession, Mutation, Project};

struct Host {
    project: Project,
    registry: PackageRegistry,
    state: DesktopState,
    preview: Option<EditSession>,
    preview_requests: usize,
}
impl DesktopClient for Host {
    fn poll(&mut self) {}
    fn request_preview(&mut self, _: fold_platform::desktop::PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<fold_platform::desktop::PreviewResult> {
        None
    }
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<fold_project::CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn command(&mut self, command: DesktopCommand) {
        match command {
            DesktopCommand::Select(selection) => self.state.selection = selection,
            DesktopCommand::CancelPreviewEdit => self.preview = None,
            DesktopCommand::PreviewExtension(request) => {
                self.preview_requests += 1;
                let batch = self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .unwrap();
                let preview = self
                    .preview
                    .get_or_insert_with(|| self.project.begin_edit());
                self.project.update_edit(preview, batch.mutations).unwrap();
            }
            DesktopCommand::Extension(request) => {
                let batch = self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .unwrap();
                self.project.commit(batch).unwrap();
                self.preview = None;
            }
            _ => panic!("unexpected editor command"),
        }
    }
}
fn setup() -> (Host, DocumentId, ObjectId, ObjectId) {
    let object = ObjectId::new();
    let point = ObjectId::new();
    let document = DocumentId::new();
    let canvas = Canvas {
        width: 100,
        height: 100,
        roots: vec![object],
        objects: [(
            object,
            Object::new(
                "Curve",
                Content::Path {
                    contours: vec![Contour {
                        id: ObjectId::new(),
                        points: vec![Point {
                            position_track: None,
                            incoming_track: None,
                            outgoing_track: None,
                            id: point,
                            position: [20., 20.],
                            incoming: [-5., 0.],
                            outgoing: [5., 0.],
                            extensions: Default::default(),
                        }],
                        closed: false,
                        extensions: Default::default(),
                    }],
                    extensions: Default::default(),
                },
            ),
        )]
        .into(),
        ..Default::default()
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(canvas.document(document).unwrap())],
        })
        .unwrap();
    let mut registry = PackageRegistry::default();
    crate::package::register(&mut registry).unwrap();
    let state = DesktopState {
        selection: fold_platform::desktop::Selection {
            document: Some(document),
            objects: vec![object],
        },
        navigation: vec![fold_platform::desktop::ViewLocation {
            document,
            time: Time::ZERO,
            label: "Curve".into(),
        }],
        ..Default::default()
    };
    (
        Host {
            project,
            registry,
            state,
            preview: None,
            preview_requests: 0,
        },
        document,
        object,
        point,
    )
}
fn position(canvas: &Canvas, object: ObjectId) -> [f64; 2] {
    let Content::Path { contours, .. } = &canvas.objects[&object].content else {
        unreachable!()
    };
    contours[0].points[0].position
}

#[test]
fn unselected_path_is_picked_and_moved_with_one_undo_at_two_view_sizes() {
    for (window_width, image_size) in [(600., 300.), (300., 150.)] {
        let (mut host, document, object, _) = setup();
        let mut canvas =
            Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
        let value = canvas.objects.get_mut(&object).unwrap();
        value.paint.fill = None;
        value.paint.stroke = Some([1.; 4].into());
        value.paint.stroke_width.value = 3.;
        let Content::Path { contours, .. } = &mut value.content else {
            unreachable!()
        };
        contours[0].points.push(Point {
            position_track: None,
            incoming_track: None,
            outgoing_track: None,
            id: ObjectId::new(),
            position: [80., 20.],
            incoming: [-5., 0.],
            outgoing: [0.; 2],
            extensions: Default::default(),
        });
        host.project
            .commit(EditBatch {
                base: host.project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(canvas.document(document).unwrap())],
            })
            .unwrap();
        host.state.selection.objects.clear();
        let mut state = state::State::default();
        let mut viewport = viewport::Viewport::default();
        let mut context = sdk::imgui::Context::create();
        viewport.initialize(&context);
        context.io_mut().set_config_input_trickle_event_queue(false);
        context.set_ini_filename(None::<String>).unwrap();
        sdk::typography::Typography::install(&mut context);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([window_width, 500.]);
        context.io_mut().set_delta_time(1. / 60.);
        let rect = sdk::ViewerRect {
            editable: true,
            image_current: true,
            canvas_origin: [10., 30.],
            canvas_size: [window_width - 20., 450.],
            origin: [50., 100.],
            size: [image_size; 2],
            dimensions: [100; 2],
        };
        let start = [50. + image_size * 0.5, 100. + image_size * 0.2];
        let end = [start[0] + image_size * 0.1, start[1] + image_size * 0.1];
        for (mouse, down) in [
            ([window_width - 5., 490.], false),
            ([window_width - 5., 490.], false),
            (start, true),
            (end, true),
            (end, false),
        ] {
            context.io_mut().add_mouse_pos_event(mouse);
            context
                .io_mut()
                .add_mouse_button_event(sdk::imgui::MouseButton::Left, down);
            let ui = context.frame();
            ui.window("Motion")
                .position([0.; 2], sdk::imgui::Condition::Always)
                .size([window_width, 500.], sdk::imgui::Condition::Always)
                .build(|| {
                    state.sync(&mut host);
                    viewport.draw(
                        ExtensionUi {
                            ui,
                            host: &mut host,
                        },
                        rect,
                        &mut state,
                    );
                });
            drop(context.render_legacy());
            assert!(state.error.is_empty(), "{}", state.error);
        }
        assert_eq!(host.state.selection.objects, vec![object]);
        let after =
            Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
        assert_eq!(after.objects[&object].transform.x.value, 10.);
        assert_eq!(after.objects[&object].transform.y.value, 10.);
        assert_eq!(position(&after, object), [20., 20.]);
        host.project.undo().unwrap();
        let restored =
            Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
        assert_eq!(restored.objects[&object].transform.x.value, 0.);
        assert_eq!(restored.objects[&object].transform.y.value, 0.);
    }
}

#[test]
fn pen_inserts_an_anchor_on_a_selected_curve_with_one_undo() {
    let (mut host, document, object, original_point) = setup();
    let mut canvas =
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
    let Content::Path { contours, .. } = &mut canvas.objects.get_mut(&object).unwrap().content
    else {
        unreachable!()
    };
    contours[0].points[0].outgoing = [0., 40.];
    let end = Point {
        position_track: None,
        incoming_track: None,
        outgoing_track: None,
        id: ObjectId::new(),
        position: [80., 20.],
        incoming: [0., 40.],
        outgoing: [0.; 2],
        extensions: Default::default(),
    };
    contours[0].points.push(end.clone());
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(canvas.document(document).unwrap())],
        })
        .unwrap();
    let mut state = state::State::default();
    let mut viewport = viewport::Viewport::default();
    let mut context = sdk::imgui::Context::create();
    viewport.initialize(&context);
    context.io_mut().set_config_input_trickle_event_queue(false);
    context.set_ini_filename(None::<String>).unwrap();
    sdk::typography::Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([600., 500.]);
    context.io_mut().set_delta_time(1. / 60.);
    let rect = sdk::ViewerRect {
        editable: true,
        image_current: true,
        canvas_origin: [10., 30.],
        canvas_size: [580., 450.],
        origin: [50., 100.],
        size: [300.; 2],
        dimensions: [100; 2],
    };
    for (pen, down) in [
        (false, false),
        (true, false),
        (false, false),
        (false, true),
        (false, false),
    ] {
        // Midpoint of the cubic is [50, 50] in document coordinates.
        context.io_mut().add_mouse_pos_event([200., 250.]);
        context.io_mut().add_key_event(sdk::imgui::Key::P, pen);
        context
            .io_mut()
            .add_mouse_button_event(sdk::imgui::MouseButton::Left, down);
        let ui = context.frame();
        ui.window("Motion")
            .position([0.; 2], sdk::imgui::Condition::Always)
            .size([600., 500.], sdk::imgui::Condition::Always)
            .build(|| {
                state.sync(&mut host);
                viewport.draw(
                    ExtensionUi {
                        ui,
                        host: &mut host,
                    },
                    rect,
                    &mut state,
                );
            });
        drop(context.render_legacy());
        assert!(state.error.is_empty(), "{}", state.error);
    }
    let after =
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
    assert_eq!(after.objects.len(), 1);
    let Content::Path { contours, .. } = &after.objects[&object].content else {
        unreachable!()
    };
    assert_eq!(contours[0].points.len(), 3);
    assert_eq!(contours[0].points[0].id, original_point);
    assert_eq!(contours[0].points[2].id, end.id);
    assert!((contours[0].points[1].position[0] - 50.).abs() < 0.01);
    assert!((contours[0].points[1].position[1] - 50.).abs() < 0.01);
    assert_eq!(host.state.selection.objects, vec![object]);
    assert_eq!(state.point, Some((object, contours[0].points[1].id)));
    host.project.undo().unwrap();
    let restored =
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
    assert_eq!(restored, canvas);
    state.sync(&mut host);
    assert_eq!(state.point, None);
    host.project.redo().unwrap();
    let redone =
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
    assert_eq!(redone, after);
    state.sync(&mut host);
    state.point = Some((object, contours[0].points[1].id));
    for down in [true, false, false] {
        context
            .io_mut()
            .add_key_event(sdk::imgui::Key::Delete, down);
        let ui = context.frame();
        ui.window("Motion")
            .position([0.; 2], sdk::imgui::Condition::Always)
            .size([600., 500.], sdk::imgui::Condition::Always)
            .build(|| {
                state.sync(&mut host);
                viewport.draw(
                    ExtensionUi {
                        ui,
                        host: &mut host,
                    },
                    rect,
                    &mut state,
                );
            });
        drop(context.render_legacy());
        assert!(state.error.is_empty(), "{}", state.error);
    }
    let removed =
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
    assert_eq!(removed.objects.len(), 1);
    let Content::Path {
        contours: remaining,
        ..
    } = &removed.objects[&object].content
    else {
        unreachable!()
    };
    assert_eq!(remaining[0].points.len(), 2);
    assert_eq!(remaining[0].points[0].id, original_point);
    assert_eq!(remaining[0].points[1].id, end.id);
    assert_eq!(state.point, None);
    host.project.undo().unwrap();
    assert_eq!(
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap(),
        after
    );
}

#[test]
fn native_canvas_drag_previews_cancel_then_commit_one_undo_entry() {
    let (mut host, document, object, point) = setup();
    let mut state = state::State::default();
    let mut viewport = viewport::Viewport::default();
    let mut context = sdk::imgui::Context::create();
    viewport.initialize(&context);
    context.io_mut().set_config_input_trickle_event_queue(false);
    context.set_ini_filename(None::<String>).unwrap();
    sdk::typography::Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([600., 500.]);
    context.io_mut().set_delta_time(1. / 60.);
    let rect = sdk::ViewerRect {
        editable: true,
        image_current: true,
        canvas_origin: [10., 30.],
        canvas_size: [480., 450.],
        origin: [50., 100.],
        size: [300., 300.],
        dimensions: [100, 100],
    };
    for (frame, (mouse, down, escape)) in [
        ([500., 490.], false, false),
        ([500., 490.], false, false),
        ([110., 160.], true, false),
        ([140., 190.], true, false),
        ([140., 190.], true, true),
        ([140., 190.], false, false),
        ([110., 160.], true, false),
        ([140., 190.], true, false),
        ([140., 190.], true, false),
        ([140., 190.], false, false),
    ]
    .into_iter()
    .enumerate()
    {
        context.io_mut().add_mouse_pos_event(mouse);
        context
            .io_mut()
            .add_mouse_button_event(sdk::imgui::MouseButton::Left, down);
        context
            .io_mut()
            .add_key_event(sdk::imgui::Key::Escape, escape);
        let ui = context.frame();
        ui.window("Motion")
            .position([0.; 2], sdk::imgui::Condition::Always)
            .size([600., 500.], sdk::imgui::Condition::Always)
            .build(|| {
                state.sync(&mut host);
                viewport.draw(
                    ExtensionUi {
                        ui,
                        host: &mut host,
                    },
                    rect,
                    &mut state,
                );
            });
        drop(context.render_legacy());
        assert!(state.error.is_empty(), "{}", state.error);
        if frame == 2 {
            assert!(host.preview.is_none(), "a click selects without editing");
        }
        if frame == 8 {
            assert_eq!(
                host.preview_requests, 2,
                "stationary drag must not resubmit"
            );
        }
        if frame == 3 {
            assert_eq!(
                position(
                    &Canvas::from_document(&host.project.snapshot().state().documents[&document])
                        .unwrap(),
                    object
                ),
                [20., 20.]
            );
            let preview = host.preview.as_ref().unwrap().snapshot();
            assert_eq!(
                position(
                    &Canvas::from_document(&preview.state().documents[&document]).unwrap(),
                    object
                ),
                [30., 30.]
            );
        }
        if frame == 4 {
            assert!(host.preview.is_none());
            assert!(!state.editing);
        }
    }
    let after =
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
    assert_eq!(position(&after, object), [30., 30.]);
    let Content::Path { contours, .. } = &after.objects[&object].content else {
        unreachable!()
    };
    assert_eq!(contours[0].points[0].id, point);
    assert_eq!(contours[0].points[0].outgoing, [5., 0.]);
    host.project.undo().unwrap();
    assert_eq!(
        position(
            &Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap(),
            object
        ),
        [20., 20.]
    );
    host.project.redo().unwrap();
    assert_eq!(
        position(
            &Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap(),
            object
        ),
        [30., 30.]
    );
}

#[test]
fn graph_and_agent_share_objects_order_guards_and_undo() {
    use sdk::graph_canvas::{GraphChange, GraphContext, GraphEvent, Socket, WireView};
    let (mut host, document, path, _) = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let group;
    let script;
    {
        let mut context = graph::Context {
            state: &mut state,
            host: &mut host,
        };
        context
            .event(GraphEvent::Commit(vec![GraphChange::Add {
                template: "group".into(),
                position: [260., 0.],
            }]))
            .unwrap();
        group = context.state.selected[0];
        context
            .event(GraphEvent::Commit(vec![GraphChange::Add {
                template: "script".into(),
                position: [0., 180.],
            }]))
            .unwrap();
        script = context.state.selected[0];
        context
            .event(GraphEvent::Commit(vec![GraphChange::Connect(WireView {
                source: Socket {
                    node: path,
                    key: "content".into(),
                },
                target: Socket {
                    node: group,
                    key: "add".into(),
                },
            })]))
            .unwrap();
        assert_eq!(context.graph().nodes.len(), 2);
        assert!(context.graph().wires.is_empty());
        context.event(GraphEvent::Navigate(Some(group))).unwrap();
        let scoped = context.graph();
        assert_eq!(scoped.key.group, Some(group));
        assert!(scoped.has_parent);
        assert_eq!(scoped.nodes.len(), 2);
        assert!(!scoped.nodes.iter().any(|node| node.id == script));
        assert_eq!(scoped.wires[0].source.node, path);
        context.event(GraphEvent::Navigate(None)).unwrap();
        assert_eq!(context.graph().key.group, None);
        assert_eq!(context.graph().nodes.len(), 2);
    }
    let snapshot = host.project.snapshot();
    let original = &snapshot.state().documents[&document];
    let canvas = Canvas::from_document(original).unwrap();
    let mut object = canvas.objects[&script].clone();
    let count = Scalar::new(10_000.);
    if let Content::Script {
        source, controls, ..
    } = &mut object.content
    {
        *source = "pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {\n    Ok((0..input.number(\"Count\")? as usize).map(|_| fframes::svgr!(<circle r=\"1\"/>)).collect())\n}\n".into();
        controls.insert(
            ObjectId::new(),
            fold_motion_canvas::model::Control {
                name: "Count".into(),
                value: fold_motion_canvas::model::ControlValue::Number(count.clone()),
                protected: false,
                extensions: Default::default(),
            },
        );
    }
    let edited = host
        .registry
        .author(
            original,
            &[serde_json::to_value(Edit::Replace {
                id: script,
                expected: Box::new(canvas.objects[&script].clone()),
                object: Box::new(object),
            })
            .unwrap()],
        )
        .unwrap();
    host.project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(edited)],
        })
        .unwrap();
    state.sync(&mut host);
    let context = graph::Context {
        state: &mut state,
        host: &mut host,
    };
    let view = context.graph();
    assert_eq!(view.nodes.len(), 2);
    assert_eq!(view.nodes.iter().filter(|n| n.id == script).count(), 1);
    assert_eq!(
        view.nodes.iter().find(|n| n.id == script).unwrap().summary,
        "Script · 1 controls"
    );

    let snapshot = host.project.snapshot();
    let original = &snapshot.state().documents[&document];
    let mut value = count.clone();
    value.value = 8000.;
    let edited = host
        .registry
        .author(
            original,
            &[serde_json::to_value(Edit::Parameter {
                object: script,
                expected: count,
                value,
            })
            .unwrap()],
        )
        .unwrap();
    host.project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(edited)],
        })
        .unwrap();
    state.sync(&mut host);
    {
        let mut context = graph::Context {
            state: &mut state,
            host: &mut host,
        };
        assert_eq!(context.graph().nodes.len(), 2);
        context
            .event(GraphEvent::Commit(vec![GraphChange::Rewire {
                remove: vec![Socket {
                    node: group,
                    key: format!("child:{path}"),
                }],
                add: vec![WireView {
                    source: Socket {
                        node: script,
                        key: "content".into(),
                    },
                    target: Socket {
                        node: group,
                        key: format!("child:{path}"),
                    },
                }],
            }]))
            .unwrap();
        context.event(GraphEvent::Navigate(Some(group))).unwrap();
        assert_eq!(context.graph().wires[0].source.node, script);
        let before = context.host.snapshot().unwrap();
        assert!(
            context
                .event(GraphEvent::Commit(vec![
                    GraphChange::Add {
                        template: "ellipse".into(),
                        position: [0., 0.]
                    },
                    GraphChange::Connect(WireView {
                        source: Socket {
                            node: group,
                            key: "content".into()
                        },
                        target: Socket {
                            node: group,
                            key: "add".into()
                        }
                    })
                ]))
                .is_err()
        );
        assert_eq!(
            context.host.snapshot().unwrap().revision(),
            before.revision()
        );
    }
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(
        (graph::Context {
            state: &mut state,
            host: &mut host
        })
        .graph()
        .wires[0]
            .source
            .node,
        path
    );
    host.project.redo().unwrap();
    state.sync(&mut host);
    assert_eq!(
        (graph::Context {
            state: &mut state,
            host: &mut host
        })
        .graph()
        .wires[0]
            .source
            .node,
        script
    );

    // A protected object's graph location can change, with a precise stale guard.
    (graph::Context {
        state: &mut state,
        host: &mut host,
    })
    .event(GraphEvent::Navigate(None))
    .unwrap();
    let snapshot = host.project.snapshot();
    let mut canvas = Canvas::from_document(&snapshot.state().documents[&document]).unwrap();
    canvas.objects.get_mut(&path).unwrap().protected = true;
    host.project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(canvas.document(document).unwrap())],
        })
        .unwrap();
    state.sync(&mut host);
    {
        let mut context = graph::Context {
            state: &mut state,
            host: &mut host,
        };
        context
            .event(GraphEvent::PreviewPositions(vec![(path, [0., 0.])]))
            .unwrap();
        assert!(context.state.editing);
        context.event(GraphEvent::Commit(vec![])).unwrap();
        assert_eq!(
            context
                .graph()
                .nodes
                .iter()
                .find(|n| n.id == path)
                .unwrap()
                .position,
            Some([0., 0.])
        );
    }
    let snapshot = host.project.snapshot();
    let original = &snapshot.state().documents[&document];
    assert!(
        host.registry
            .author(
                original,
                &[serde_json::to_value(Edit::GraphPosition {
                    object: path,
                    expected: None,
                    value: Some([10., 20.])
                })
                .unwrap()]
            )
            .is_err()
    );
    assert!(
        host.registry
            .author(
                original,
                &[serde_json::to_value(Edit::Delete {
                    id: path,
                    expected: Box::new(
                        Canvas::from_document(original).unwrap().objects[&path].clone()
                    )
                })
                .unwrap()]
            )
            .is_err()
    );
    // Replacing several inputs retains their original drawing order, even if
    // the wire events arrive in another order and preceding slots are removed.
    let group = ObjectId::new();
    let [a, b, c, y, z] = std::array::from_fn(|_| ObjectId::new());
    let mut grouped = Canvas::default();
    grouped.roots = vec![group, y, z];
    grouped.objects.insert(
        group,
        Object::new(
            "Group",
            Content::Group {
                children: vec![a, b, c],
                extensions: Default::default(),
            },
        ),
    );
    for id in [a, b, c, y, z] {
        grouped.objects.insert(
            id,
            fold_motion_canvas::tools::new_object("rectangle").unwrap(),
        );
    }
    let (edits, _) = graph::changes(
        &grouped,
        &[GraphChange::Rewire {
            remove: vec![
                Socket {
                    node: group,
                    key: format!("child:{a}"),
                },
                Socket {
                    node: group,
                    key: format!("child:{b}"),
                },
            ],
            add: vec![
                WireView {
                    source: Socket {
                        node: y,
                        key: "content".into(),
                    },
                    target: Socket {
                        node: group,
                        key: format!("child:{b}"),
                    },
                },
                WireView {
                    source: Socket {
                        node: z,
                        key: "content".into(),
                    },
                    target: Socket {
                        node: group,
                        key: format!("child:{a}"),
                    },
                },
            ],
        }],
    )
    .unwrap();
    let rewired = fold_motion_canvas::editing::apply(&grouped, &edits).unwrap();
    assert!(
        matches!(&rewired.objects[&group].content,Content::Group { children, .. } if children == &vec![z,y,c])
    );
}

#[test]
fn group_graph_creation_uses_document_hierarchy_and_undo_clears_removed_scope() {
    use sdk::graph_canvas::{GraphChange, GraphContext, GraphEvent};
    let (mut host, _, path, _) = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let mut context = graph::Context {
        state: &mut state,
        host: &mut host,
    };
    context
        .event(GraphEvent::Commit(vec![GraphChange::Add {
            template: "group".into(),
            position: [260., 0.],
        }]))
        .unwrap();
    let group = context.state.selected[0];
    context.event(GraphEvent::Navigate(Some(group))).unwrap();
    context
        .event(GraphEvent::Commit(vec![GraphChange::Add {
            template: "ellipse".into(),
            position: [0., 0.],
        }]))
        .unwrap();
    let child = context.state.selected[0];
    let canvas = context.state.canvas().unwrap();
    assert!(!canvas.roots.contains(&child));
    assert_eq!(
        fold_motion_canvas::hierarchy::children(canvas, Some(group)).unwrap(),
        &[child]
    );
    assert_eq!(context.graph().nodes.len(), 2);
    context.event(GraphEvent::Navigate(None)).unwrap();
    // Explicit graph navigation stays where requested until another surface
    // changes the selection. Canvas/outliner selection reveals its owner group.
    assert_eq!(context.state.graph_group, None);
    state.select(&mut host, vec![child]);
    assert_eq!(state.graph_group, Some(group));
    state.select(&mut host, vec![group]);
    assert_eq!(state.graph_group, Some(group));
    host.state.selection.objects = vec![path];
    state.sync(&mut host);
    assert_eq!(state.graph_group, None);
    host.state.selection.objects = vec![child];
    state.sync(&mut host);
    assert_eq!(state.graph_group, Some(group));
    state.select(&mut host, vec![]);
    assert_eq!(state.graph_group, Some(group));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.graph_group, Some(group));
    assert!(!state.canvas().unwrap().objects.contains_key(&child));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.graph_group, None);
}

#[test]
fn point_target_survives_native_revision_and_clears_with_removed_selection() {
    let (mut host, document, object, point) = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    state.point = Some((object, point));
    let mut changed = state.canvas().unwrap().clone();
    changed.objects.get_mut(&object).unwrap().name = "Renamed curve".into();
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(changed.document(document).unwrap())],
        })
        .unwrap();
    state.sync(&mut host);
    assert_eq!(state.point, Some((object, point)));
    let deletion = Edit::Delete {
        id: object,
        expected: Box::new(changed.objects[&object].clone()),
    };
    host.project
        .commit(
            fold_motion_canvas::editing::stage(&host.project.snapshot(), document, &[deletion])
                .unwrap(),
        )
        .unwrap();
    state.sync(&mut host);
    assert_eq!(state.point, None);
    host.project.undo().unwrap();
    state.sync(&mut host);
    state.point = Some((object, point));
    host.state.selection.objects.clear();
    state.sync(&mut host);
    assert_eq!(state.point, None);
}

#[test]
fn numeric_point_controls_create_coupled_tracks_and_keep_identity_and_pose() {
    use fold_graph::{
        binding::{Interpolation, Key, Track},
        types::{Literal, Space, Unit},
    };
    let time = Time::new(1, 2).unwrap();
    let mut x = fold_animation::Curve::default();
    x.insert(time, 20.);
    let (base, created) =
        inspector::edited_point_property([4., 6.], None, [4., 6.], [Some(x), None], time).unwrap();
    assert_eq!(base, [4., 6.]);
    let created = created.unwrap();
    let vector = |value: [f64; 2]| Literal::Vector {
        value: value.to_vec(),
        unit: Unit::Canvas,
        space: Space::Screen,
    };
    assert_eq!(created.sample(time).unwrap(), vector([20., 6.]));
    let mut track = Track::constant(vector([0., 0.]), Time::ZERO).unwrap();
    track.keys.push(Key {
        id: ObjectId::new(),
        time: Time::new(1, 1).unwrap(),
        value: vector([20., 40.]),
        interpolation: Interpolation::Linear,
        handles: vec![],
        extensions: Default::default(),
    });
    track
        .extensions
        .insert("vendor.track".into(), serde_json::json!(true));
    track.keys[1]
        .extensions
        .insert("vendor.key".into(), serde_json::json!(true));
    let curves = fold_graph::curves::curves(&track).unwrap();
    let mut x = curves[0].1.clone();
    x.keys[1].value = 30.;
    let (_, edited) = inspector::edited_point_property(
        [4., 6.],
        Some(&track),
        [4., 6.],
        [Some(x), Some(curves[1].1.clone())],
        time,
    )
    .unwrap();
    let edited = edited.unwrap();
    assert_eq!(edited.sample(time).unwrap(), vector([15., 20.]));
    assert_eq!(edited.id, track.id);
    assert_eq!(edited.keys[1].id, track.keys[1].id);
    assert_eq!(edited.extensions, track.extensions);
    assert_eq!(edited.keys[1].extensions, track.keys[1].extensions);
    let (pose, cleared) = inspector::edited_point_property(
        [4., 6.],
        Some(&edited),
        [4., 6.],
        [None, Some(curves[1].1.clone())],
        time,
    )
    .unwrap();
    assert_eq!(pose, [15., 20.]);
    assert!(cleared.is_none());
    assert_eq!(
        inspector::edited_point_property([4., 6.], None, [8., 10.], [None, None], time).unwrap(),
        ([8., 10.], None)
    );
}

#[test]
fn numeric_point_preview_cancel_and_commit_share_one_native_undo() {
    let (mut host, document, object, point) = setup();
    host.state.navigation.last_mut().unwrap().time = Time::new(1, 2).unwrap();
    let mut state = state::State::default();
    state.sync(&mut host);
    state.point = Some((object, point));
    let original = state.canvas().unwrap().clone();
    let expected = original.objects[&object].clone();
    let time = state.time;
    let mut next = expected.clone();
    let Content::Path { contours, .. } = &mut next.content else {
        unreachable!()
    };
    let target = &mut contours[0].points[0];
    let mut x = fold_animation::Curve::default();
    x.insert(time, 24.);
    let (base, track) = inspector::edited_point_property(
        target.outgoing,
        None,
        target.outgoing,
        [Some(x), None],
        time,
    )
    .unwrap();
    target.outgoing = base;
    target.outgoing_track = track;
    let track_id = target.outgoing_track.as_ref().unwrap().id;
    let key_id = target.outgoing_track.as_ref().unwrap().keys[0].id;
    let replacement = |next: &Object| Edit::Replace {
        id: object,
        expected: Box::new(expected.clone()),
        object: Box::new(next.clone()),
    };
    state.preview(&mut host, vec![replacement(&next)]).unwrap();
    assert_eq!(
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap(),
        original
    );
    state.cancel(&mut host);
    assert!(host.preview.is_none());
    assert_eq!(state.canvas().unwrap(), &original);
    state.preview(&mut host, vec![replacement(&next)]).unwrap();
    let Content::Path { contours, .. } = &mut next.content else {
        unreachable!()
    };
    let target = &mut contours[0].points[0];
    let curves = fold_graph::curves::curves(target.outgoing_track.as_ref().unwrap()).unwrap();
    let mut x = curves[0].1.clone();
    x.keys[0].value = 32.;
    let (base, track) = inspector::edited_point_property(
        target.outgoing,
        target.outgoing_track.as_ref(),
        target.outgoing,
        [Some(x), Some(curves[1].1.clone())],
        time,
    )
    .unwrap();
    target.outgoing = base;
    target.outgoing_track = track;
    assert_eq!(target.outgoing_track.as_ref().unwrap().id, track_id);
    assert_eq!(target.outgoing_track.as_ref().unwrap().keys[0].id, key_id);
    state.preview(&mut host, vec![replacement(&next)]).unwrap();
    assert_eq!(
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap(),
        original
    );
    state.commit(&mut host).unwrap();
    let committed = host.project.snapshot().state().documents[&document].clone();
    assert_eq!(
        Canvas::from_document(&committed).unwrap().objects[&object],
        next
    );
    assert_eq!(state.point, Some((object, point)));
    host.project.undo().unwrap();
    assert_eq!(
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap(),
        original
    );
    host.project.redo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents[&document],
        committed
    );
}

#[test]
fn removing_last_anchor_keeps_path_identity_and_undo_restores_it() {
    let (mut host, document, object, point) = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    state.point = Some((object, point));
    let original = state.canvas().unwrap().clone();
    let removal = Edit::RemovePoint {
        object,
        expected: Box::new(original.objects[&object].clone()),
        point,
    };
    let after =
        fold_motion_canvas::editing::apply(&original, std::slice::from_ref(&removal)).unwrap();
    let Content::Path { contours, .. } = &after.objects[&object].content else {
        unreachable!()
    };
    assert!(contours[0].points.is_empty());
    assert_eq!(
        after.objects[&object].paint,
        original.objects[&object].paint
    );
    assert_eq!(after.roots, original.roots);
    let mut stale = original.clone();
    stale.objects.get_mut(&object).unwrap().name = "Changed".into();
    assert!(fold_motion_canvas::editing::apply(&stale, std::slice::from_ref(&removal)).is_err());
    let mut protected = original.clone();
    protected.objects.get_mut(&object).unwrap().protected = true;
    let locked = Edit::RemovePoint {
        object,
        expected: Box::new(protected.objects[&object].clone()),
        point,
    };
    assert!(fold_motion_canvas::editing::apply(&protected, &[locked]).is_err());
    state.preview(&mut host, vec![removal]).unwrap();
    state.commit(&mut host).unwrap();
    assert_eq!(state.point, None);
    host.project.undo().unwrap();
    assert_eq!(
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap(),
        original
    );
}

#[test]
fn implot_handles_preserve_length_and_respond_before_render_completion() {
    let (mut host, document, object, point) = setup();
    let original = host.project.snapshot();
    let shared = Shared::default();
    let mut inspector = inspector::Inspector {
        state: shared.clone(),
        sources: Default::default(),
        names: Default::default(),
    };
    let mut state = shared.borrow_mut();
    state.sync(&mut host);
    state.point = Some((object, point));
    drop(state);
    let mut viewport = viewport::Viewport::default();
    let mut context = sdk::imgui::Context::create();
    viewport.initialize(&context);
    context.io_mut().set_config_input_trickle_event_queue(false);
    context.set_ini_filename(None::<String>).unwrap();
    sdk::typography::Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 900.]);
    context.io_mut().set_delta_time(1. / 60.);
    let rect = sdk::ViewerRect {
        editable: true,
        // The control layer must remain usable while artwork catches up.
        image_current: false,
        canvas_origin: [10., 30.],
        canvas_size: [480., 450.],
        origin: [50., 100.],
        size: [300., 300.],
        dimensions: [100, 100],
    };
    for (mouse, down) in [
        ([500., 490.], false),
        ([500., 490.], false),
        ([125., 160.], true),
        ([155., 190.], true),
        ([155., 190.], true),
        ([155., 190.], false),
    ] {
        context.io_mut().add_mouse_pos_event(mouse);
        context
            .io_mut()
            .add_mouse_button_event(sdk::imgui::MouseButton::Left, down);
        let ui = context.frame();
        ui.window("Inspector")
            .position([600., 0.], sdk::imgui::Condition::Always)
            .size([400., 900.], sdk::imgui::Condition::Always)
            .build(|| {
                inspector.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                });
            });
        ui.window("Motion")
            .position([0.; 2], sdk::imgui::Condition::Always)
            .size([600., 500.], sdk::imgui::Condition::Always)
            .build(|| {
                let mut state = shared.borrow_mut();
                state.sync(&mut host);
                viewport.draw(
                    ExtensionUi {
                        ui,
                        host: &mut host,
                    },
                    rect,
                    &mut state,
                );
            });
        drop(context.render_legacy());
        let state = shared.borrow();
        assert!(state.error.is_empty(), "{}", state.error);
    }
    assert_eq!(host.preview_requests, 1);
    let canvas =
        Canvas::from_document(&host.project.snapshot().state().documents[&document]).unwrap();
    let Content::Path { contours, .. } = &canvas.objects[&object].content else {
        unreachable!()
    };
    let p = &contours[0].points[0];
    assert_eq!(p.position, [20., 20.]);
    assert_eq!(p.outgoing, [15., 10.]);
    assert!((p.incoming[0].hypot(p.incoming[1]) - 5.).abs() < 1e-9);
    assert!((p.incoming[0] * p.outgoing[1] - p.incoming[1] * p.outgoing[0]).abs() < 1e-9);
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents[&document].payload,
        original.state().documents[&document].payload
    );
}
