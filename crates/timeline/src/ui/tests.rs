use super::interaction::*;

#[test]
fn panel_toolbar_fits_wide_and_narrow_windows_without_editing() {
    use super::*;
    use fold_platform::desktop::*;
    use fold_project::{CommittedSnapshot, EditBatch, Mutation, Project};
    use fold_ui::sdk::imgui::{Condition, Context};

    struct Host {
        project: Project,
        state: DesktopState,
    }
    impl DesktopClient for Host {
        fn state(&self) -> &DesktopState {
            &self.state
        }
        fn snapshot(&self) -> Option<CommittedSnapshot> {
            Some(self.project.snapshot())
        }
        fn poll(&mut self) {}
        fn command(&mut self, command: DesktopCommand) {
            panic!("drawing dispatched {command:?}");
        }
        fn request_preview(&mut self, _: PreviewKey) {}
        fn cancel_preview(&mut self) {}
        fn take_preview(&mut self) -> Option<PreviewResult> {
            None
        }
    }
    let mut project = Project::new(8);
    let document = DocumentId::new();
    let sequence = Sequence {
        tracks: vec![crate::Track::new(TrackKind::Video, "V1")],
        clips: vec![],
        dimensions: [640, 360],
        rate: [24, 1],
        extensions: Default::default(),
    };
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(sequence.document(document).unwrap())],
        })
        .unwrap();
    let mut host = Host {
        project,
        state: DesktopState::default(),
    };
    host.state.selection.document = Some(document);
    let before = host.project.snapshot().revision();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 700.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut panel = TimelinePanel::default();
    for width in [850., 320.] {
        for frame in 0..8 {
            let ui = context.frame();
            ui.window("Timeline")
                .position([0.; 2], Condition::Always)
                .size([width, 650.], Condition::Always)
                .build(|| {
                    panel.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
                    if frame >= 3 {
                        assert_eq!(ui.scroll_max_x(), 0.);
                    }
                });
            assert!(context.render_legacy().total_vtx_count() > 100);
        }
    }
    assert_eq!(host.project.snapshot().revision(), before);
}

#[test]
fn zoom_keeps_pointer_time_and_handles_are_scale_aware() {
    let mut view = View {
        first: 100.0,
        pixels_per_frame: 5.0,
        scroll_y: 0.0,
    };
    let before = view.frame_at(400.0, 100.0);
    view.zoom(2.0, 400.0, 100.0);
    assert_eq!(view.frame_at(400.0, 100.0), before);
    let rect = Rect {
        min: [100.0, 0.0],
        max: [300.0, 50.0],
    };
    assert_eq!(handle(rect, [102.0, 20.0], 1.0), Handle::Left);
    assert_eq!(handle(rect, [298.0, 20.0], 1.0), Handle::Right);
    assert_eq!(handle(rect, [150.0, 20.0], 1.0), Handle::Move);
}
