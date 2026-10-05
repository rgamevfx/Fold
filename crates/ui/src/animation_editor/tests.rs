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
                                tracks: &[],
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
                        tracks: &[],
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

#[test]
fn collapsed_parent_summary_retimes_all_descendant_keys_and_commits_once() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut imgui = context();
    let mut editor = Editor::default();
    let document = DocumentId::new();
    let mut channels = channels();
    let original: Vec<_> = channels.iter().map(|c| c.curve.clone()).collect();
    let parent = ObjectId::new();
    let tracks = vec![
        Track {
            id: parent,
            parent: None,
            label: "Title".into(),
            locked: false,
            range: None,
            layer: None,
        },
        Track {
            id: channels[0].object,
            parent: Some(parent),
            label: "Transform".into(),
            locked: false,
            range: None,
            layer: None,
        },
    ];
    let mut auto = false;
    let mut draw = |imgui: &mut ImGui, editor: &mut Editor, channels: &mut [Channel]| {
        let mut response = Response::default();
        let mut body = [0.; 2];
        let ui = imgui.frame();
        ui.window("Scene")
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
                    channels,
                    super::Context {
                        document,
                        generation: 0,
                        time: Time::ZERO,
                        rate: [24, 1],
                        frames: 96,
                        nodes: &[],
                        tracks: &tracks,
                        auto_key: &mut auto,
                    },
                );
            });
        drop(imgui.render_legacy());
        (response, body)
    };
    for _ in 0..3 {
        draw(&mut imgui, &mut editor, &mut channels);
    }
    editor.collapsed_nodes.insert(parent);
    let (_, body) = draw(&mut imgui, &mut editor, &mut channels);
    // Grab between summary diamonds, not an individual key.
    let at = [editor.view.x(36., body[0]), body[1] + 9.5];
    imgui.io_mut().add_mouse_pos_event(at);
    draw(&mut imgui, &mut editor, &mut channels);
    imgui
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut imgui, &mut editor, &mut channels);
    imgui.io_mut().add_mouse_pos_event([at[0] + 40., at[1]]);
    let (response, _) = draw(&mut imgui, &mut editor, &mut channels);
    assert!(response.edit.changed && !response.edit.finished);
    let delta = channels[0].curve.keys[0]
        .time
        .checked_sub(original[0].keys[0].time)
        .unwrap();
    assert!(delta > Time::ZERO);
    for (channel, before) in channels.iter().zip(&original) {
        for (key, before) in channel.curve.keys.iter().zip(&before.keys) {
            assert_eq!(key.time, before.time.checked_add(delta).unwrap());
            assert_eq!(key.value, before.value);
        }
    }
    imgui
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    assert!(draw(&mut imgui, &mut editor, &mut channels).0.edit.finished);
    assert!(!draw(&mut imgui, &mut editor, &mut channels).0.edit.finished);
}

#[test]
fn static_object_range_drag_previews_and_cancels_without_keyframes() {
    strip_gesture(false, false, true);
}

#[test]
fn object_strip_moves_descendant_keys_but_edge_trims_preserve_them() {
    for trim in [false, true] {
        for cancel in [false, true] {
            strip_gesture(true, trim, cancel);
        }
    }
}

fn strip_gesture(animated: bool, trim: bool, cancel: bool) {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut imgui = context();
    let mut editor = Editor::default();
    let document = DocumentId::new();
    let id = ObjectId::new();
    let original = (Time::ZERO, Time::new(4, 1).unwrap());
    let mut tracks = vec![Track {
        id,
        parent: None,
        label: "Static box".into(),
        locked: false,
        range: Some(original),
        layer: None,
    }];
    let mut channels = if animated { channels() } else { vec![] };
    let before = channels.clone();
    if let Some(channel) = channels.first() {
        tracks.push(Track {
            id: channel.object,
            parent: Some(id),
            label: "Transform".into(),
            locked: false,
            range: None,
            layer: None,
        });
    }
    let mut auto = false;
    let mut draw =
        |imgui: &mut ImGui, editor: &mut Editor, tracks: &[Track], channels: &mut [Channel]| {
            let mut response = Response::default();
            let mut body = [0.; 2];
            let ui = imgui.frame();
            ui.window("Scene range")
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
                        channels,
                        super::Context {
                            document,
                            generation: 0,
                            time: Time::ZERO,
                            rate: [24, 1],
                            frames: 120,
                            nodes: &[],
                            tracks,
                            auto_key: &mut auto,
                        },
                    );
                });
            drop(imgui.render_legacy());
            (response, body)
        };
    for _ in 0..3 {
        draw(&mut imgui, &mut editor, &tracks, &mut channels);
    }
    let (_, body) = draw(&mut imgui, &mut editor, &tracks, &mut channels);
    editor.collapsed_nodes.insert(id);
    let at = [
        if trim {
            editor.view.x(0., body[0]) + 2.
        } else {
            editor.view.x(48., body[0])
        },
        body[1] + 9.,
    ];
    imgui.io_mut().add_mouse_pos_event(at);
    draw(&mut imgui, &mut editor, &tracks, &mut channels);
    imgui
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    draw(&mut imgui, &mut editor, &tracks, &mut channels);
    imgui.io_mut().add_mouse_pos_event([at[0] + 40., at[1]]);
    let (response, _) = draw(&mut imgui, &mut editor, &tracks, &mut channels);
    let (_, start, end) = response.range.unwrap();
    assert!(response.edit.changed && !response.edit.finished && start > Time::ZERO);
    if trim {
        assert_eq!(end, original.1);
    } else {
        assert_eq!(end.checked_sub(start).unwrap(), original.1);
    }
    for (channel, before) in channels.iter().zip(&before) {
        for (key, old) in channel.curve.keys.iter().zip(&before.curve.keys) {
            assert_eq!(
                key.time,
                if trim {
                    old.time
                } else {
                    old.time.checked_add(start).unwrap()
                }
            );
            assert_eq!(key.value, old.value);
        }
    }
    tracks[0].range = Some((start, end));
    if !cancel {
        imgui
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        assert!(
            draw(&mut imgui, &mut editor, &tracks, &mut channels)
                .0
                .edit
                .finished
        );
        assert!(
            !draw(&mut imgui, &mut editor, &tracks, &mut channels)
                .0
                .edit
                .finished
        );
        return;
    }
    imgui.io_mut().add_key_event(Key::Escape, true);
    let (response, _) = draw(&mut imgui, &mut editor, &tracks, &mut channels);
    assert!(response.edit.cancelled);
    assert_eq!(response.range, Some((id, original.0, original.1)));
    assert!(
        channels
            .iter()
            .zip(&before)
            .all(|(a, b)| a.curve == b.curve)
    );
}

#[test]
fn layer_eye_and_lock_emit_discrete_actions_at_normal_and_narrow_widths() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for width in [900., 170.] {
        let mut imgui = context();
        let mut editor = Editor::default();
        let document = DocumentId::new();
        let id = ObjectId::new();
        let mut auto = false;
        let tracks = [Track {
            id,
            parent: None,
            label: "Badge group".into(),
            locked: false,
            range: Some((Time::ZERO, Time::new(4, 1).unwrap())),
            layer: Some(LayerControls {
                accepts_children: false,
                visible: true,
                icon: "G",
            }),
        }];
        let mut draw = |imgui: &mut ImGui, editor: &mut Editor| {
            let mut response = Response::default();
            let mut row = [0.; 2];
            let ui = imgui.frame();
            ui.window("Layer controls")
                .position([0.; 2], Condition::Always)
                .size([width, 400.], Condition::Always)
                .build(|| {
                    let origin = ui.cursor_screen_pos();
                    row = [
                        origin[0],
                        origin[1] + ui.frame_height_with_spacing() + ui.text_line_height() + 6.,
                    ];
                    response = editor.draw(
                        ui,
                        &mut [],
                        super::Context {
                            document,
                            generation: 0,
                            time: Time::ZERO,
                            rate: [24, 1],
                            frames: 96,
                            nodes: &[],
                            tracks: &tracks,
                            auto_key: &mut auto,
                        },
                    );
                });
            drop(imgui.render_legacy());
            (response, row)
        };
        for _ in 0..3 {
            draw(&mut imgui, &mut editor);
        }
        let (_, row) = draw(&mut imgui, &mut editor);
        for (x, lock) in [(8., false), (26., true)] {
            imgui
                .io_mut()
                .add_mouse_pos_event([row[0] + x, row[1] + 9.]);
            draw(&mut imgui, &mut editor);
            imgui
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true);
            let (response, _) = draw(&mut imgui, &mut editor);
            if lock {
                assert_eq!(response.lock, Some((id, true)));
                assert!(response.visibility.is_none());
            } else {
                assert_eq!(response.visibility, Some((id, false)));
                assert!(response.lock.is_none());
            }
            assert!(!response.edit.changed);
            assert!(response.range.is_none());
            assert!(response.reorder.is_none());
            imgui
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false);
            draw(&mut imgui, &mut editor);
        }
    }
}

#[test]
fn scene_drag_targets_expand_commit_once_and_cancel_at_normal_and_narrow_widths() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for width in [900., 390.] {
        let mut imgui = context();
        let mut editor = Editor::default();
        let document = DocumentId::new();
        let group = ObjectId::new();
        let path = ObjectId::new();
        let tracks: Vec<_> = [(group, "Badge", true), (path, "Path", false)]
            .into_iter()
            .map(|(id, label, accepts_children)| Track {
                id,
                parent: None,
                label: label.into(),
                locked: false,
                range: None,
                layer: Some(LayerControls {
                    accepts_children,
                    visible: true,
                    icon: if accepts_children { "G" } else { "P" },
                }),
            })
            .collect();
        let mut auto = false;
        let mut draw = |imgui: &mut ImGui, editor: &mut Editor| {
            let mut response = Response::default();
            let mut body = [0.; 2];
            let mut height = 0.;
            let ui = imgui.frame();
            ui.window("Scene drop")
                .position([0.; 2], Condition::Always)
                .size([width, 400.], Condition::Always)
                .build(|| {
                    let origin = ui.cursor_screen_pos();
                    height = ui.text_line_height() + 6.;
                    body = [
                        origin[0],
                        origin[1] + ui.frame_height_with_spacing() + height,
                    ];
                    response = editor.draw(
                        ui,
                        &mut [],
                        super::Context {
                            document,
                            generation: 0,
                            time: Time::ZERO,
                            rate: [24, 1],
                            frames: 96,
                            nodes: &[],
                            tracks: &tracks,
                            auto_key: &mut auto,
                        },
                    );
                });
            drop(imgui.render_legacy());
            (response, body, height)
        };
        for _ in 0..3 {
            draw(&mut imgui, &mut editor);
        }
        let (_, body, height) = draw(&mut imgui, &mut editor);
        for cancel in [false, true] {
            editor.collapsed_nodes.insert(group);
            imgui
                .io_mut()
                .add_mouse_pos_event([body[0] + 85., body[1] + height * 1.5]);
            draw(&mut imgui, &mut editor);
            imgui
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true);
            draw(&mut imgui, &mut editor);
            imgui
                .io_mut()
                .add_mouse_pos_event([body[0] + 85., body[1] + height * 0.5]);
            for _ in 0..45 {
                let (r, _, _) = draw(&mut imgui, &mut editor);
                assert!(r.reparent.is_none());
            }
            assert!(!editor.collapsed_nodes.contains(&group));
            if cancel {
                imgui.io_mut().add_key_event(Key::Escape, true);
                draw(&mut imgui, &mut editor);
            }
            imgui
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false);
            let (r, _, _) = draw(&mut imgui, &mut editor);
            assert_eq!(
                r.reparent,
                if cancel {
                    None
                } else {
                    Some((path, Some(group)))
                }
            );
            assert_eq!(r.edit.changed, !cancel);
            assert!(draw(&mut imgui, &mut editor).0.reparent.is_none());
            imgui.io_mut().add_key_event(Key::Escape, false);
        }
    }
}
