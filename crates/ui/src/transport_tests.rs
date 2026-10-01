use super::*;
use dear_imgui_rs::{Condition, Context, MouseButton};
use fold_platform::desktop::{DesktopState, PreviewKey, PreviewResult};
struct Host {
    state: DesktopState,
    commands: Vec<DesktopCommand>,
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        if let DesktopCommand::Transport(Action::Jump(frame)) = command {
            self.state.frame = frame;
            self.state.playing = false;
        }
        self.commands.push(command);
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
struct Layout {
    field: [f32; 2],
    strip: [[f32; 2]; 2],
}
fn draw(context: &mut Context, transport: &mut Transport, host: &mut Host) -> Layout {
    let ui = context.frame();
    let mut layout = Layout {
        field: [0.; 2],
        strip: [[0.; 2]; 2],
    };
    ui.window("Viewer")
        .position([0.; 2], Condition::Always)
        .size([700., 100.], Condition::Always)
        .build(|| {
            let p = ui.cursor_screen_pos();
            layout.strip = [
                p,
                [
                    p[0] + ui.content_region_avail()[0],
                    p[1] + ui.frame_height(),
                ],
            ];
            transport.draw(ui, host);
            let p = ui.item_rect_min();
            layout.field = [p[0] + 20., p[1] + 10.];
            shortcuts(ui, host);
        });
    drop(context.render_legacy());
    layout
}
#[test]
fn frame_entry_is_not_a_drag_control_and_does_not_get_overwritten_by_playback() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = context();
    let mut transport = Transport::default();
    let mut host = Host {
        state: DesktopState {
            viewer_document: Some(fold_foundation::DocumentId::new()),
            frame: 12,
            frames: 200,
            ..DesktopState::default()
        },
        commands: vec![],
    };
    let mut field = [0.; 2];
    for _ in 0..3 {
        field = draw(&mut context, &mut transport, &mut host).field;
    }
    context.io_mut().add_mouse_pos_event(field);
    draw(&mut context, &mut transport, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut context, &mut transport, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    draw(&mut context, &mut transport, &mut host);
    context.io_mut().add_input_characters_utf8("75");
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(transport.frame, "75");
    host.state.frame = 30;
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(
        transport.frame, "75",
        "playback must not replace an active draft"
    );
    assert!(host.commands.is_empty(), "typing must not seek until Enter");
    context.io_mut().add_key_event(Key::Enter, true);
    draw(&mut context, &mut transport, &mut host);
    context.io_mut().add_key_event(Key::Enter, false);
    draw(&mut context, &mut transport, &mut host);
    assert!(matches!(
        host.commands.as_slice(),
        [DesktopCommand::Transport(Action::Jump(75))]
    ));
    // Escape cancels another entry, without dispatching a jump.
    context.io_mut().add_mouse_pos_event(field);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut context, &mut transport, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    draw(&mut context, &mut transport, &mut host);
    context.io_mut().add_key_event(Key::ModCtrl, true);
    context.io_mut().add_key_event(Key::A, true);
    draw(&mut context, &mut transport, &mut host);
    context.io_mut().add_key_event(Key::A, false);
    context.io_mut().add_key_event(Key::ModCtrl, false);
    context.io_mut().add_input_characters_utf8("99");
    draw(&mut context, &mut transport, &mut host);
    context.io_mut().add_key_event(Key::Escape, true);
    draw(&mut context, &mut transport, &mut host);
    context.io_mut().add_key_event(Key::Escape, false);
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(host.commands.len(), 1);
    assert_eq!(transport.frame, "75");
    // A view switch drops the old draft instead of seeking the new document.
    host.state.viewer_document = Some(fold_foundation::DocumentId::new());
    host.state.frame = 7;
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(transport.frame, "7");
    assert_eq!(host.commands.len(), 1);
}
fn context() -> Context {
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([800., 200.]);
    context.io_mut().set_delta_time(1. / 60.);
    context
}

#[test]
fn scrub_mapping_includes_both_endpoints_and_handles_short_documents() {
    assert_eq!(scrub_frame(10., 10., 200., 101), 0);
    assert_eq!(scrub_frame(110., 10., 200., 101), 50);
    assert_eq!(scrub_frame(210., 10., 200., 101), 100);
    assert_eq!(scrub_frame(-100., 10., 200., 101), 0);
    assert_eq!(scrub_frame(500., 10., 200., 101), 100);
    assert_eq!(scrub_frame(500., 10., 200., 1), 0);
    assert_eq!(scrub_frame(500., 10., 200., 0), 0);
    assert_eq!(scrub_frame(500., 10., 0., 101), 0);
}

#[test]
fn scrub_click_drag_release_and_document_switch_use_the_shared_jump_command() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = context();
    let mut transport = Transport::default();
    let mut host = Host {
        state: DesktopState {
            viewer_document: Some(fold_foundation::DocumentId::new()),
            frame: 50,
            frames: 101,
            playing: true,
            playback_range: fold_platform::desktop::PlaybackRange {
                start: Some(20),
                end: Some(80),
            },
            ..DesktopState::default()
        },
        commands: vec![],
    };
    for _ in 0..3 {
        draw(&mut context, &mut transport, &mut host);
    }
    let rect = draw(&mut context, &mut transport, &mut host).strip;
    let middle = [
        (rect[0][0] + rect[1][0]) * 0.5,
        (rect[0][1] + rect[1][1]) * 0.5,
    ];
    context.io_mut().add_mouse_pos_event(middle);
    draw(&mut context, &mut transport, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(host.state.frame, 50);
    assert!(
        !host.state.playing,
        "clicking even the current frame must pause"
    );
    assert_eq!(host.commands.len(), 1);
    for _ in 0..3 {
        draw(&mut context, &mut transport, &mut host);
    }
    assert_eq!(
        host.commands.len(),
        1,
        "a stationary pointer must not repeatedly seek"
    );
    // Capture continues outside the hit box; scrubbing spans the full document,
    // not only the marked section, and clamps at both endpoints.
    context
        .io_mut()
        .add_mouse_pos_event([rect[1][0] + 100., middle[1] + 40.]);
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(host.state.frame, 100);
    context
        .io_mut()
        .add_mouse_pos_event([rect[0][0] - 100., middle[1] + 40.]);
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(host.state.frame, 0);
    // Movement delivered with the release must also reach the final frame.
    context.io_mut().add_mouse_pos_event(middle);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    for _ in 0..2 {
        draw(&mut context, &mut transport, &mut host);
    }
    assert_eq!(host.state.frame, 50);
    assert_eq!(
        host.state.playback_range.bounds(host.state.frames),
        (20, 80)
    );
    assert!(
        host.commands
            .iter()
            .all(|c| matches!(c, DesktopCommand::Transport(Action::Jump(_))))
    );
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut context, &mut transport, &mut host);
    let count = host.commands.len();
    host.state.viewer_document = Some(fold_foundation::DocumentId::new());
    host.state.frame = 7;
    context
        .io_mut()
        .add_mouse_pos_event([rect[1][0], middle[1]]);
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(
        host.state.frame, 7,
        "a held drag must not seek the newly viewed document"
    );
    assert_eq!(host.commands.len(), count);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    draw(&mut context, &mut transport, &mut host);
    host.state.viewer_document = None;
    context.io_mut().add_mouse_pos_event(middle);
    draw(&mut context, &mut transport, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut context, &mut transport, &mut host);
    assert_eq!(host.commands.len(), count, "no document means no seeking");
}
