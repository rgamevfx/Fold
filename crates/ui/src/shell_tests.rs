use super::*;
use crate::sdk::Panel;
use dear_imgui_rs::{ConfigFlags, Context};
use fold_platform::desktop::{DesktopState, PreviewResult};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn preview_resolution_tracks_viewport_pixels_quality_and_aspect() {
    let preview_dimensions = |dimensions, viewport, divisor| {
        crate::viewer_navigation::Navigation::default()
            .demand(dimensions, viewport, [1.; 2], divisor)
            .unwrap()
            .0
    };
    assert_eq!(
        preview_dimensions([3840, 2160], [480., 480.], 1),
        [480, 270]
    );
    assert_eq!(
        preview_dimensions([1920, 1080], [480., 480.], 2),
        [240, 135]
    );
    assert_eq!(
        preview_dimensions([3840, 2160], [960., 960.], 1),
        [960, 540]
    );
    assert_eq!(preview_dimensions([320, 180], [960., 960.], 1), [320, 180]);
    assert_eq!(
        preview_dimensions([1080, 1920], [480., 480.], 1),
        [270, 480]
    );
    assert!(
        crate::viewer_navigation::Navigation::default()
            .demand([1920, 1080], [0.; 2], [1.; 2], 1)
            .is_none()
    );
}

#[test]
#[cfg(feature = "native-probe")]
fn full_resolution_probe_does_not_measure_a_viewport_sized_substitute() {
    let mut shell = Shell::new(vec![]);
    let id = *shell.workspace.viewers.keys().next().unwrap();
    shell.viewport_pixels.insert(id, [480., 480.]);
    let key = PreviewKey {
        region: None,
        target: None,
        output: "video".into(),
        content: "test".into(),
        frame: 0,
        dimensions: [1920, 1080],
        view: 1,
        channels: Default::default(),
    };
    assert_eq!(
        shell.spatial_key(id, key.clone(), 1).unwrap().dimensions,
        [480, 270]
    );
    shell.probe_full_quality();
    assert_eq!(
        shell.spatial_key(id, key, 1).unwrap().dimensions,
        [1920, 1080]
    );
}

#[test]
fn playback_menu_selects_both_modes_and_source_default_at_narrow_widths() {
    use dear_imgui_rs::{Condition, MouseButton};
    use fold_platform::desktop::PlaybackMode;
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for width in [440., 170.] {
        for (row, expected) in [
            (2, Some(PlaybackMode::EveryFrame)),
            (1, Some(PlaybackMode::RealTime)),
            (0, None),
        ] {
            let mut context = Context::create();
            context.set_ini_filename(None::<String>).unwrap();
            context
                .font_atlas()
                .try_claim_legacy_renderer()
                .unwrap()
                .build();
            context.io_mut().set_display_size([500., 320.]);
            context.io_mut().set_delta_time(1. / 60.);
            let docs = [DocumentId::new(), DocumentId::new()];
            let mut host = Host {
                documents: docs,
                state: Default::default(),
                delivery: docs[0],
                commands: vec![],
            };
            let mut shell = Shell::new(vec![]);
            let id = *shell.workspace.viewers.keys().next().unwrap();
            let viewer = shell.workspace.viewers.get_mut(&id).unwrap();
            viewer.binding = ViewerBinding::Pinned(workspace::DocumentRef {
                document: docs[0],
                output: "video".into(),
                extensions: Default::default(),
            });
            viewer.playback_mode = Some(PlaybackMode::RealTime);
            {
                let mut frame = |context: &mut Context| {
                    let mut point = [0.; 2];
                    let ui = context.frame();
                    ui.window("Playback menu test")
                        .position([0.; 2], Condition::Always)
                        .size([width, 280.], Condition::Always)
                        .build(|| {
                            ui.open_popup("Mode choices");
                            if let Some(_popup) = ui.begin_popup("Mode choices") {
                                shell.playback_mode_items(ui, id, &mut host);
                                let min = ui.item_rect_min();
                                let size = ui.item_rect_size();
                                point = [
                                    min[0] + size[0] / 2.,
                                    min[1] + size[1] / 2.
                                        - (2 - row) as f32 * ui.text_line_height_with_spacing(),
                                ];
                            }
                        });
                    context.end_frame();
                    point
                };
                for _ in 0..4 {
                    frame(&mut context);
                }
                let point = frame(&mut context);
                context.io_mut().add_mouse_pos_event(point);
                frame(&mut context);
                context
                    .io_mut()
                    .add_mouse_button_event(MouseButton::Left, true);
                frame(&mut context);
                context
                    .io_mut()
                    .add_mouse_button_event(MouseButton::Left, false);
                frame(&mut context);
            }
            assert_eq!(shell.workspace.viewers[&id].playback_mode, expected);
            assert!(
                matches!(host.commands.last(), Some(DesktopCommand::ViewerTransport { viewer, .. }) if *viewer == id)
            );
        }
    }
}

#[test]
fn image_fit() {
    assert_eq!(fitted_size([640, 360], [800., 600.]), [800., 450.]);
    assert_eq!(fitted_size([640, 360], [-1., 90.]), [0., 0.]);
}

type Seen = Rc<RefCell<Vec<(Option<PanelInstanceId>, Option<DocumentId>, bool, u32)>>>;
struct Marker {
    inspector: bool,
    count: Rc<Cell<u32>>,
    seen: Seen,
}
impl Panel for Marker {
    fn id(&self) -> &'static str {
        if self.inspector {
            "test.inspector"
        } else {
            "test.editor"
        }
    }
    fn document_type(&self) -> Option<&'static str> {
        Some("test.kind")
    }
    fn new_instance(&self) -> Option<EditorPanels> {
        let count = Rc::new(Cell::new(0));
        Some(EditorPanels {
            editor: Box::new(Self {
                inspector: false,
                count: count.clone(),
                seen: self.seen.clone(),
            }),
            inspector: Box::new(Self {
                inspector: true,
                count,
                seen: self.seen.clone(),
            }),
        })
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        if !self.inspector {
            self.count.set(self.count.get() + 1);
        }
        self.seen.borrow_mut().push((
            context.instance(),
            context.host.state().selection.document,
            self.inspector,
            self.count.get(),
        ));
        context.ui.text("Instance content");
    }
}
struct Host {
    documents: [DocumentId; 2],
    state: DesktopState,
    delivery: DocumentId,
    commands: Vec<DesktopCommand>,
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn video_info(&self, id: DocumentId) -> Result<fold_platform::VideoInfo, String> {
        if !self.documents.contains(&id) {
            return Err("Missing document".into());
        }
        Ok(fold_platform::VideoInfo {
            width: 64,
            height: 64,
            rate: [24, 1],
            frames: 24,
        })
    }
    fn output_info(&self) -> Result<(DocumentId, fold_platform::VideoInfo), String> {
        Ok((self.delivery, self.video_info(self.delivery)?))
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        self.commands.push(command);
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}

#[test]
fn restored_viewers_initialize_host_transports_and_keep_explicit_mode_on_retarget() {
    use fold_platform::desktop::PlaybackMode;
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    shell.workspace.viewers.clear();
    let output = workspace::DocumentRef {
        document: docs[0],
        output: "video".into(),
        extensions: Default::default(),
    };
    let id = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Pinned(output.clone()),
        last_output: Some(output),
        playback_mode: Some(PlaybackMode::EveryFrame),
        time: Time::new(7, 48).unwrap(),
        ..Default::default()
    });
    shell.refresh_viewer_targets(&mut host);
    let Some(DesktopCommand::ViewerTransport { viewer, transport }) = host.commands.last() else {
        panic!("restored output needs a host transport before any frame can be admitted");
    };
    assert_eq!(*viewer, id);
    assert_eq!(transport.mode, PlaybackMode::EveryFrame);
    assert_eq!(transport.time, Time::new(7, 48).unwrap());
    assert!(!transport.playing);
    shell.workspace.viewers.get_mut(&id).unwrap().binding =
        ViewerBinding::Pinned(workspace::DocumentRef {
            document: docs[1],
            output: "video".into(),
            extensions: Default::default(),
        });
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(shell.playback_mode(id, &host), PlaybackMode::EveryFrame);
    shell.workspace.viewers.get_mut(&id).unwrap().playback_mode = None;
    assert_eq!(shell.playback_mode(id, &host), PlaybackMode::RealTime);
}

#[test]
fn editor_seeks_choose_one_explicit_viewer_and_leave_others_and_delivery_untouched() {
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    let editor = shell.workspace.add_editor("test.editor", "test.kind");
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(ViewLocation {
            document: docs[0],
            time: Time::ZERO,
            label: String::new(),
        });
    shell.workspace.viewers.clear();
    shell.workspace.record_selection(editor);
    let a = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Linked,
        editor: Some(editor),
        ..Default::default()
    });
    let other = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Pinned(workspace::DocumentRef {
            document: docs[1],
            output: "video".into(),
            extensions: Default::default(),
        }),
        ..Default::default()
    });
    shell.workspace.viewers.get_mut(&other).unwrap().playing = true;
    shell.seek_from_editor(editor, Time::new(7, 48).unwrap(), &mut host);
    assert_eq!(shell.workspace.viewers[&a].time, Time::new(7, 48).unwrap());
    assert_eq!(shell.workspace.viewers[&other].time, Time::ZERO);
    assert!(
        matches!(host.commands.last(), Some(DesktopCommand::ViewerTransport { viewer, .. }) if *viewer == a)
    );
    assert!(shell.workspace.viewers[&other].playing);
    let b = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Linked,
        editor: Some(editor),
        ..Default::default()
    });
    shell.seek_from_editor(editor, Time::new(1, 8).unwrap(), &mut host);
    assert!(matches!(
        host.commands.last(),
        Some(DesktopCommand::Notify(_))
    ));
    assert_eq!(shell.workspace.viewers[&b].time, Time::ZERO);
    shell.workspace.inspector_viewer = Some(b);
    shell.seek_from_editor(editor, Time::new(1, 8).unwrap(), &mut host);
    assert_eq!(shell.workspace.viewers[&b].time, Time::new(1, 8).unwrap());
    assert_eq!(shell.workspace.viewers[&a].time, Time::new(7, 48).unwrap());
    assert_eq!(host.delivery, docs[0]);
    assert!(
        !host
            .commands
            .iter()
            .any(|c| matches!(c, DesktopCommand::Pause | DesktopCommand::SetOutput(_)))
    );
}

#[test]
fn following_preserves_viewer_time_on_root_switch_and_maps_nested_navigation_and_back() {
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    let editor = shell.workspace.add_editor("test.editor", "test.kind");
    let location = |document, time| ViewLocation {
        document,
        time,
        label: String::new(),
    };
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(location(docs[0], Time::ZERO));
    shell.workspace.record_selection(editor);
    let viewer = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Linked,
        editor: Some(editor),
        time: Time::new(7, 48).unwrap(),
        ..Default::default()
    });
    shell.workspace.reconcile();
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(location(docs[1], Time::ZERO));
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(7, 48).unwrap()
    );
    // An explicit Open Source carries the associated edit-time context into
    // its breadcrumb, rather than treating it as an ordinary root switch.
    let binding = shell.workspace.editors.get_mut(&editor).unwrap();
    binding.navigation.last_mut().unwrap().time = Time::new(7, 48).unwrap();
    binding.navigate(location(docs[0], Time::new(1, 48).unwrap()));
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(1, 48).unwrap()
    );
    shell.workspace.editors.get_mut(&editor).unwrap().back();
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(7, 48).unwrap()
    );
    assert_eq!(host.delivery, docs[0]);
}

#[test]
fn joining_another_group_preserves_time_even_when_its_source_has_nested_navigation() {
    use workspace::LinkGroup;
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    let a = shell.workspace.add_editor("test.editor", "test.kind");
    let b = shell.workspace.add_editor("test.editor", "test.kind");
    let location = |document, time| ViewLocation {
        document,
        time,
        label: String::new(),
    };
    shell
        .workspace
        .editors
        .get_mut(&a)
        .unwrap()
        .bind(location(docs[0], Time::ZERO));
    shell
        .workspace
        .editors
        .get_mut(&b)
        .unwrap()
        .bind(location(docs[0], Time::ZERO));
    shell
        .workspace
        .editors
        .get_mut(&b)
        .unwrap()
        .navigate(location(docs[1], Time::new(1, 48).unwrap()));
    shell.workspace.record_selection(a);
    shell.workspace.set_editor_group(b, LinkGroup::B);
    let viewer = *shell.workspace.viewers.keys().next().unwrap();
    shell.workspace.viewers.get_mut(&viewer).unwrap().time = Time::new(7, 48).unwrap();
    shell.refresh_viewer_targets(&mut host);
    shell.workspace.set_viewer_group(viewer, LinkGroup::B);
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(shell.workspace.resolve(viewer).unwrap().document, docs[1]);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(7, 48).unwrap()
    );
    assert_eq!(host.delivery, docs[0]);
}

#[test]
fn runtime_editors_and_inspectors_are_isolated_at_normal_and_narrow_widths() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .io_mut()
        .set_config_flags(ConfigFlags::DOCKING_ENABLE);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_delta_time(1. / 60.);
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let seen = Seen::default();
    let panels = [
        ("test.editor", "Editor", PanelPlacement::Editor, false),
        (
            "test.inspector",
            "Inspector",
            PanelPlacement::Inspector,
            true,
        ),
    ]
    .into_iter()
    .map(|(id, title, placement, inspector)| RegisteredPanel {
        descriptor: fold_platform::packages::PanelDescriptor {
            id,
            title,
            placement,
        },
        key: WindowKey::new(id, title).unwrap(),
        panel: Box::new(Marker {
            inspector,
            count: Default::default(),
            seen: seen.clone(),
        }),
    })
    .collect();
    let mut shell = Shell::new(panels);
    shell.bootstrap = false;
    let a = *shell.workspace.editors.keys().next().unwrap();
    let b = shell.workspace.add_editor("test.editor", "test.kind");
    for (id, document) in [(a, docs[0]), (b, docs[1])] {
        shell
            .workspace
            .editors
            .get_mut(&id)
            .unwrap()
            .bind(ViewLocation {
                document,
                time: Time::ZERO,
                label: String::new(),
            });
    }
    shell.workspace.record_selection(a);
    shell.workspace.set_editor_group(b, workspace::LinkGroup::B);
    shell.sync_instances();
    for width in [1280., 520.] {
        context.io_mut().set_display_size([width, 600.]);
        shell.reset_layout = true;
        for id in [a, b, a] {
            shell.workspace.inspector_group = shell.workspace.editors[&id].group;
            shell.focus = Some(id);
            for _ in 0..3 {
                shell.prepare_frame(&mut context);
                let ui = context.frame();
                shell.controls(ui, &mut host).unwrap();
                shell.viewer(ui, &Preview::Pending, "test", &mut host);
                context.end_frame();
            }
        }
    }
    let observed = seen.borrow();
    for id in [a, b] {
        let document = shell.workspace.editors[&id].document();
        assert!(
            observed
                .iter()
                .any(|&(owner, target, inspector, _)| owner == Some(id)
                    && target == document
                    && inspector)
        );
        assert!(
            observed
                .iter()
                .filter(|&&(owner, _, _, _)| owner == Some(id))
                .all(|&(_, target, _, _)| target == document)
        );
        assert!(
            observed
                .iter()
                .any(|&(owner, _, inspector, count)| owner == Some(id) && !inspector && count == 1),
            "each instance starts with fresh gesture state"
        );
    }
    drop(observed);
    shell.workspace.inspector_group = workspace::LinkGroup::A;
    shell.workspace.record_selection(a);
    shell.workspace.toggle_inspector_lock();
    shell
        .workspace
        .editors
        .get_mut(&a)
        .unwrap()
        .bind(ViewLocation {
            document: docs[1],
            time: Time::new(5, 24).unwrap(),
            label: String::new(),
        });
    seen.borrow_mut().clear();
    for _ in 0..3 {
        shell.prepare_frame(&mut context);
        let ui = context.frame();
        shell.controls(ui, &mut host).unwrap();
        context.end_frame();
    }
    assert!(
        seen.borrow()
            .iter()
            .any(|&(owner, document, inspector, _)| owner == Some(a)
                && inspector
                && document == Some(docs[0])),
        "locked inspector retains the original network after the editor navigates"
    );
    assert_eq!(shell.workspace.editors[&a].document(), Some(docs[1]));
    assert_eq!(host.delivery, docs[0]);
    assert!(!host.commands.iter().any(|c| matches!(
        c,
        DesktopCommand::ActivateWorkspace(_)
            | DesktopCommand::Navigate(_)
            | DesktopCommand::SetOutput(_)
    )));
}

#[test]
fn review_menu_is_explicit_and_preserves_playback_mode_at_normal_and_narrow_widths() {
    use dear_imgui_rs::{Condition, MouseButton};
    use fold_platform::desktop::PlaybackMode;
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for width in [440., 170.] {
        for (mode, row, playing, ready, expected) in [
            (
                PlaybackMode::RealTime,
                0,
                false,
                false,
                Some(crate::review::Action::Prepare),
            ),
            (PlaybackMode::RealTime, 0, true, false, None),
            (
                PlaybackMode::RealTime,
                1,
                false,
                true,
                Some(crate::review::Action::PlayCached),
            ),
            (PlaybackMode::RealTime, 1, false, false, None),
            (PlaybackMode::EveryFrame, 1, false, true, None),
        ] {
            let mut context = Context::create();
            context.set_ini_filename(None::<String>).unwrap();
            context
                .font_atlas()
                .try_claim_legacy_renderer()
                .unwrap()
                .build();
            context.io_mut().set_display_size([600., 320.]);
            context.io_mut().set_delta_time(1. / 60.);
            let docs = [DocumentId::new(), DocumentId::new()];
            let host = Host {
                documents: docs,
                state: Default::default(),
                delivery: docs[0],
                commands: vec![],
            };
            let mut shell = Shell::new(vec![]);
            let id = *shell.workspace.viewers.keys().next().unwrap();
            let viewer = shell.workspace.viewers.get_mut(&id).unwrap();
            viewer.playback_mode = Some(mode);
            viewer.playing = playing;
            shell.review_states.insert(id, (None, ready));
            let mut frame = |context: &mut Context| {
                let mut point = [0.; 2];
                let ui = context.frame();
                ui.window("Review menu test")
                    .position([0.; 2], Condition::Always)
                    .size([width, 280.], Condition::Always)
                    .build(|| {
                        ui.open_popup("Review choices");
                        if let Some(_popup) = ui.begin_popup("Review choices") {
                            shell.review_menu_items(ui, id, &host);
                            let min = ui.item_rect_min();
                            let size = ui.item_rect_size();
                            let last = if mode == PlaybackMode::EveryFrame {
                                3
                            } else {
                                2
                            };
                            point = [
                                min[0] + 20.,
                                min[1] + size[1] / 2.
                                    - (last - row) as f32 * ui.text_line_height_with_spacing(),
                            ];
                        }
                    });
                context.end_frame();
                point
            };
            for _ in 0..4 {
                frame(&mut context);
            }
            let point = frame(&mut context);
            context.io_mut().add_mouse_pos_event(point);
            frame(&mut context);
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true);
            frame(&mut context);
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false);
            frame(&mut context);
            match expected {
                Some(crate::review::Action::Prepare) => assert!(
                    matches!(shell.take_review_actions().as_slice(),[(owner,crate::review::Action::Prepare)] if *owner == id)
                ),
                Some(crate::review::Action::PlayCached) => assert!(
                    matches!(shell.take_review_actions().as_slice(),[(owner,crate::review::Action::PlayCached)] if *owner == id)
                ),
                _ => assert!(shell.take_review_actions().is_empty()),
            }
            assert_eq!(shell.workspace.viewers[&id].playback_mode, Some(mode));
            assert!(host.commands.is_empty());
        }
    }
}

#[test]
fn animation_surface_uses_the_explicit_viewer_time_and_rejects_ambiguity() {
    type Times = Rc<RefCell<Vec<Time>>>;
    struct Animated(Times);
    impl Panel for Animated {
        fn id(&self) -> &'static str {
            "test.animated"
        }
        fn document_type(&self) -> Option<&'static str> {
            Some("test.kind")
        }
        fn supports_animation(&self) -> bool {
            true
        }
        fn new_instance(&self) -> Option<EditorPanels> {
            Some(EditorPanels {
                editor: Box::new(Self(self.0.clone())),
                inspector: Box::new(Self(self.0.clone())),
            })
        }
        fn draw(&mut self, _: ExtensionUi<'_>) {}
        fn draw_animation(&mut self, context: ExtensionUi<'_>) {
            self.0
                .borrow_mut()
                .push(context.host.state().navigation.last().unwrap().time);
        }
        fn supports_network(&self) -> bool {
            true
        }
        fn network_primary(&self) -> bool {
            true
        }
        fn draw_network(&mut self, context: ExtensionUi<'_>) {
            self.draw_animation(context);
        }
    }
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .io_mut()
        .set_config_flags(ConfigFlags::DOCKING_ENABLE);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 600.]);
    context.io_mut().set_delta_time(1. / 60.);
    let seen = Times::default();
    let panels = vec![
        RegisteredPanel {
            descriptor: fold_platform::packages::PanelDescriptor {
                id: "test.animated",
                title: "Editor",
                placement: PanelPlacement::Editor,
            },
            panel: Box::new(Animated(seen.clone())),
            key: WindowKey::new("test.animated", "Editor").unwrap(),
        },
        RegisteredPanel {
            descriptor: fold_platform::packages::PanelDescriptor {
                id: crate::sdk::animation_editor::PANEL_ID,
                title: "Animation",
                placement: PanelPlacement::Editor,
            },
            panel: Box::new(crate::sdk::animation_editor::Surface),
            key: WindowKey::new(crate::sdk::animation_editor::PANEL_ID, "Animation").unwrap(),
        },
    ];
    let mut shell = Shell::new(panels);
    shell.bootstrap = false;
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let editor = *shell.workspace.editors.keys().next().unwrap();
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(ViewLocation {
            document: docs[0],
            time: Time::ZERO,
            label: String::new(),
        });
    shell.workspace.record_selection(editor);
    let first = *shell.workspace.viewers.keys().next().unwrap();
    shell.workspace.viewers.get_mut(&first).unwrap().time = Time::new(7, 48).unwrap();
    let second = shell.workspace.add_viewer(ViewerInstance {
        time: Time::new(1, 8).unwrap(),
        ..Default::default()
    });
    // Explicitly reveal the Animation tab while keeping its ordinary dock placement.
    let mut frame = |shell: &mut Shell, context: &mut Context| {
        shell.prepare_frame(context);
        let ui = context.frame();
        shell.controls(ui, &mut host).unwrap();
        ui.window(&shell.panels[1].key).focused(true).build(|| {});
        context.end_frame();
    };
    for _ in 0..3 {
        frame(&mut shell, &mut context);
    }
    assert!(
        seen.borrow().is_empty(),
        "ambiguous viewer clocks must not reach animation controls"
    );
    for viewer in [first, second] {
        shell.workspace.inspector_viewer = Some(viewer);
        seen.borrow_mut().clear();
        for _ in 0..3 {
            frame(&mut shell, &mut context);
        }
        assert!(!seen.borrow().is_empty());
        assert!(
            seen.borrow()
                .iter()
                .all(|&time| time == shell.workspace.viewers[&viewer].time)
        );
    }
    // Reuse the linked instance through Network; the primary graph editor has no duplicate window.
    shell.panels[1].descriptor.id = crate::sdk::network::PANEL_ID;
    shell.panels[1].panel = Box::new(crate::sdk::network::Surface);
    shell.panels[1].key = WindowKey::new(crate::sdk::network::PANEL_ID, "Network").unwrap();
    shell.workspace.inspector_viewer = Some(first);
    seen.borrow_mut().clear();
    for _ in 0..3 {
        frame(&mut shell, &mut context);
    }
    assert!(!seen.borrow().is_empty());
    assert!(
        seen.borrow()
            .iter()
            .all(|&time| time == shell.workspace.viewers[&first].time)
    );
    assert!(shell.visible_editors.contains(&editor));
}

#[test]
fn channel_selector_and_viewer_status_fit_narrow_headers() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let documents = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents,
        state: Default::default(),
        delivery: documents[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    shell.workspace.viewers.clear();
    let source = workspace::DocumentRef {
        document: documents[0],
        output: "video".into(),
        extensions: Default::default(),
    };
    let id = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Pinned(source.clone()),
        last_output: Some(source),
        divisor: 4,
        playback_mode: Some(fold_platform::desktop::PlaybackMode::RealTime),
        ..Default::default()
    });
    shell.workspace.monitored_viewer = Some(id);
    for width in [420., 280.] {
        for frame in 0..4 {
            let ui = context.frame();
            ui.window("Viewer")
                .position([0.; 2], dear_imgui_rs::Condition::Always)
                .size([width, 400.], dear_imgui_rs::Condition::Always)
                .build(|| {
                    let top = ui.cursor_screen_pos()[1];
                    shell.viewer_header(ui, id, &mut host);
                    if frame >= 2 {
                        assert_eq!(ui.scroll_max_x(), 0., "{width}px");
                        assert!(
                            ui.cursor_screen_pos()[1] - top <= ui.frame_height_with_spacing() * 2.
                        );
                    }
                });
            drop(context.render_legacy());
        }
    }
}

#[test]
fn viewer_header_stays_one_row_and_inside_normal_and_narrow_panels() {
    use dear_imgui_rs::Condition;
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for width in [680., 240., 170.] {
        let mut context = Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([800., 400.]);
        context.io_mut().set_delta_time(1. / 60.);
        let docs = [DocumentId::new(), DocumentId::new()];
        let mut host = Host {
            documents: docs,
            state: Default::default(),
            delivery: docs[0],
            commands: vec![],
        };
        let mut shell = Shell::new(vec![]);
        let viewer = *shell.workspace.viewers.keys().next().unwrap();
        shell.workspace.pin_output(
            viewer,
            workspace::DocumentRef {
                document: docs[0],
                output: "video".into(),
                extensions: Default::default(),
            },
        );
        for divisor in [1, 2, 4] {
            shell.workspace.viewers.get_mut(&viewer).unwrap().divisor = divisor;
            for _ in 0..3 {
                let ui = context.frame();
                ui.window("Header")
                    .position([0.; 2], Condition::Always)
                    .size([width, 300.], Condition::Always)
                    .build(|| {
                        let origin = ui.cursor_screen_pos();
                        let available = ui.content_region_avail()[0];
                        shell.viewer_header(ui, viewer, &mut host);
                        assert!(
                            ui.cursor_screen_pos()[1] - origin[1]
                                <= ui.frame_height_with_spacing() + 1.
                        );
                        assert!(
                            ui.item_rect_max()[0] <= origin[0] + available + 1.,
                            "header fits at {width}px with divisor {divisor}"
                        );
                    });
                context.end_frame();
            }
        }
    }
}

#[test]
fn editor_without_a_viewer_seeks_its_own_exact_local_time() {
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    let editor = shell.workspace.add_editor("motion", "motion");
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(ViewLocation {
            document: docs[1],
            time: Time::ZERO,
            label: String::new(),
        });
    shell.workspace.record_selection(editor);
    // This viewer is explicitly watching a different output.
    let viewer = *shell.workspace.viewers.keys().next().unwrap();
    shell.workspace.pin_output(
        viewer,
        workspace::DocumentRef {
            document: docs[0],
            output: "video".into(),
            extensions: Default::default(),
        },
    );
    let time = Time::new(7, 48).unwrap();
    shell.seek_from_editor(editor, time, &mut host);
    assert_eq!(shell.workspace.inspector_context(), Some((editor, time)));
    assert_eq!(shell.workspace.viewers[&viewer].time, Time::ZERO);
    assert!(host.commands.is_empty());
}

#[test]
fn viewer_errors_do_not_change_preview_demand_size() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let documents = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents,
        state: Default::default(),
        delivery: documents[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    let id = *shell.workspace.viewers.keys().next().unwrap();
    shell.workspace.pin_output(
        id,
        workspace::DocumentRef {
            document: documents[0],
            output: "video".into(),
            extensions: Default::default(),
        },
    );
    shell.sync_instances();
    for width in [680., 240.] {
        let mut baseline = None;
        for frame in 0..9 {
            let error = (frame >= 4).then(|| {
                "Render failed with a long diagnostic that wraps over several lines. ".repeat(5)
            });
            shell.presentation.insert(id, (None, error.clone()));
            shell.review_state(id, (error.clone(), false));
            let preview = error.map(Preview::Failed).unwrap_or(Preview::Pending);
            let ui = context.frame();
            ui.window(&shell.viewers[&id])
                .position([0.; 2], dear_imgui_rs::Condition::Always)
                .size([width, 400.], dear_imgui_rs::Condition::Always)
                .build(|| {});
            shell.viewer_instance(ui, id, &preview, "", &mut host);
            let size = shell.viewport_pixels[&id];
            if frame == 3 {
                baseline = Some(size);
            }
            if frame >= 4 {
                assert_eq!(
                    Some(size),
                    baseline,
                    "message changed drawable size at {width}px"
                );
            }
            context.end_frame();
        }
    }
}
