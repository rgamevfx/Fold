use super::*;
use dear_imgui_rs::{Condition, Context, MouseButton};
#[test]
fn compact_selector_mouse_choice_is_stable_at_narrow_width() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([400., 300.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut button = [0.; 2];
    let mut choice = [0.; 2];
    let mut chosen = vec![];
    let mut frame = |context: &mut Context| {
        let ui = context.frame();
        ui.window("Target test")
            .position([0.; 2], Condition::Always)
            .size([220., 250.], Condition::Always)
            .build(|| {
                let origin = ui.cursor_screen_pos();
                let height = ui.frame_height_with_spacing();
                if let Some(Action::Choose(id)) = draw(
                    ui,
                    "Same name · Motion",
                    false,
                    &[Choice {
                        target: 42,
                        label: "Same name · Motion (2)".into(),
                        error: None,
                    }],
                ) {
                    chosen.push(id);
                }
                assert!(
                    ui.cursor_screen_pos()[1] - origin[1] <= height + 1.,
                    "target chrome stays one row at narrow widths"
                );
                button = [origin[0] + 60., origin[1] + ui.frame_height() / 2.];
                choice = [
                    origin[0] + 60.,
                    origin[1] + ui.frame_height() + 8. + ui.text_line_height() / 2.,
                ];
            });
        context.end_frame();
        (button, choice)
    };
    for _ in 0..3 {
        frame(&mut context);
    }
    let (button, _) = frame(&mut context);
    context.io_mut().add_mouse_pos_event(button);
    frame(&mut context);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    frame(&mut context);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    frame(&mut context);
    let (_, choice) = frame(&mut context);
    context.io_mut().add_mouse_pos_event(choice);
    frame(&mut context);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    frame(&mut context);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    frame(&mut context);
    drop(frame);
    assert_eq!(chosen, vec![42]);
}

#[test]
fn selector_supports_keyboard_activation_and_choice() {
    use dear_imgui_rs::{ConfigFlags, Key};
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .io_mut()
        .set_config_flags(ConfigFlags::NAV_ENABLE_KEYBOARD);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([400., 300.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut chosen = vec![];
    let mut frame = |context: &mut Context| {
        let ui = context.frame();
        ui.window("Keyboard targets")
            .position([0.; 2], Condition::Always)
            .size([220., 250.], Condition::Always)
            .build(|| {
                if let Some(Action::Choose(id)) = draw(
                    ui,
                    "Choose source",
                    false,
                    &[Choice {
                        target: 17,
                        label: "Pinned output".into(),
                        error: None,
                    }],
                ) {
                    chosen.push(id);
                }
            });
        context.end_frame();
    };
    for _ in 0..4 {
        frame(&mut context);
    }
    for key in [Key::Tab, Key::Space, Key::Enter] {
        context.io_mut().add_key_event(key, true);
        frame(&mut context);
        context.io_mut().add_key_event(key, false);
        frame(&mut context);
        frame(&mut context);
    }
    drop(frame);
    assert_eq!(chosen, vec![17]);
}
