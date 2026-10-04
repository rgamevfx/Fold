use super::*;
use crate::{Composite, Node, Parameters};
use fold_foundation::DocumentId;
use fold_platform::{desktop::*, packages::PackageRegistry};
use fold_project::{CommittedSnapshot, EditBatch, Mutation, Project};
use fold_ui::sdk::{ExtensionUi, Panel, graph_canvas::*, imgui};
struct Host {
    project: Project,
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
fn setup() -> (Host, state::State) {
    let mut registry = PackageRegistry::default();
    crate::package::register(&mut registry).unwrap();
    let mut host = Host {
        project: Project::new(16),
        state: DesktopState::default(),
        registry,
    };
    let source = Node::new(
        Parameters::Solid {
            rgba: [0.5, 0.2, 0.1, 1.],
        },
        vec![],
    );
    let output = Node::new(Parameters::Output, vec![source.id]);
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
fn compositor_graph_and_inspector_draw_without_authoring() {
    let _guard = IMGUI_TEST_LOCK.lock().unwrap();
    let (mut host, state) = setup();
    let before = host.project.snapshot();
    let state = Rc::new(RefCell::new(state));
    let selected = state.borrow().graph.as_ref().unwrap().nodes[0].id;
    state.borrow_mut().selected = vec![selected];
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = canvas::Canvas::new(state.clone());
    canvas.initialize(&context);
    let mut inspector = inspector::Inspector(state);
    for frame in 0..20 {
        let width = if frame >= 10 { 320. } else { 850. };
        context.io_mut().add_mouse_pos_event([width - 20., 700.]);
        let ui = context.frame();
        ui.window("graph")
            .position([0.; 2], imgui::Condition::Always)
            .size([width, 800.], imgui::Condition::Always)
            .build(|| {
                let top = ui.cursor_screen_pos()[1];
                canvas.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                });
                if frame % 10 >= 3 {
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
    node.animation.insert("color.3".into(), curve);
    state.preview(&mut host);
    assert!(state.editing && !state.property_editing);
    let state = Rc::new(RefCell::new(state));
    let mut inspector = inspector::Inspector(state.clone());
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
