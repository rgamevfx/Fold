use super::*;
use dear_imgui_rs::{Condition, ConfigFlags, Context, Key};
use fold_foundation::Time;
use fold_platform::{
    desktop::{PlaybackMode, ViewLocation},
    workspace::{DocumentRef, LinkGroup},
};

#[test]
fn combined_picker_keyboard_routing_is_independent_and_one_row() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for width in [680., 240.] {
        for unlinked in [false, true] {
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
            context.io_mut().set_display_size([800., 500.]);
            context.io_mut().set_delta_time(1. / 60.);
            let mut workspace = Workspace::default();
            let comp = workspace.add_editor("compositor", "comp");
            let motion = workspace.add_editor("motion", "motion");
            let docs = [DocumentId::new(), DocumentId::new()];
            for (editor, document) in [(comp, docs[0]), (motion, docs[1])] {
                workspace
                    .editors
                    .get_mut(&editor)
                    .unwrap()
                    .bind(ViewLocation {
                        document,
                        time: Time::ZERO,
                        label: String::new(),
                    });
            }
            workspace.record_selection(comp);
            let viewer = workspace.add_viewer(Default::default());
            let other = workspace.add_viewer(Default::default());
            let labels = [
                (docs[0], "Final Composite".into()),
                (docs[1], "Title Motion".into()),
            ]
            .into();
            let mut changed = 0;
            let mut frame = |context: &mut Context| {
                let ui = context.frame();
                ui.window("Viewer source test")
                    .position([0.; 2], Condition::Always)
                    .size([width, 440.], Condition::Always)
                    .build(|| {
                        let origin = ui.cursor_screen_pos();
                        changed += usize::from(draw(
                            ui,
                            &mut workspace,
                            viewer,
                            &labels,
                            0.,
                            |document| {
                                vec![OutputDescriptor {
                                    reference: DocumentRef {
                                        document,
                                        output: "video".into(),
                                        extensions: Default::default(),
                                    },
                                    label: "Video".into(),
                                    info: Ok(fold_platform::VideoInfo {
                                        width: 64,
                                        height: 64,
                                        rate: [24, 1],
                                        frames: 24,
                                    }),
                                    playback_mode: PlaybackMode::EveryFrame,
                                }]
                            },
                        ));
                        assert!(
                            ui.cursor_screen_pos()[1] - origin[1]
                                <= ui.frame_height_with_spacing() + 1.,
                            "group and source stay on one row"
                        );
                        assert!(
                            ui.item_rect_max()[0] <= origin[0] + width - 15.,
                            "source fits the panel width"
                        );
                    });
                context.end_frame();
            };
            for _ in 0..4 {
                frame(&mut context);
            }
            let mut keys = vec![Key::Tab, Key::Tab, Key::Space, Key::DownArrow];
            if unlinked {
                keys.push(Key::DownArrow);
            }
            keys.push(Key::Enter);
            for key in keys {
                context.io_mut().add_key_event(key, true);
                frame(&mut context);
                context.io_mut().add_key_event(key, false);
                frame(&mut context);
                frame(&mut context);
            }
            drop(frame);
            assert_eq!(changed, 1);
            assert_eq!(workspace.editor_for_viewer(other), Some(comp));
            if unlinked {
                assert!(matches!(
                    workspace.viewers[&viewer].binding,
                    ViewerBinding::Pinned(_)
                ));
            } else {
                assert_eq!(workspace.editor_for_viewer(viewer), Some(motion));
                assert_eq!(workspace.viewers[&viewer].group, LinkGroup::A);
            }
        }
    }
}
