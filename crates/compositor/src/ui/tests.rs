use super::*;
use crate::{Composite, Node, Parameters};
use fold_foundation::DocumentId;
use fold_platform::desktop::*;
use fold_project::{EditBatch, Mutation, Project, Snapshot};
use fold_ui::sdk::{ExtensionUi, Panel, imgui};
struct Host {
    project: Project,
    state: DesktopState,
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<Snapshot> {
        Some(self.project.snapshot())
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        match command {
            DesktopCommand::Select(selection) => self.state.selection = selection,
            DesktopCommand::CancelPreviewEdit => {}
            _ => panic!("drawing must not mutate the project"),
        }
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
#[test]
fn native_canvas_navigation_and_inspector_do_not_mutate_authoring() {
    let id = DocumentId::new();
    let source = Node::new(
        Parameters::Solid {
            rgba: [0.5, 0.2, 0.1, 1.0],
        },
        vec![],
    );
    let selected = source.id;
    let mut nodes = vec![source];
    for _ in 0..6 {
        nodes.push(Node::new(
            Parameters::Blur { radius: 2 },
            vec![nodes.last().unwrap().id],
        ));
    }
    let output = Node::new(Parameters::Output, vec![nodes.last().unwrap().id]);
    let output_id = output.id;
    nodes.insert(0, output);
    let graph = Composite {
        info: fold_media::VideoInfo {
            width: 16,
            height: 16,
            rate: [24, 1],
            frames: 24,
        },
        output: output_id,
        nodes,
        extensions: Default::default(),
    };
    let mut host = Host {
        project: Project::new(4),
        state: DesktopState::default(),
    };
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(graph.document(id).unwrap())],
        })
        .unwrap();
    let revision = host.project.snapshot().revision();
    let state = Rc::new(RefCell::new(state::State::default()));
    state.borrow_mut().sync(&mut host);
    state.borrow_mut().selected = vec![selected];
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context.io_mut().set_config_flags(
        imgui::ConfigFlags::DOCKING_ENABLE | imgui::ConfigFlags::NAV_ENABLE_KEYBOARD,
    );
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200.0, 800.0]);
    context.io_mut().set_delta_time(1.0 / 60.0);
    let mut canvas = canvas::Canvas::new(state.clone());
    canvas.initialize(&context);
    let mut inspector = inspector::Inspector(state.clone());
    let mut draw_frame = |context: &mut imgui::Context, canvas: &mut canvas::Canvas| {
        let ui = context.frame();
        ui.window("graph")
            .position([0.0, 0.0], imgui::Condition::Always)
            .size([850.0, 800.0], imgui::Condition::Always)
            .build(|| {
                canvas.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                });
            });
        ui.window("inspector")
            .position([850.0, 0.0], imgui::Condition::Always)
            .size([350.0, 800.0], imgui::Condition::Always)
            .build(|| {
                inspector.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                })
            });
        let draw = context.render_legacy();
        assert!(draw.total_vtx_count() > 100);
    };
    for _ in 0..20 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(
        canvas.framed,
        "every node in the wide graph must fit inside the canvas"
    );
    context.io_mut().add_mouse_pos_event([420.0, 650.0]);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    let scale = |canvas: &canvas::Canvas| canvas.view[1][0] - canvas.view[0][0];
    let before = scale(&canvas);
    context.io_mut().add_mouse_wheel_event([0.0, 1.0]);
    for _ in 0..20 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(
        scale(&canvas) > before + 1.0,
        "wheel must zoom after framing: {before} -> {}",
        scale(&canvas)
    );
    let before = scale(&canvas);
    context.io_mut().add_mouse_wheel_event([0.0, -1.0]);
    for _ in 0..20 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(
        scale(&canvas) < before - 1.0,
        "wheel must continue zooming out after framing"
    );
    let before = scale(&canvas);
    for _ in 0..12 {
        context.io_mut().add_mouse_wheel_event([0.0, 0.2]);
        draw_frame(&mut context, &mut canvas);
    }
    for _ in 0..20 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(
        scale(&canvas) > before + 1.0,
        "fractional trackpad wheel must keep zooming after framing: {before} -> {}",
        scale(&canvas)
    );
    let before = scale(&canvas);
    for _ in 0..12 {
        context.io_mut().add_mouse_wheel_event([0.0, -0.2]);
        draw_frame(&mut context, &mut canvas);
    }
    for _ in 0..20 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(
        scale(&canvas) < before - 1.0,
        "fractional trackpad wheel must zoom out too"
    );
    assert!(canvas.framed);
    // Plain background drag pans without any held keyboard key.
    let before = canvas.view[0];
    let start = [420.0, 650.0];
    context.io_mut().add_mouse_pos_event(start);
    let mut pan = fold_ui::sdk::CanvasPan::default();
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(canvas.accepts_background_pan(start));
    let button = pan.left_button(true, canvas.accepts_background_pan(start));
    assert_eq!(button, imgui::MouseButton::Middle);
    context.io_mut().add_mouse_button_event(button, true);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context
        .io_mut()
        .add_mouse_pos_event([start[0] + 40.0, start[1] + 25.0]);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context
        .io_mut()
        .add_mouse_pos_event([start[0] + 70.0, start[1] + 35.0]);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context
        .io_mut()
        .add_mouse_button_event(pan.left_button(false, false), false);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(
        (canvas.view[0][0] - before[0] - 70.0).abs() < 2.0,
        "background left-drag must pan without moving nodes: {before:?} -> {:?}",
        canvas.view[0]
    );
    assert!((canvas.view[0][1] - before[1] - 35.0).abs() < 2.0);
    // The original physical middle-drag path still works after the alias.
    let before = canvas.view[0];
    context.io_mut().add_mouse_pos_event([420.0, 650.0]);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Middle, true);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context.io_mut().add_mouse_pos_event([380.0, 640.0]);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Middle, false);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!((canvas.view[0][0] - before[0] + 40.0).abs() < 2.0);
    assert!((canvas.view[0][1] - before[1] + 10.0).abs() < 2.0);
    // Wires retain ordinary click selection, not background pan routing.
    let wire = canvas.wire_probe.0;
    assert!(!canvas.accepts_background_pan(wire));
    context.io_mut().add_mouse_pos_event(wire);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    let button = pan.left_button(true, canvas.accepts_background_pan(wire));
    assert_eq!(button, imgui::MouseButton::Left);
    context.io_mut().add_mouse_button_event(button, true);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context
        .io_mut()
        .add_mouse_button_event(pan.left_button(false, false), false);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    assert_eq!(
        canvas.wire_probe.1, 1,
        "native wire click must select the rendered wire"
    );
    // Shift+background drag keeps native box selection, not camera motion.
    let before = canvas.view;
    pan.shift(true);
    context.io_mut().add_key_event(imgui::Key::ModShift, true);
    context.io_mut().add_mouse_pos_event([40.0, 300.0]);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(canvas.accepts_background_pan([40.0, 300.0]));
    let button = pan.left_button(true, canvas.accepts_background_pan([40.0, 300.0]));
    assert_eq!(button, imgui::MouseButton::Left);
    context
        .io_mut()
        .add_key_event(imgui::Key::ModShift, pan.effective_shift());
    context.io_mut().add_mouse_button_event(button, true);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context.io_mut().add_mouse_pos_event([815.0, 600.0]);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context
        .io_mut()
        .add_mouse_button_event(pan.left_button(false, true), false);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    pan.shift(false);
    context.io_mut().add_key_event(imgui::Key::ModShift, false);
    draw_frame(&mut context, &mut canvas);
    assert!(
        state.borrow().selected.len() > 1,
        "Shift drag must box-select nodes: view {:?}, selected {:?}",
        canvas.view,
        state.borrow().selected
    );
    assert_eq!(canvas.view, before);

    // Incoming presses on a node stay left presses, even when the previous
    // pointer was on background. Moving a node must not move the camera.
    let unit = scale(&canvas) / 100.0;
    let start = [
        canvas.view[0][0] + 80.0 * unit,
        canvas.view[0][1] + 20.0 * unit,
    ];
    assert!(!canvas.accepts_background_pan(start));
    context.io_mut().add_mouse_pos_event(start);
    context.io_mut().add_mouse_button_event(
        pan.left_button(true, canvas.accepts_background_pan(start)),
        true,
    );
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    context
        .io_mut()
        .add_mouse_pos_event([start[0] + 30.0, start[1] + 20.0]);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(
        state.borrow().editing,
        "node drag must still edit node positions"
    );
    assert_eq!(canvas.view, before);
    context.io_mut().add_key_event(imgui::Key::Escape, true);
    draw_frame(&mut context, &mut canvas);
    context.io_mut().add_key_event(imgui::Key::Escape, false);
    context
        .io_mut()
        .add_mouse_button_event(pan.left_button(false, false), false);
    for _ in 0..3 {
        draw_frame(&mut context, &mut canvas);
    }
    assert!(!state.borrow().editing);
    assert_eq!(host.project.snapshot().revision(), revision);
    assert_eq!(
        Composite::from_document(&host.project.snapshot().state().documents[&id]).unwrap(),
        graph
    );
    // Explicitly prove the ownership order used by the desktop shell.
    drop(canvas);
    drop(inspector);
    drop(context);
}
