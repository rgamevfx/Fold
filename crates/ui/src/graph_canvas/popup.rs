//! Missing safe next-window geometry bindings for non-modal popups in dear-imgui 0.18.
use crate::sdk::imgui;

#[allow(unsafe_code)]
pub(super) fn geometry(ui: &imgui::Ui, position: [f32; 2], size: [f32; 2]) {
    assert!(position.into_iter().chain(size).all(f32::is_finite));
    assert!(size.into_iter().all(|v| v > 0.));
    // SAFETY: the live Ui binds its owning context. Both calls copy finite
    // value arguments into that context; no pointers are passed or retained.
    ui.with_bound_context(|| unsafe {
        imgui::sys::igSetNextWindowPos(
            imgui::sys::ImVec2_c {
                x: position[0],
                y: position[1],
            },
            imgui::Condition::Always as i32,
            imgui::sys::ImVec2_c { x: 0., y: 0. },
        );
        imgui::sys::igSetNextWindowSize(
            imgui::sys::ImVec2_c {
                x: size[0],
                y: size[1],
            },
            imgui::Condition::Always as i32,
        );
    });
}
