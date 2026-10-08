use super::*;
use crate::sdk::Panel;
use dear_imgui_rs::{ConfigFlags, Context};
use fold_platform::desktop::{DesktopState, PreviewResult};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn external_viewer_retarget_updates_binding_without_moving_other_viewers() {
    use fold_platform::desktop::ViewerTransport;
    struct External {
        state: DesktopState,
        id: PanelInstanceId,
        transport: ViewerTransport,
    }
    impl DesktopClient for External {
        fn state(&self) -> &DesktopState {
            &self.state
        }
        fn poll(&mut self) {}
        fn command(&mut self, _: DesktopCommand) {}
        fn request_preview(&mut self, _: PreviewKey) {}
        fn cancel_preview(&mut self) {}
        fn take_preview(&mut self) -> Option<PreviewResult> {
            None
        }
        fn viewer_transport(&self, id: PanelInstanceId) -> Option<ViewerTransport> {
            (id == self.id).then(|| self.transport.clone())
        }
    }
    let reference = || workspace::DocumentRef {
        document: DocumentId::new(),
        output: "video".into(),
        extensions: Default::default(),
    };
    let old = reference();
    let new = reference();
    let mut shell = Shell::new(vec![]);
    let viewer = *shell.workspace.viewers.keys().next().unwrap();
    let original = ViewerInstance {
        binding: ViewerBinding::Pinned(old.clone()),
        last_output: Some(old.clone()),
        time: Time::new(6, 1).unwrap(),
        ..Default::default()
    };
    shell.workspace.viewers.insert(viewer, original.clone());
    let other = shell.workspace.add_viewer(original);
    let host = External {
        state: Default::default(),
        id: viewer,
        transport: ViewerTransport {
            output: new.clone(),
            time: Time::new(3, 1).unwrap(),
            playing: false,
            looping: false,
            mode: Default::default(),
            range: Default::default(),
        },
    };
    shell.presentation(viewer, None, None, &host);
    assert_eq!(shell.workspace.resolve(viewer), Some(new.clone()));
    assert_eq!(shell.workspace.viewers[&viewer].last_output, Some(new));
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(3, 1).unwrap()
    );
    assert_eq!(shell.workspace.resolve(other), Some(old));
    assert_eq!(
        shell.workspace.viewers[&other].time,
        Time::new(6, 1).unwrap()
    );
}

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
    fn draw_viewer_overlay(&mut self, context: ExtensionUi<'_>, rect: crate::sdk::ViewerRect) {
        context
            .ui
            .get_window_draw_list()
            .add_line(
                rect.origin,
                [rect.origin[0] + rect.size[0], rect.origin[1] + rect.size[1]],
                [1.; 4],
            )
            .build();
        self.seen.borrow_mut().push((
            context.instance(),
            context.host.state().selection.document,
            false,
            0,
        ));
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
fn composition_surface_uses_the_explicit_viewer_time_and_rejects_ambiguity() {
    type Times = Rc<RefCell<Vec<Time>>>;
    struct Animated(Times);
    impl Panel for Animated {
        fn id(&self) -> &'static str {
            "test.animated"
        }
        fn document_type(&self) -> Option<&'static str> {
            Some("test.kind")
        }
        fn supports_composition(&self) -> bool {
            true
        }
        fn new_instance(&self) -> Option<EditorPanels> {
            Some(EditorPanels {
                editor: Box::new(Self(self.0.clone())),
                inspector: Box::new(Self(self.0.clone())),
            })
        }
        fn draw(&mut self, context: ExtensionUi<'_>) {
            self.0
                .borrow_mut()
                .push(context.host.state().navigation.last().unwrap().time);
        }
        fn supports_network(&self) -> bool {
            true
        }
        fn draw_network(&mut self, context: ExtensionUi<'_>) {
            self.draw(context);
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
                id: crate::sdk::composition::PANEL_ID,
                title: "Composition",
                placement: PanelPlacement::Editor,
            },
            panel: Box::new(crate::sdk::composition::Surface),
            key: WindowKey::new(crate::sdk::composition::PANEL_ID, "Composition").unwrap(),
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
    // Explicitly reveal the Composition tab while keeping its ordinary dock placement.
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
    // A locked Inspector must not replace Composition's own local authoring clock.
    shell.workspace.toggle_inspector_lock();
    let locked_time = shell
        .workspace
        .inspector_lock
        .as_ref()
        .unwrap()
        .1
        .navigation
        .last()
        .unwrap()
        .time;
    let viewers = std::mem::take(&mut shell.workspace.viewers);
    let local_time = Time::new(19, 24).unwrap();
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .navigation
        .last_mut()
        .unwrap()
        .time = local_time;
    seen.borrow_mut().clear();
    for _ in 0..3 {
        frame(&mut shell, &mut context);
    }
    assert!(!seen.borrow().is_empty());
    assert!(seen.borrow().iter().all(|&time| time == local_time));
    assert_eq!(
        shell
            .workspace
            .inspector_lock
            .as_ref()
            .unwrap()
            .1
            .navigation
            .last()
            .unwrap()
            .time,
        locked_time
    );
    shell.workspace.inspector_lock = None;
    shell.workspace.viewers = viewers;

    // Reuse the linked instance through Network; the provider has no duplicate window.
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

#[test]
fn ready_viewer_releases_image_draw_list_before_editor_overlay() {
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
    let seen = Seen::default();
    let mut shell = Shell::new(vec![RegisteredPanel {
        descriptor: fold_platform::packages::PanelDescriptor {
            id: "test.editor",
            title: "Editor",
            placement: PanelPlacement::Editor,
        },
        key: WindowKey::new("test.editor", "Editor").unwrap(),
        panel: Box::new(Marker {
            inspector: false,
            count: Default::default(),
            seen: seen.clone(),
        }),
    }]);
    let editor = *shell.workspace.editors.keys().next().unwrap();
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(ViewLocation {
            document: documents[0],
            time: Time::ZERO,
            label: String::new(),
        });
    shell.workspace.record_selection(editor);
    shell.sync_instances();
    let id = *shell.workspace.viewers.keys().next().unwrap();
    assert_eq!(shell.workspace.editor_for_viewer(id), Some(editor));
    let preview = Preview::Ready {
        texture: dear_imgui_rs::TextureId::new(1),
        dimensions: [64, 64],
        uv_max: [1.; 2],
    };
    for width in [680., 240.] {
        for _ in 0..3 {
            let before = seen.borrow().len();
            let ui = context.frame();
            ui.window(&shell.viewers[&id])
                .position([0.; 2], dear_imgui_rs::Condition::Always)
                .size([width, 400.], dear_imgui_rs::Condition::Always)
                .build(|| {});
            shell.viewer_instance(ui, id, &preview, "", &mut host);
            assert_eq!(
                seen.borrow().len(),
                before + 1,
                "overlay must actually draw"
            );
            context.end_frame();
        }
    }
}

#[test]
fn opening_a_layer_graph_retains_the_parent_stack_selection_and_clock() {
    struct Graph;
    impl Panel for Graph {
        fn id(&self) -> &'static str {
            "test.graph"
        }
        fn document_type(&self) -> Option<&'static str> {
            Some("test.graph.kind")
        }
        fn is_layer_graph(&self) -> bool {
            true
        }
        fn draw(&mut self, _: ExtensionUi<'_>) {}
    }
    let mut shell = Shell::new(vec![RegisteredPanel {
        descriptor: fold_platform::packages::PanelDescriptor {
            id: "test.graph",
            title: "Layer graph",
            placement: PanelPlacement::Editor,
        },
        panel: Box::new(Graph),
        key: WindowKey::new("test.graph", "Layer graph").unwrap(),
    }]);
    let parent = shell
        .workspace
        .add_editor("test.composition", "test.composition.kind");
    let document = DocumentId::new();
    let source = DocumentId::new();
    let layer = fold_foundation::ObjectId::new();
    let binding = shell.workspace.editors.get_mut(&parent).unwrap();
    binding.bind(ViewLocation {
        document,
        time: Time::new(7, 24).unwrap(),
        label: "Shot".into(),
    });
    binding.selection.objects = vec![layer];
    let parent_before = binding.clone();
    let mut child = binding.clone();
    child.navigate(ViewLocation {
        document: source,
        time: Time::new(1, 48).unwrap(),
        label: "Foreground".into(),
    });
    child.document_type = "test.graph.kind".into();
    shell.workspace.composition_editor = Some(parent);
    shell.workspace.record_selection(parent);
    let object = fold_foundation::ObjectId::new();
    child.selection.objects = vec![object];
    shell.open_layer_graph(child.clone());
    let graph = shell.workspace.focused_editor.unwrap();
    assert_ne!(graph, parent);
    assert_eq!(shell.workspace.inspector_editor(), Some(parent));
    assert_eq!(shell.workspace.network_editor, Some(graph));
    assert_eq!(
        shell.workspace.editors[&graph].selection.objects, child.selection.objects,
        "Network must receive the requested motion object to open its graph"
    );
    assert_eq!(shell.workspace.composition_editor, Some(parent));
    assert_eq!(
        shell.workspace.editors[&parent].navigation,
        parent_before.navigation
    );
    assert_eq!(
        shell.workspace.editors[&parent].selection.objects,
        vec![layer]
    );
    assert_eq!(shell.workspace.editors[&graph].navigation, child.navigation);
    assert_eq!(shell.workspace.editors[&graph].document(), Some(source));
    let count = shell.workspace.editors.len();
    shell.open_layer_graph(child.clone());
    assert_eq!(
        shell.workspace.editors[&graph].selection.objects,
        vec![object]
    );
    // Another layer must replace the old object's scope in the same Network slot.
    let next_object = fold_foundation::ObjectId::new();
    child.selection.objects = vec![next_object];
    shell.open_layer_graph(child);
    assert_eq!(
        shell.workspace.editors[&graph].selection.objects,
        vec![next_object]
    );
    assert_eq!(shell.workspace.inspector_editor(), Some(parent));
    assert_eq!(shell.workspace.focused_editor, Some(graph));
    assert_eq!(shell.workspace.editors.len(), count);
    shell
        .workspace
        .editors
        .get_mut(&graph)
        .unwrap()
        .selection
        .objects = vec![layer];
    shell.workspace.record_selection(graph);
    assert_eq!(shell.workspace.inspector_editor(), Some(graph));
    shell.workspace.record_selection(parent);
    assert_eq!(shell.workspace.inspector_editor(), Some(parent));
    assert_eq!(shell.workspace.network_editor, Some(graph));
}

#[test]
fn composition_surface_has_a_context_after_restoring_a_workspace_without_one() {
    struct CompositionPanel;
    impl Panel for CompositionPanel {
        fn id(&self) -> &'static str {
            "test.composition"
        }
        fn document_type(&self) -> Option<&'static str> {
            Some("test.composition.kind")
        }
        fn supports_composition(&self) -> bool {
            true
        }
        fn new_instance(&self) -> Option<EditorPanels> {
            Some(EditorPanels {
                editor: Box::new(Self),
                inspector: Box::new(Self),
            })
        }
        fn draw(&mut self, _: ExtensionUi<'_>) {}
    }
    let mut shell = Shell::new(vec![RegisteredPanel {
        descriptor: fold_platform::packages::PanelDescriptor {
            id: "test.composition",
            title: "Composition",
            placement: PanelPlacement::Editor,
        },
        panel: Box::new(CompositionPanel),
        key: WindowKey::new("test.composition", "Composition").unwrap(),
    }]);
    // A restored layout need not contain every currently registered editor.
    shell.workspace.editors.clear();
    shell.workspace.composition_editor = None;
    shell.sync_instances();
    let id = shell.workspace.composition_editor.expect(
        "Composition needs a bound editor context to draw its group badge and authoring controls",
    );
    assert_eq!(shell.workspace.editors[&id].group, workspace::LinkGroup::A);
    shell.sync_instances();
    assert_eq!(shell.workspace.composition_editor, Some(id));
    assert_eq!(shell.workspace.editors.len(), 1);
}

#[test]
fn editor_content_context_menu_survives_right_button_release() {
    use dear_imgui_rs::MouseButton;
    #[derive(Default)]
    struct Observed {
        point: [f32; 2],
        open: bool,
        tab: [f32; 2],
        tab_menu: bool,
    }
    struct Clip(Rc<RefCell<Observed>>);
    impl Panel for Clip {
        fn id(&self) -> &'static str {
            "test.clip"
        }
        fn document_type(&self) -> Option<&'static str> {
            Some("test.kind")
        }
        fn new_instance(&self) -> Option<EditorPanels> {
            Some(EditorPanels {
                editor: Box::new(Self(self.0.clone())),
                inspector: Box::new(Self(self.0.clone())),
            })
        }
        fn draw(&mut self, context: ExtensionUi<'_>) {
            let ui = context.ui;
            let pos = ui.window_pos();
            self.0.borrow_mut().tab = [pos[0] + 45., pos[1] + ui.text_line_height() * 0.5];
            self.0.borrow_mut().tab_menu = ui.is_popup_open("editor-tab-menu");
            let p = ui.cursor_screen_pos();
            self.0.borrow_mut().point = [p[0] + 40., p[1] + 30.];
            ui.invisible_button("clip", [180., 80.]);
            if ui.is_item_hovered() && ui.is_mouse_clicked(MouseButton::Right) {
                ui.open_popup("clip-menu");
            }
            let popup = ui.begin_popup("clip-menu");
            self.0.borrow_mut().open = popup.is_some();
            if popup.is_some() {
                ui.menu_item("New composition from clip");
            }
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
    let seen = Rc::new(RefCell::new(Observed::default()));
    let mut shell = Shell::new(vec![RegisteredPanel {
        descriptor: fold_platform::packages::PanelDescriptor {
            id: "test.clip",
            title: "Timeline",
            placement: PanelPlacement::Editor,
        },
        panel: Box::new(Clip(seen.clone())),
        key: WindowKey::new("test.clip", "Timeline").unwrap(),
    }]);
    shell.bootstrap = false;
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    for step in 0..23 {
        context
            .io_mut()
            .add_key_event(dear_imgui_rs::Key::Escape, step == 12);
        if step == 16 {
            context.io_mut().add_mouse_pos_event(seen.borrow().tab);
        }
        if step == 17 {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Right, true);
        }
        if step == 19 {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Right, false);
        }
        if step == 5 {
            context.io_mut().add_mouse_pos_event(seen.borrow().point);
        }
        if step == 6 {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Right, true);
        }
        if step == 8 {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Right, false);
        }
        let ui = context.frame();
        shell.controls(ui, &mut host).unwrap();
        if (7..12).contains(&step) {
            assert!(seen.borrow().open, "clip menu disappeared at frame {step}");
        }
        if step >= 20 {
            assert!(
                seen.borrow().tab_menu,
                "right-clicking the tab must open Close editor"
            );
        }
        context.end_frame();
    }
}

#[test]
fn nested_navigation_preserves_timeline_and_follows_group_a() {
    struct Editor(bool);
    impl Panel for Editor {
        fn id(&self) -> &'static str {
            if self.0 { "test.comp" } else { "test.timeline" }
        }
        fn document_type(&self) -> Option<&'static str> {
            Some(if self.0 { "test.comp" } else { "test.timeline" })
        }
        fn supports_composition(&self) -> bool {
            self.0
        }
        fn new_instance(&self) -> Option<EditorPanels> {
            Some(EditorPanels {
                editor: Box::new(Self(self.0)),
                inspector: Box::new(Self(self.0)),
            })
        }
        fn draw(&mut self, _: ExtensionUi<'_>) {}
    }
    let mut shell = Shell::new(
        [false, true]
            .into_iter()
            .map(|composition| {
                let panel = Editor(composition);
                RegisteredPanel {
                    descriptor: fold_platform::packages::PanelDescriptor {
                        id: panel.id(),
                        title: panel.id(),
                        placement: PanelPlacement::Editor,
                    },
                    key: WindowKey::new(panel.id(), panel.id()).unwrap(),
                    panel: Box::new(panel),
                }
            })
            .collect(),
    );
    let timeline = shell
        .workspace
        .editors
        .iter()
        .find(|(_, e)| e.contribution == "test.timeline")
        .map(|(&id, _)| id)
        .unwrap();
    let comp = shell.workspace.composition_editor.unwrap();
    let parent = DocumentId::new();
    let child = DocumentId::new();
    shell
        .workspace
        .editors
        .get_mut(&timeline)
        .unwrap()
        .bind(ViewLocation {
            document: parent,
            time: Time::new(7, 24).unwrap(),
            label: "Sequence".into(),
        });
    let viewer = *shell.workspace.viewers.keys().next().unwrap();
    shell.workspace.follow_source(viewer, timeline);
    let pinned = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Pinned(shell.workspace.editors[&timeline].output().unwrap()),
        ..Default::default()
    });
    let b = shell.workspace.add_editor("test.comp", "test.comp");
    shell
        .workspace
        .editors
        .get_mut(&b)
        .unwrap()
        .bind(ViewLocation {
            document: parent,
            time: Time::ZERO,
            label: "Other group".into(),
        });
    let other = shell.workspace.add_viewer(ViewerInstance {
        group: workspace::LinkGroup::B,
        source: Some("test.comp".into()),
        time: Time::new(3, 24).unwrap(),
        ..Default::default()
    });
    let selected = fold_foundation::ObjectId::new();
    shell
        .workspace
        .editors
        .get_mut(&timeline)
        .unwrap()
        .selection
        .objects = vec![selected];
    let before = shell.workspace.editors[&timeline].clone();
    let mut target = before.clone();
    target.navigate(ViewLocation {
        document: child,
        time: Time::new(1, 48).unwrap(),
        label: "Shot".into(),
    });
    target.document_type = "test.comp".into();
    shell.navigate_editor(timeline, target);
    shell.sync_instances();
    assert_eq!(
        shell.workspace.editors[&timeline].document(),
        Some(parent),
        "opening a composition must retain the timeline tab and document"
    );
    assert_eq!(
        shell.workspace.editors[&timeline].contribution,
        "test.timeline"
    );
    assert_eq!(shell.workspace.editors[&comp].document(), Some(child));
    assert_eq!(shell.workspace.composition_editor, Some(comp));
    assert_eq!(shell.workspace.network_editor, Some(comp));
    assert_eq!(shell.workspace.inspector_editor(), Some(comp));
    assert_eq!(shell.focus, Some(comp));
    assert_eq!(shell.workspace.resolve(viewer).unwrap().document, child);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(1, 48).unwrap()
    );
    assert_eq!(shell.workspace.resolve(pinned).unwrap().document, parent);
    assert_eq!(shell.workspace.resolve(other).unwrap().document, parent);
    assert_eq!(
        shell.workspace.viewers[&other].time,
        Time::new(3, 24).unwrap()
    );
    assert_eq!(
        shell.workspace.editors[&timeline].selection.objects,
        vec![selected]
    );
    shell.workspace.toggle_inspector_lock();
    let mut back = shell.workspace.editors[&comp].clone();
    back.back();
    back.document_type = "test.timeline".into();
    shell.navigate_editor(comp, back);
    shell.sync_instances();
    assert_eq!(shell.focus, Some(timeline));
    assert_eq!(
        shell
            .workspace
            .inspector_lock
            .as_ref()
            .unwrap()
            .1
            .document(),
        Some(child)
    );
    assert_eq!(
        shell.workspace.editors[&timeline].selection.objects,
        vec![selected]
    );
    assert_eq!(shell.workspace.editors[&comp].document(), Some(child));
    assert_eq!(shell.workspace.resolve(viewer).unwrap().document, parent);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(7, 24).unwrap()
    );
}
