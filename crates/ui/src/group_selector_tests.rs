use super::*;
use dear_imgui_rs::{Condition, ConfigFlags, Context, Key, MouseButton};

fn context() -> Context {
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
    context.io_mut().set_display_size([500., 320.]);
    context.io_mut().set_delta_time(1. / 60.);
    context
}

#[test]
fn letter_control_is_small_at_normal_and_narrow_widths_and_selects_b_by_mouse() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for width in [440., 170.] {
        let mut context = context();
        let mut group = LinkGroup::A;
        let mut frame = |context: &mut Context| {
            let mut button = [0.; 2];
            let mut choice = [0.; 2];
            let ui = context.frame();
            ui.window("Group test")
                .position([0.; 2], Condition::Always)
                .size([width, 280.], Condition::Always)
                .build(|| {
                    let origin = ui.cursor_screen_pos();
                    if let Some(chosen) = draw(ui, group) {
                        group = chosen;
                    }
                    assert!(
                        ui.item_rect_size()[0] <= ui.frame_height() * 2.5,
                        "only a letter and dropdown arrow"
                    );
                    assert!(
                        ui.cursor_screen_pos()[1] - origin[1]
                            <= ui.frame_height_with_spacing() + 1.
                    );
                    button = [origin[0] + 12., origin[1] + ui.frame_height() / 2.];
                    choice = [
                        origin[0] + 12.,
                        origin[1]
                            + ui.frame_height()
                            + 8.
                            + 1.5 * ui.text_line_height_with_spacing(),
                    ];
                    ui.same_line();
                    ui.text("Source");
                });
            context.end_frame();
            (button, choice)
        };
        for _ in 0..4 {
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
        assert_eq!(group, LinkGroup::B);
    }
}

#[test]
fn letter_dropdown_has_keyboard_navigation() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = context();
    let mut group = LinkGroup::A;
    let mut frame = |context: &mut Context| {
        let ui = context.frame();
        ui.window("Keyboard groups")
            .position([0.; 2], Condition::Always)
            .size([220., 280.], Condition::Always)
            .build(|| {
                if let Some(chosen) = draw(ui, group) {
                    group = chosen;
                }
            });
        context.end_frame();
    };
    for _ in 0..4 {
        frame(&mut context);
    }
    for key in [Key::Tab, Key::Space, Key::DownArrow, Key::Enter] {
        context.io_mut().add_key_event(key, true);
        frame(&mut context);
        context.io_mut().add_key_event(key, false);
        frame(&mut context);
        frame(&mut context);
    }
    drop(frame);
    assert_eq!(group, LinkGroup::B);
}
