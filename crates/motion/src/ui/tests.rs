use super::*;
use crate::{Motion, authoring as a, fields::Datum};
use fold_foundation::{DocumentId, Time};
use fold_platform::{desktop::*, packages::PackageRegistry};
use fold_project::{CommittedSnapshot, Project};
use fold_ui::sdk::{ExtensionUi, Panel, imgui};
struct Host {
    project: Project,
    state: DesktopState,
    registry: PackageRegistry,
}
impl Host {
    fn new() -> Self {
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
fn drawing_motion_graph_inspector_and_handles_does_not_mutate_project() {
    let mut host = Host::new();
    let state = std::rc::Rc::new(std::cell::RefCell::new(state::State::default()));
    state.borrow_mut().create(&mut host);
    state.borrow_mut().change(&mut host, |m| {
        a::add_content(m, "fold.motion.text").map(Some)
    });
    let revision = host.project.snapshot().revision();
    let mut context = imgui::Context::create();
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
        canvas: Default::default(),
        overlay: Default::default(),
    };
    canvas.initialize(&context);
    let mut inspector = inspector::Inspector {
        state: state.clone(),
        curves: Default::default(),
    };
    for frame in 0..16 {
        let maximized = frame >= 8;
        context.io_mut().set_display_size(if maximized {
            [1920., 1080.]
        } else {
            [1200., 900.]
        });
        let ui = context.frame();
        ui.window("Motion graph")
            .position([0., 300.], imgui::Condition::Always)
            .size([850., 600.], imgui::Condition::Always)
            .build(|| {
                canvas.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                })
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
