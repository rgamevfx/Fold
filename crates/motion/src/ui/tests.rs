use super::*;
use crate::{Motion, authoring as a, fields::Datum};
use fold_foundation::{DocumentId, Time};
use fold_platform::{desktop::*, packages::PackageRegistry};
use fold_project::{CommittedSnapshot, Project};
use fold_ui::sdk::{ExtensionUi, Panel, imgui};
pub(super) struct Host {
    pub(super) project: Project,
    state: DesktopState,
    registry: PackageRegistry,
}
impl Host {
    pub(super) fn new() -> Self {
        let mut registry = PackageRegistry::default();
        crate::package::register(&mut registry).unwrap();
        Self {
            project: Project::new(32),
            state: DesktopState::default(),
            registry,
        }
    }
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn video_info(&self, document: DocumentId) -> Result<fold_media::VideoInfo, String> {
        self.registry.output(&self.project.snapshot(), document)
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        match command {
            DesktopCommand::Extension(request) => {
                let batch = self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .unwrap();
                self.project.commit(batch).unwrap();
                self.state.transient = false;
            }
            DesktopCommand::PreviewExtension(request) => {
                let batch = self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .unwrap();
                let mut session = self.project.begin_edit();
                self.project
                    .update_edit(&mut session, batch.mutations)
                    .unwrap();
                self.state.transient = true;
            }
            DesktopCommand::CancelPreviewEdit => self.state.transient = false,
            DesktopCommand::Navigate(location) => {
                self.state.selection.document = Some(location.document);
                self.state.navigation = vec![location];
            }
            DesktopCommand::Select(selection) => self.state.selection = selection,
            other => panic!("unexpected test command {other:?}"),
        }
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
#[test]
fn direct_ui_edits_are_one_transaction_and_group_edits_preserve_root_output() {
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    let id = state.document.unwrap();
    state.change(&mut host, |m| {
        a::add_content(m, "fold.motion.rectangle").map(Some)
    });
    let rectangle = state.selected[0];
    let root_output = state.motion.as_ref().unwrap().graph.output.clone();
    let before = host.project.snapshot();
    for width in [120., 130., 140.] {
        a::node(state.motion.as_mut().unwrap(), rectangle)
            .unwrap()
            .set("width", Datum::Scalar(width));
        state.preview(&mut host);
        assert_eq!(host.project.snapshot().revision(), before.revision());
    }
    state.commit(&mut host);
    assert_eq!(
        host.project.snapshot().revision().0,
        before.revision().0 + 1
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    host.project.redo().unwrap();
    state.sync(&mut host);
    state.change(&mut host, |m| a::group_node(m, rectangle).map(Some));
    let instance = state.selected[0];
    let group = state
        .motion
        .as_ref()
        .unwrap()
        .graph
        .node(instance)
        .unwrap()
        .settings::<crate::nodes::interface::GroupSettings>()
        .unwrap()
        .group
        .unwrap();
    state.group = Some(group);
    let mut view = state.view_motion().unwrap();
    assert!(view.graph.node(rectangle).is_ok());
    a::node(&mut view, rectangle).unwrap().settings["future-note"] = serde_json::json!("keep");
    state.replace_view(view);
    state.commit(&mut host);
    let saved = Motion::from_document(&host.project.snapshot().state().documents[&id]).unwrap();
    assert_eq!(saved.graph.output, root_output);
    assert_eq!(
        saved.groups[&group].graph.node(rectangle).unwrap().settings["future-note"],
        "keep"
    );
    assert!(crate::evaluation::compile(&saved, Time::ZERO, 64, 64).is_ok());
}

#[test]
fn network_keeps_modifier_graph_and_does_not_handle_keyframe_shortcut() {
    use fold_ui::sdk::graph_canvas::{GraphContext, GraphEvent};
    let _guard = super::IMGUI_TEST_LOCK.lock().unwrap();
    let mut host = Host::new();
    let shared = state::Shared::default();
    let (group, rectangle) = {
        let mut state = shared.borrow_mut();
        state.create(&mut host);
        state.change(&mut host, |m| {
            a::add_content(m, "fold.motion.rectangle").map(Some)
        });
        let rectangle = state.selected[0];
        state.change(&mut host, |m| a::group_node(m, rectangle).map(Some));
        let instance = state.selected[0];
        graph::Context {
            state: &mut state,
            host: &mut host,
        }
        .event(GraphEvent::Navigate(Some(instance)))
        .unwrap();
        state.change(&mut host, |m| {
            let node = crate::authoring::node(m, rectangle)?;
            node.set("width", Datum::Scalar(120.));
            let mut curve = fold_animation::Curve::default();
            curve.insert(Time::ZERO, 120.);
            node.animation.insert("width.0".into(), curve);
            Ok(Some(rectangle))
        });
        (state.group.unwrap(), rectangle)
    };
    let before = host.project.snapshot();
    let document = shared.borrow().document.unwrap();
    let root = Motion::from_document(&before.state().documents[&document]).unwrap();
    host.state.navigation.last_mut().unwrap().time = Time::new(1, 2).unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([900., 600.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = canvas::Canvas {
        state: shared.clone(),
        files: Default::default(),
        canvas: Default::default(),
        overlay: Default::default(),
    };
    for frame in 0..6 {
        context.io_mut().add_key_event(imgui::Key::I, frame == 3);
        let ui = context.frame();
        ui.window("Composition")
            .focused(true)
            .size([850., 500.], imgui::Condition::Always)
            .build(|| {
                canvas.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                });
            });
        context.end_frame();
    }
    let after = host.project.snapshot();
    assert_eq!(after.revision(), before.revision());
    let edited = Motion::from_document(&after.state().documents[&document]).unwrap();
    assert_eq!(edited.graph, root.graph);
    assert_eq!(
        edited.groups[&group]
            .graph
            .node(rectangle)
            .unwrap()
            .animation["width.0"]
            .keys
            .len(),
        1
    );
}

#[test]
fn nested_composition_opens_at_source_time_and_back_restores_parent_time() {
    use fold_platform::{panel_context::PanelContext, workspace::Workspace};
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| {
        a::scene::create_object(m, "fold.motion.rectangle").map(Some)
    });
    let parent = state.document.unwrap();
    super::composition::nest(&mut state, &mut host, false);
    let layer = state.selected[0];
    let child = state
        .motion
        .as_ref()
        .unwrap()
        .graph
        .node(layer)
        .unwrap()
        .settings::<crate::nodes::composition::Settings>()
        .unwrap()
        .source
        .unwrap()
        .document;
    state.change_scene(&mut host, |m| {
        a::composition::set_range(m, layer, Time::new(1, 1).unwrap(), Time::new(6, 1).unwrap())?;
        Ok(Some(layer))
    });
    let parent_time = Time::new(7, 6).unwrap();
    let mut workspace = Workspace::default();
    let id = workspace.add_editor(crate::package::PANEL, crate::MOTION);
    let mut editor = workspace.editors.remove(&id).unwrap();
    editor.bind(ViewLocation {
        document: parent,
        time: parent_time,
        label: "Parent".into(),
    });
    {
        let mut context = PanelContext::new(&mut host, &mut editor);
        super::composition::open(&mut state, &mut context, layer).unwrap();
    }
    assert_eq!(editor.document(), Some(child));
    assert_eq!(editor.navigation.len(), 2);
    assert_eq!(
        editor.navigation.last().unwrap().time,
        Time::new(1, 6).unwrap()
    );
    assert_eq!(state.document, Some(child));
    {
        let mut context = PanelContext::new(&mut host, &mut editor);
        context.command(DesktopCommand::NavigateBack);
        state.sync(&mut context);
    }
    assert_eq!(editor.document(), Some(parent));
    assert_eq!(editor.navigation.last().unwrap().time, parent_time);
    assert_eq!(state.document, Some(parent));
}
#[test]
fn drawing_motion_graph_inspector_and_handles_does_not_mutate_project() {
    let _guard = super::IMGUI_TEST_LOCK.lock().unwrap();
    let mut host = Host::new();
    let state = std::rc::Rc::new(std::cell::RefCell::new(state::State::default()));
    state.borrow_mut().create(&mut host);
    state.borrow_mut().change(&mut host, |m| {
        a::scene::create_object(m, "fold.motion.text").map(Some)
    });
    let revision = host.project.snapshot().revision();
    let mut context = imgui::Context::create();
    let typography = fold_ui::sdk::typography::Typography::install(&mut context);
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 900.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = canvas::Canvas {
        state: state.clone(),
        files: Default::default(),
        canvas: Default::default(),
        overlay: Default::default(),
    };
    canvas.initialize(&context, Some(&typography));
    let mut inspector = inspector::Inspector {
        state: state.clone(),
    };
    for frame in 0..24 {
        let maximized = frame >= 8;
        let narrow = frame >= 16;
        let graph_width = if narrow { 170. } else { 850. };
        context
            .io_mut()
            .add_mouse_pos_event([graph_width - 20., 800.]);
        context.io_mut().set_display_size(if maximized {
            [1920., 1080.]
        } else {
            [1200., 900.]
        });
        let ui = context.frame();
        ui.window("Motion graph")
            .position([0., 300.], imgui::Condition::Always)
            .size([graph_width, 600.], imgui::Condition::Always)
            .build(|| {
                canvas.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                });
                if frame % 8 >= 3 {
                    assert_eq!(ui.scroll_max_x(), 0., "toolbar must fit the panel");
                }
            });
        ui.window("Motion properties")
            .position([850., 0.], imgui::Condition::Always)
            .size([350., 900.], imgui::Condition::Always)
            .build(|| {
                inspector.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                })
            });
        // Match the shell's two-pass viewer: image first, then append overlay
        // items to the same window. The saved cursor is below the image.
        let mut rect = None;
        ui.window("Viewer")
            .position([0., 0.], imgui::Condition::Always)
            .size(
                if maximized {
                    [1500., 600.]
                } else {
                    [850., 300.]
                },
                imgui::Condition::Always,
            )
            .build(|| {
                let size = ui.content_region_avail();
                rect = Some(fold_ui::sdk::ViewerRect {
                    editable: true,
                    image_current: true,
                    canvas_origin: [0., 0.],
                    canvas_size: [640., 480.],
                    origin: ui.cursor_screen_pos(),
                    size,
                    dimensions: [640, 240],
                });
                ui.dummy(size); // Same layout advance as the preview image.
            });
        ui.window("Viewer").build(|| {
            canvas.draw_viewer_overlay(
                ExtensionUi {
                    ui,
                    host: &mut host,
                },
                rect.unwrap(),
            )
        });
        assert!(context.render_legacy().total_vtx_count() > 100);
    }
    assert_eq!(host.project.snapshot().revision(), revision);
    assert_eq!(host.project.snapshot().state().documents.len(), 1);
    let _: DocumentId = state.borrow().document.unwrap();

    // A popup may disappear before its custom numeric field can report
    // deactivation. Its preview still commits once, without quantizing a
    // continuous alignment to the three named presets.
    let before = host.project.snapshot();
    {
        let mut state = state.borrow_mut();
        let selected = state.selected[0];
        let mut motion = state.view_motion().unwrap();
        a::node(&mut motion, selected)
            .unwrap()
            .set("alignment", Datum::Scalar(0.375));
        state.property_editing = true;
        state.replace_view(motion);
        state.preview(&mut host);
    }
    assert_eq!(host.project.snapshot().revision(), before.revision());
    let ui = context.frame();
    ui.window("Motion properties").build(|| {
        inspector.draw(ExtensionUi {
            ui,
            host: &mut host,
        })
    });
    drop(context.render_legacy());
    assert_eq!(
        host.project.snapshot().revision().0,
        before.revision().0 + 1
    );
    assert!(!state.borrow().editing);
    let saved = Motion::from_document(
        &host.project.snapshot().state().documents[&state.borrow().document.unwrap()],
    )
    .unwrap();
    assert_eq!(
        saved.graph.node(state.borrow().selected[0]).unwrap().inputs["alignment"],
        crate::graph::Input::Value(Datum::Scalar(0.375))
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn shared_editor_events_keep_motion_batches_atomic_and_selection_plural() {
    use fold_ui::sdk::graph_canvas::*;
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change(&mut host, |m| {
        a::add_content(m, "fold.motion.rectangle").map(Some)
    });
    let rectangle = state.selected[0];
    let output = state.motion.as_ref().unwrap().graph.output.node;
    let before = host.project.snapshot();
    let mut adapter = graph::Context {
        state: &mut state,
        host: &mut host,
    };
    adapter
        .event(GraphEvent::Select(vec![rectangle, output]))
        .unwrap();
    assert_eq!(adapter.state.selected, vec![rectangle, output]);
    assert_eq!(
        adapter.host.state().selection.objects,
        vec![rectangle, output]
    );
    assert!(
        adapter
            .validate(&GraphChange::Delete(vec![output]))
            .is_err()
    );
    let original = adapter.state.motion.clone();
    // Removing a normal node first must not leak if a later deletion is illegal.
    assert!(
        adapter
            .event(GraphEvent::Commit(vec![GraphChange::Delete(vec![
                rectangle, output
            ])]))
            .is_err()
    );
    assert_eq!(adapter.state.motion, original);
    let invalid = GraphChange::Connect(WireView {
        source: Socket {
            node: rectangle,
            key: "content".into(),
        },
        target: Socket {
            node: rectangle,
            key: "width".into(),
        },
    });
    assert!(adapter.validate(&invalid).is_err());
    assert_eq!(adapter.state.motion, original);
    for x in [10., 20., 30.] {
        adapter
            .event(GraphEvent::PreviewPositions(vec![(rectangle, [x, 40.])]))
            .unwrap();
    }
    assert_eq!(
        adapter.host.snapshot().unwrap().revision(),
        before.revision()
    );
    adapter.event(GraphEvent::Cancel).unwrap();
    assert_eq!(adapter.state.motion, original);
    adapter
        .event(GraphEvent::PreviewPositions(vec![(rectangle, [50., 60.])]))
        .unwrap();
    adapter.event(GraphEvent::Commit(vec![])).unwrap();
    assert_eq!(
        adapter.host.snapshot().unwrap().revision().0,
        before.revision().0 + 1
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    state.change(&mut host, |m| a::group_node(m, rectangle).map(Some));
    let instance = state.selected[0];
    let root_output = state.motion.as_ref().unwrap().graph.output.clone();
    let before = host.project.snapshot();
    let mut adapter = graph::Context {
        state: &mut state,
        host: &mut host,
    };
    let root_key = adapter.graph().key;
    adapter.event(GraphEvent::Navigate(Some(instance))).unwrap();
    assert_ne!(adapter.graph().key, root_key);
    assert!(adapter.graph().has_parent);
    assert!(adapter.state.selected.is_empty());
    assert!(adapter.graph().nodes.iter().any(|n| n.id == rectangle));
    assert_eq!(
        adapter.host.snapshot().unwrap().revision(),
        before.revision()
    );
    adapter
        .event(GraphEvent::Commit(vec![GraphChange::Positions(vec![(
            rectangle,
            [70., 80.],
        )])]))
        .unwrap();
    assert_eq!(
        adapter.state.motion.as_ref().unwrap().graph.output,
        root_output
    );
    adapter.event(GraphEvent::Navigate(None)).unwrap();
    assert_eq!(adapter.graph().key, root_key);
    assert_eq!(
        adapter.host.snapshot().unwrap().revision().0,
        before.revision().0 + 1
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn scene_modifier_gestures_commit_once_and_cancel_restores_authored_scene() {
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| {
        a::scene::create_object(m, "fold.motion.rectangle").map(Some)
    });
    let object = state.selected[0];
    state.change_scene(&mut host, |m| {
        let group = a::scene::new_modifier_definition(m, true)?;
        a::scene::add_modifier(m, object, group).map(Some)
    });
    let modifier = state.selected[0];
    let before = host.project.snapshot();
    for radius in [40., 90., 120.] {
        a::node(state.motion.as_mut().unwrap(), modifier)
            .unwrap()
            .set("radius", Datum::Scalar(radius));
        state.preview(&mut host);
        assert_eq!(host.project.snapshot().revision(), before.revision());
    }
    state.commit(&mut host);
    assert_eq!(
        host.project.snapshot().revision().0,
        before.revision().0 + 1
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    state.sync(&mut host);
    let original = state.motion.clone();
    state
        .motion
        .as_mut()
        .unwrap()
        .scene
        .as_mut()
        .unwrap()
        .objects[0]
        .visible = false;
    state.preview(&mut host);
    state.cancel(&mut host);
    assert_eq!(state.motion, original);
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn duplicator_creation_and_wave_toggle_are_separate_undoable_edits() {
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| a::scene::assets::badge(m).map(Some));
    let source = state.selected[0];
    state.change_scene(&mut host, |m| {
        a::scene::assets::duplicate(m, source).map(Some)
    });
    let duplicate = state.selected[0];
    assert!(
        !state
            .motion
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .objects
            .iter()
            .find(|o| o.id == source)
            .unwrap()
            .visible
    );
    state.change_scene(&mut host, |m| {
        a::scene::assets::attach_copy_wave(m, duplicate)?;
        Ok(Some(duplicate))
    });
    let driver =
        a::scene::assets::copy_driver(state.motion.as_ref().unwrap(), duplicate, "offset_y")
            .unwrap();
    assert!(state.motion.as_ref().unwrap().graph.node(driver).is_ok());
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(
        state
            .motion
            .as_ref()
            .unwrap()
            .graph
            .node(duplicate)
            .unwrap()
            .inputs["offset_y"],
        crate::graph::Input::Value(Datum::Scalar(0.))
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert!(
        state
            .motion
            .as_ref()
            .unwrap()
            .graph
            .node(duplicate)
            .is_err()
    );
    assert!(
        state
            .motion
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .objects
            .iter()
            .find(|o| o.id == source)
            .unwrap()
            .visible
    );
}

#[test]
fn exposed_count_socket_and_new_group_input_survive_reload_and_undo() {
    use fold_ui::sdk::graph_canvas::GraphContext;
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| {
        let source = a::scene::assets::badge(m)?;
        a::scene::assets::duplicate(m, source).map(Some)
    });
    let id = state.selected[0];
    state.network_object = Some(id);
    let group = state
        .motion
        .as_ref()
        .unwrap()
        .graph
        .node(id)
        .unwrap()
        .settings::<crate::nodes::interface::GroupSettings>()
        .unwrap()
        .group
        .unwrap();
    let before = host.project.snapshot();
    a::node(state.motion.as_mut().unwrap(), id)
        .unwrap()
        .extensions
        .insert(
            "fold.motion.input_visibility".into(),
            serde_json::json!({"count":true}),
        );
    state.commit(&mut host);
    let view = graph::Context {
        state: &mut state,
        host: &mut host,
    }
    .graph();
    assert!(
        view.nodes
            .iter()
            .find(|n| n.id == id)
            .unwrap()
            .inputs
            .iter()
            .any(|p| p.key == "count")
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    host.project.redo().unwrap();
    state.sync(&mut host);
    let before = host.project.snapshot();
    let motion = state.motion.as_mut().unwrap();
    let input = a::add_group_input(motion, group, Datum::Scalar(3.)).unwrap();
    let key = motion.groups[&group].inputs.last().unwrap().id.clone();
    motion
        .groups
        .get_mut(&group)
        .unwrap()
        .inputs
        .last_mut()
        .unwrap()
        .name = "Energy".into();
    state.commit(&mut host);
    assert!(state.error.is_empty(), "{}", state.error);
    let saved =
        Motion::from_document(&host.project.snapshot().state().documents[&state.document.unwrap()])
            .unwrap();
    let definition = &saved.groups[&group];
    assert_eq!(definition.inputs.last().unwrap().name, "Energy");
    assert_eq!(definition.graph.node(input).unwrap().settings["key"], key);
    assert!(
        saved
            .signature(saved.graph.node(id).unwrap())
            .unwrap()
            .0
            .iter()
            .any(|p| p.id == key)
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn oscillator_creation_is_one_undoable_connection() {
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| {
        let source = a::scene::assets::badge(m)?;
        a::scene::assets::duplicate(m, source).map(Some)
    });
    let target = state.selected[0];
    let before = host.project.snapshot();
    state.change(&mut host, |m| {
        a::scene::assets::attach_oscillator(m, target, "count").map(Some)
    });
    assert!(state.error.is_empty(), "{}", state.error);
    let driver = state.selected[0];
    let motion = state.motion.as_ref().unwrap();
    assert_eq!(
        a::scene::assets::copy_driver(motion, target, "count"),
        Some(driver)
    );
    let after = host.project.snapshot();
    assert_eq!(after.revision().0, before.revision().0 + 1);
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    host.project.redo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        after.state().documents
    );
}

#[test]
fn color_ramp_wiring_and_stop_edits_are_undoable_and_persistent() {
    use crate::nodes::color_ramp::{Interpolation, Settings, stop};
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| {
        let source = a::scene::assets::badge(m)?;
        a::scene::assets::duplicate(m, source).map(Some)
    });
    let target = state.selected[0];
    let before = host.project.snapshot();
    state.change(&mut host, |m| {
        a::scene::assets::attach_color_ramp(m, target, "color").map(Some)
    });
    assert!(state.error.is_empty(), "{}", state.error);
    let ramp = state.selected[0];
    let wired = host.project.snapshot();
    state.change(&mut host, |m| {
        let mut first = stop(0., [1., 0., 0., 0.25]);
        first
            .extensions
            .insert("future".into(), serde_json::json!("keep"));
        a::scene::assets::set_color_ramp(
            m,
            ramp,
            Settings {
                stops: vec![
                    first,
                    stop(0.4, [0., 1., 0., 0.5]),
                    stop(1., [0., 0., 1., 1.]),
                ],
                interpolation: Interpolation::Stepped,
            },
        )?;
        Ok(Some(ramp))
    });
    assert!(state.error.is_empty(), "{}", state.error);
    let saved =
        Motion::from_document(&host.project.snapshot().state().documents[&state.document.unwrap()])
            .unwrap();
    let group = saved
        .graph
        .node(ramp)
        .unwrap()
        .settings::<crate::nodes::interface::GroupSettings>()
        .unwrap()
        .group
        .unwrap();
    let settings = saved.groups[&group]
        .graph
        .node(ramp)
        .unwrap()
        .settings::<Settings>()
        .unwrap();
    assert_eq!(settings.stops.len(), 3);
    assert_eq!(settings.stops[0].extensions["future"], "keep");
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        wired.state().documents
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn scene_drop_is_one_undoable_edit() {
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| a::scene::assets::badge(m).map(Some));
    let badge = state.selected[0];
    state.change_scene(&mut host, |m| {
        a::scene::create_object(m, "fold.motion.path").map(Some)
    });
    let path = state.selected[0];
    let before = host.project.snapshot();
    state.change_scene(&mut host, |m| {
        a::scene::place_object(m, path, Some(badge), None, Time::ZERO)?;
        Ok(Some(path))
    });
    assert!(state.error.is_empty(), "{}", state.error);
    assert_eq!(
        state
            .motion
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .objects
            .iter()
            .find(|o| o.id == path)
            .unwrap()
            .parent,
        Some(badge)
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn drifting_shapes_action_and_solver_graph_edits_share_undo_history() {
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| {
        a::scene::create_object(m, "fold.motion.rectangle").map(Some)
    });
    let source = state.selected[0];
    let before = host.project.snapshot();
    state.change_scene(&mut host, |m| a::drift::from_object(m, source).map(Some));
    assert!(state.error.is_empty(), "{}", state.error);
    let created = host.project.snapshot();
    assert_eq!(created.revision().0, before.revision().0 + 1);
    let owner = state.selected[0];
    let root_output = state.motion.as_ref().unwrap().graph.output.clone();
    let outer = state
        .motion
        .as_ref()
        .unwrap()
        .graph
        .node(owner)
        .unwrap()
        .settings::<crate::nodes::interface::GroupSettings>()
        .unwrap()
        .group
        .unwrap();
    state.group = Some(outer);
    let solver = state
        .view_motion()
        .unwrap()
        .graph
        .nodes
        .iter()
        .find(|n| n.kind == "fold.motion.solver")
        .unwrap()
        .id;
    let update = state
        .view_motion()
        .unwrap()
        .graph
        .node(solver)
        .unwrap()
        .settings::<crate::nodes::solver::Settings>()
        .unwrap()
        .group
        .unwrap();
    state.group = Some(update);
    state.change(&mut host, |m| {
        a::add_node(m, "fold.motion.read_attribute").map(Some)
    });
    assert!(state.error.is_empty(), "{}", state.error);
    assert_eq!(state.motion.as_ref().unwrap().graph.output, root_output);
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        created.state().documents
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    host.project.redo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        created.state().documents
    );
}

#[test]
fn embedded_font_document_supports_preview_cancel_commit_and_undo() {
    let mut host = Host::new();
    let id = DocumentId::new();
    let mut motion = Motion::empty();
    let mut font = crate::document::Font::builtin();
    font.source = crate::document::FontSource::Embedded(font.bytes().to_vec());
    motion.fonts.insert(fold_foundation::ObjectId::new(), font);
    assert!(serde_json::to_vec(&motion).unwrap().len() > 1024 * 1024);
    host.project
        .commit(fold_project::EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                motion.document(id).unwrap(),
            )],
        })
        .unwrap();
    let before = host.project.snapshot();
    let mut state = state::State::default();
    state.sync(&mut host);
    state.motion.as_mut().unwrap().graph.nodes[0].position[0] += 10.;
    state.preview(&mut host);
    assert!(host.state.transient);
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    state.cancel(&mut host);
    assert_eq!(state.motion.as_ref().unwrap(), &motion);
    state.motion.as_mut().unwrap().graph.nodes[0].position[0] += 20.;
    state.preview(&mut host);
    state.commit(&mut host);
    assert_eq!(
        host.project.snapshot().revision().0,
        before.revision().0 + 1
    );
    assert_eq!(state.motion.as_ref().unwrap().fonts, motion.fonts);
    let after = host.project.snapshot();
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    host.project.redo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        after.state().documents
    );
}

#[test]
fn attract_particles_action_and_animation_share_one_undo_step() {
    let mut host = Host::new();
    let mut state = state::State::default();
    state.create(&mut host);
    state.change_scene(&mut host, |m| {
        a::scene::create_object(m, "fold.motion.text").map(Some)
    });
    let target = state.selected[0];
    let before = host.project.snapshot();
    state.change_scene(&mut host, |m| {
        a::particles::from_object(m, target).map(Some)
    });
    assert!(state.error.is_empty(), "{}", state.error);
    assert_eq!(
        host.project.snapshot().revision().0,
        before.revision().0 + 1
    );
    let owner = state.selected[0];
    assert!(
        state
            .motion
            .as_ref()
            .unwrap()
            .graph
            .node(owner)
            .unwrap()
            .animation
            .contains_key("attraction.0")
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    host.project.redo().unwrap();
    let snapshot = host.project.snapshot();
    let document = &snapshot.state().documents[&state.document.unwrap()];
    assert!(
        crate::Motion::from_document(document)
            .unwrap()
            .graph
            .node(owner)
            .is_ok()
    );
}

#[test]
fn badge_layer_selection_opens_its_original_graph_and_leaves_modifier_scope() {
    use fold_ui::sdk::graph_canvas::{GraphContext, GraphEvent};
    let mut host = Host::new();
    host.project = fold_project::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../demos/fold-badge-live.fold"),
        32,
    )
    .unwrap();
    let snapshot = host.project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| d.type_id == crate::MOTION)
        .unwrap();
    let motion = Motion::from_document(document).unwrap();
    let scene = motion.scene.as_ref().unwrap();
    let copies = scene
        .objects
        .iter()
        .find(|o| o.name == "Duplicator")
        .unwrap();
    let badge = scene
        .objects
        .iter()
        .find(|o| o.name.to_lowercase() == "badge")
        .unwrap();
    let text = scene
        .objects
        .iter()
        .find(|o| o.parent == Some(badge.id) && o.name == "Text")
        .unwrap();
    let mut state = state::State::default();
    let mut graphs = vec![];
    for object in [copies, badge, text] {
        host.state.selection = Selection {
            document: Some(document.id),
            objects: vec![object.id],
        };
        state.sync(&mut host);
        assert_eq!(
            state.network_object,
            Some(object.id),
            "Layer selection must choose that layer's actual graph"
        );
        assert_eq!(state.group, None);
        let mut context = graph::Context {
            state: &mut state,
            host: &mut host,
        };
        let graph = context.graph();
        assert!(graph.nodes.iter().any(|node| node.id == object.source.node));
        assert!(graph.nodes.iter().any(|node| node.id == object.output.node));
        assert!(graph.nodes.iter().all(|node| node.summary != "Graph"));
        graphs.push(graph.nodes.iter().map(|node| node.id).collect::<Vec<_>>());
        if object.id == copies.id {
            let group = graph.nodes.iter().find(|node| node.can_open).unwrap();
            context.event(GraphEvent::Navigate(Some(group.id))).unwrap();
            assert!(context.state.group.is_some());
        }
    }
    assert_ne!(graphs[0], graphs[1]);
    assert_ne!(graphs[1], graphs[2]);
}
