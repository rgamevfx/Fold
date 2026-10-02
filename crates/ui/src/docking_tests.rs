//! Real dock-tab mouse gestures through the shared shell.
use super::*;
use crate::sdk::{Panel, project_drop};
use dear_imgui_rs::{ConfigFlags, Context, MouseButton};
use fold_platform::desktop::{DesktopState, PreviewResult};

struct Client(DesktopState);
impl DesktopClient for Client {
    fn state(&self) -> &DesktopState {
        &self.0
    }
    fn poll(&mut self) {}
    fn command(&mut self, _: DesktopCommand) {}
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
struct Canvas {
    project_drops: bool,
}
impl Panel for Canvas {
    fn id(&self) -> &'static str {
        "test.canvas"
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ui = context.ui;
        let origin = ui.cursor_screen_pos();
        let size = ui.content_region_avail();
        ui.invisible_button("canvas", size);
        if self.project_drops {
            assert!(
                project_drop::canvas_target(ui, origin, size).is_none(),
                "panel docking is not a Project item drop"
            );
        }
    }
}
#[test]
fn grabbing_and_dragging_a_dock_tab_keeps_the_shell_alive() {
    dock_drag(true);
}
#[test]
fn dock_tab_drag_without_project_overlay() {
    dock_drag(false);
}
fn dock_drag(project_drops: bool) {
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
    context.io_mut().set_display_size([1280., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut shell = Shell::new(vec![RegisteredPanel {
        descriptor: fold_platform::packages::PanelDescriptor {
            id: "test.canvas",
            title: "Canvas",
            placement: PanelPlacement::Editor,
        },
        panel: Box::new(Canvas { project_drops }),
        key: WindowKey::new("test.canvas", "Canvas").unwrap(),
    }]);
    let mut client = Client(DesktopState::default());
    let mut tab = [0.; 2];
    let mut saw_drag = false;
    for step in 0..16 {
        if step == 5 {
            context.io_mut().add_mouse_pos_event(tab);
        }
        if step == 6 {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true);
        }
        if (7..12).contains(&step) {
            context
                .io_mut()
                .add_mouse_pos_event([tab[0] + 100., tab[1] + 100.]);
        }
        if step == 12 {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false);
        }
        let ui = context.frame();
        shell.controls(ui, &mut client).unwrap();
        shell.viewer(ui, &Preview::Pending, "", &mut client);
        ui.window(&shell.viewer).build(|| {
            let pos = ui.window_pos();
            if step == 4 {
                tab = [pos[0] + 45., pos[1] + ui.text_line_height() * 0.5];
            }
        });
        saw_drag |= ui.drag_drop_payload().is_some();
        context.end_frame();
    }
    assert!(saw_drag, "gesture must actually start ImGui's docking drag");
}

#[test]
fn project_drag_delivers_once_and_preserves_the_canvas_cursor() {
    use dear_imgui_rs::Condition;
    use fold_platform::browser::Entry;
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([900., 500.]);
    context.io_mut().set_delta_time(1. / 60.);
    let entry = Entry::Bin(fold_foundation::BinId::new());
    let mut source = [0.; 2];
    let mut destination = [0.; 2];
    let mut delivered = Vec::new();
    for step in 0..16 {
        if step == 5 {
            context.io_mut().add_mouse_pos_event(source);
        }
        if step == 6 {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true);
        }
        if step == 7 {
            context
                .io_mut()
                .add_mouse_pos_event([source[0] + 15., source[1]]);
        }
        if (8..12).contains(&step) {
            context.io_mut().add_mouse_pos_event(destination);
        }
        if step == 12 {
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false);
        }
        let ui = context.frame();
        ui.window("Project source")
            .position([10., 10.], Condition::Always)
            .size([220., 200.], Condition::Always)
            .build(|| {
                let p = ui.cursor_screen_pos();
                source = [p[0] + 20., p[1] + 20.];
                ui.button_with_size("Item", [150., 50.]);
                project_drop::source(ui, entry, "Item");
            });
        ui.window("Canvas target")
            .position([300., 10.], Condition::Always)
            .size([350., 300.], Condition::Always)
            .build(|| {
                let origin = ui.cursor_screen_pos();
                let size = ui.content_region_avail();
                destination = [origin[0] + size[0] / 2., origin[1] + size[1] / 2.];
                ui.invisible_button("canvas", size);
                let cursor = ui.cursor_screen_pos();
                if let Some(entry) = project_drop::canvas_target(ui, origin, size) {
                    delivered.push(entry);
                }
                assert_eq!(
                    ui.cursor_screen_pos(),
                    cursor,
                    "the overlay must not move subsequent controls"
                );
            });
        context.end_frame();
    }
    assert_eq!(delivered, vec![entry]);
}
