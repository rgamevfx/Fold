use super::*;
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, DesktopState, PreviewKey, PreviewResult},
    workspace::PanelInstanceId,
};
struct Host(DesktopState);
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.0
    }
    fn panel_instance(&self) -> Option<PanelInstanceId> {
        Some(PanelInstanceId(2))
    }
    fn poll(&mut self) {}
    fn command(&mut self, _: DesktopCommand) {
        panic!("a different viewer must not act on the gesture");
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
#[test]
fn another_viewer_cannot_continue_or_cancel_an_owned_overlay_gesture() {
    let _guard = super::super::IMGUI_TEST_LOCK.lock().unwrap();
    let mut overlay = Overlay {
        tools: Default::default(),
        picking: Default::default(),
        gesture: Some(Gesture {
            handle: Handle {
                node: ObjectId::new(),
                target: Target::Size,
                point: [10., 10.],
                matrix: IDENTITY,
            },
            start: [0., 0.],
            base: Motion::empty(),
            generation: 0,
            viewer: Some(PanelInstanceId(1)),
            time: fold_foundation::Time::new(7, 48).unwrap(),
        }),
    };
    let state = Shared::default();
    let mut host = Host(Default::default());
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([640., 480.]);
    context.io_mut().set_delta_time(1. / 60.);
    let ui = context.frame();
    ui.window("Other viewer").build(|| {
        overlay.draw(
            &state,
            ExtensionUi {
                ui,
                host: &mut host,
            },
            ViewerRect {
                editable: true,
                image_current: true,
                canvas_origin: [0., 0.],
                canvas_size: [640., 480.],
                origin: [0., 0.],
                size: [100., 100.],
                dimensions: [100, 100],
            },
        );
    });
    context.end_frame();
    assert_eq!(
        overlay.gesture.as_ref().unwrap().viewer,
        Some(PanelInstanceId(1))
    );
    assert_eq!(
        overlay.gesture.as_ref().unwrap().time,
        fold_foundation::Time::new(7, 48).unwrap()
    );
    assert_eq!(state.borrow().generation, 0);
    overlay.cancel();
    assert!(overlay.gesture.is_none());
}
