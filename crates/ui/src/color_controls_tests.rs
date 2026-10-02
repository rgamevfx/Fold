use super::*;
use crate::sdk::imgui::{Condition, ConfigFlags, Context, Key};

#[test]
fn input_picker_is_one_row_narrow_and_keyboard_operable() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for width in [440., 170.] {
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
        let choices = Choices {
            inputs: vec!["Camera Rec.709".into(), "ACEScg".into()],
            ..Default::default()
        };
        let mut selected = choices.inputs[0].clone();
        let mut changes = 0;
        let mut frame = |context: &mut Context| {
            let ui = context.frame();
            ui.window("Color input test")
                .position([0.; 2], Condition::Always)
                .size([width, 280.], Condition::Always)
                .build(|| {
                    let start = ui.cursor_screen_pos();
                    let available = ui.content_region_avail()[0];
                    if input(ui, &choices, &mut selected) {
                        changes += 1;
                    }
                    assert!(ui.item_rect_size()[0] <= available + 1.);
                    assert!(
                        ui.cursor_screen_pos()[1] - start[1] <= ui.frame_height_with_spacing() + 1.
                    );
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
        assert_eq!(selected, "ACEScg");
        assert_eq!(changes, 1);
    }
}

#[test]
fn output_and_authored_color_do_not_rewrite_values_on_draw() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([500., 320.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut transform = OutputTransform::default();
    transform
        .extensions
        .insert("future".into(), serde_json::json!(7));
    let before = transform.clone();
    let mut color = [-0.2, 3., 0.123456789, 0.456789];
    for width in [440., 170.] {
        let ui = context.frame();
        ui.window("Output color test")
            .position([0.; 2], Condition::Always)
            .size([width, 280.], Condition::Always)
            .build(|| {
                assert!(!output(ui, &Choices::default(), &mut transform));
                let mut response = EditResponse::default();
                authored(ui, &mut color, true, &mut response);
                assert!(!response.changed);
            });
        context.end_frame();
    }
    assert_eq!(transform, before);
    assert_eq!(color, [-0.2, 3., 0.123456789, 0.456789]);
}
