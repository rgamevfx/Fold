use super::*;
use crate::sdk::imgui::{Condition, Context as ImGui, MouseButton};
fn channels() -> Vec<Channel> {
    let object = ObjectId::new();
    ["X", "Y"]
        .into_iter()
        .enumerate()
        .map(|(i, name)| {
            let mut curve = fold_animation::Curve::default();
            curve.insert(Time::new(1, 1).unwrap(), i as f64);
            curve.insert(Time::new(2, 1).unwrap(), 10. + i as f64);
            Channel {
                object,
                node_label: "Transform 1".into(),
                property: "position".into(),
                property_label: "Position".into(),
                component: name.into(),
                path: format!("position.{i}"),
                curve,
            }
        })
        .collect()
}
fn context() -> ImGui {
    let mut context = ImGui::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 700.]);
    context.io_mut().set_delta_time(1. / 60.);
    context
}
#[test]
fn compact_views_fit_normal_and_narrow_panels_without_mutations() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = context();
    let mut editor = Editor::default();
    let document = DocumentId::new();
    let mut channels = channels();
    let before = channels.clone();
    let mut auto = false;
    for width in [900., 320., 210., 200., 180., 170., 150.] {
        for curves in [false, true] {
            for frame in 0..4 {
                let ui = context.frame();
                ui.window("Animation")
                    .position([0.; 2], Condition::Always)
                    .size([width, 600.], Condition::Always)
                    .build(|| {
                        editor.curves = curves;
                        let top = ui.cursor_screen_pos()[1];
                        let response = editor.draw(
                            ui,
                            &mut channels,
                            super::Context {
                                document,
                                generation: 0,
                                time: Time::ZERO,
                                rate: [24, 1],
                                frames: 96,
                                nodes: &[],
                                auto_key: &mut auto,
                            },
                        );
                        assert!(
                            !response.edit.changed
                                && !response.edit.finished
                                && !response.edit.cancelled
                        );
                        assert!(
                            ui.item_rect_min()[1] - top <= ui.frame_height_with_spacing() + 2.,
                            "only one toolbar row"
                        );
                        if frame >= 2 {
                            assert_eq!(
                                ui.scroll_max_x(),
                                0.,
                                "toolbar must not overflow at {width}"
                            );
                        }
                    });
                assert!(context.render_legacy().total_vtx_count() > 100);
            }
        }
    }
    for (a, b) in channels.iter().zip(before) {
        assert_eq!(a.curve, b.curve);
    }
}
#[test]
fn dragging_keys_previews_releases_once_and_escape_restores_original() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = context();
    let mut editor = Editor::default();
    let document = DocumentId::new();
    let mut channels = channels();
    let original = channels[0].curve.clone();
    let mut auto = false;
    let mut draw = |context: &mut ImGui, editor: &mut Editor| {
        let mut response = Response::default();
        let mut body = [0.; 2];
        let ui = context.frame();
        ui.window("Animation")
            .position([0.; 2], Condition::Always)
            .size([900., 500.], Condition::Always)
            .build(|| {
                let origin = ui.cursor_screen_pos();
                body = [
                    origin[0] + 210.,
                    origin[1] + ui.frame_height_with_spacing() + ui.text_line_height() + 6.,
                ];
                response = editor.draw(
                    ui,
                    &mut channels,
                    super::Context {
                        document,
                        generation: 0,
                        time: Time::ZERO,
                        rate: [24, 1],
                        frames: 96,
                        nodes: &[],
                        auto_key: &mut auto,
                    },
                );
            });
        drop(context.render_legacy());
        (response, body)
    };
    for _ in 0..3 {
        draw(&mut context, &mut editor);
    }
    let (_, body) = draw(&mut context, &mut editor);
    let row = 19.;
    let key = [editor.view.x(24., body[0]), body[1] + 2.5 * row];
    context.io_mut().add_mouse_pos_event(key);
    draw(&mut context, &mut editor);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut context, &mut editor);
    context.io_mut().add_mouse_pos_event([key[0] + 20., key[1]]);
    let (response, _) = draw(&mut context, &mut editor);
    assert!(response.edit.changed);
    assert!(!response.edit.finished);
    context.io_mut().add_key_event(Key::Escape, true);
    let (response, _) = draw(&mut context, &mut editor);
    assert!(response.edit.cancelled);
    context.io_mut().add_key_event(Key::Escape, false);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    draw(&mut context, &mut editor);
    context.io_mut().add_mouse_pos_event(key);
    draw(&mut context, &mut editor);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut context, &mut editor);
    context.io_mut().add_mouse_pos_event([key[0] + 20., key[1]]);
    draw(&mut context, &mut editor);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    let (response, _) = draw(&mut context, &mut editor);
    assert!(response.edit.finished);
    let (response, _) = draw(&mut context, &mut editor);
    assert!(!response.edit.finished);
    drop(draw);
    assert_ne!(channels[0].curve, original);
}
