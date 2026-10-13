//! Photographic node selectors share the normal property gesture lifecycle.
use super::controls::{check, combo};
use crate::OperationConfig as P;
use fold_ui::sdk::{EditResponse, imgui::Ui};
pub(super) fn draw(ui: &Ui, config: &mut P, response: &mut EditResponse) {
    match config {
        P::ColorShift { settings } => {
            check(ui, "Shift to color", &mut settings.use_target, response)
        }
        P::ChromaticAberration { settings } => {
            if let Some(_section) = ui.tree_node("Sampling") {
                use fold_render::operations::Edges;
                combo(
                    ui,
                    "Edges",
                    &mut settings.edges,
                    &[
                        (Edges::Clamp, "Extend"),
                        (Edges::Transparent, "Black outside"),
                    ],
                    response,
                );
            }
        }
        P::LensDistortion { settings } => {
            combo(
                ui,
                "Direction",
                &mut settings.undistort,
                &[(false, "Distort"), (true, "Undistort")],
                response,
            );
            if let Some(_section) = ui.tree_node("Sampling") {
                use fold_render::operations::{Edges, Filter};
                combo(
                    ui,
                    "Filter",
                    &mut settings.filter,
                    &[
                        (Filter::Nearest, "Nearest"),
                        (Filter::Linear, "Linear"),
                        (Filter::Cubic, "Cubic"),
                    ],
                    response,
                );
                combo(
                    ui,
                    "Edges",
                    &mut settings.edges,
                    &[
                        (Edges::Transparent, "Black outside"),
                        (Edges::Clamp, "Extend"),
                    ],
                    response,
                );
            }
        }
        P::FilmEmulation { settings } => {
            let choices: Vec<_> = crate::filters::FilmPreset::ALL
                .into_iter()
                .map(|p| (p, p.label()))
                .collect();
            combo(ui, "Film look", &mut settings.preset, &choices, response);
            if ui.is_item_hovered() {
                ui.tooltip_text("Original scene-linear film-inspired looks. Changing the look preserves animation and links.");
            }
        }
        P::Grain { settings } => {
            use crate::filters::NoiseMode;
            combo(
                ui,
                "Mode",
                &mut settings.mode,
                &[
                    (NoiseMode::Film, "Film grain"),
                    (NoiseMode::Sensor, "Sensor noise"),
                ],
                response,
            );
            check(ui, "Freeze pattern", &mut settings.freeze, response);
        }
        P::Retime { settings } => {
            use crate::retime::{Interpolation, Outside};
            combo(
                ui,
                "Interpolation",
                &mut settings.interpolation,
                &[
                    (Interpolation::Nearest, "Nearest frame"),
                    (Interpolation::Blend, "Frame blend"),
                    (Interpolation::Rife, "RIFE"),
                ],
                response,
            );
            if settings.interpolation == Interpolation::Rife {
                if let Some(_section) = ui.tree_node("RIFE options") {
                    combo(
                        ui,
                        "Motion resolution",
                        &mut settings.motion_scale,
                        &[(1, "Full"), (2, "Half"), (4, "Quarter")],
                        response,
                    );
                    super::controls::row(ui, "Cut threshold");
                    let changed = fold_ui::sdk::imgui::Drag::new("##rife-cut")
                        .speed(0.005)
                        .range(0., 1.)
                        .flags(fold_ui::sdk::imgui::DragFlags::ALWAYS_CLAMP)
                        .build(ui, &mut settings.cut_threshold);
                    response.item(ui, changed);
                    if ui.is_item_hovered() {
                        ui.tooltip_text("Hold the nearest frame across detected cuts. Lower values detect smaller changes; zero disables detection.");
                    }
                }
            }
            combo(
                ui,
                "Outside range",
                &mut settings.outside,
                &[
                    (Outside::Hold, "Hold"),
                    (Outside::Black, "Black"),
                    (Outside::Loop, "Loop"),
                    (Outside::Bounce, "Bounce"),
                ],
                response,
            );
            check(
                ui,
                "Motion blur##retime-enabled",
                &mut settings.motion_blur,
                response,
            );
            if settings.motion_blur {
                combo(
                    ui,
                    "Samples",
                    &mut settings.samples,
                    &[
                        (4, "4 — preview"),
                        (8, "8"),
                        (16, "16"),
                        (32, "32"),
                        (64, "64 — fine"),
                    ],
                    response,
                );
            }
            ui.text_disabled(format!(
                "Source grid: {:.3} fps",
                f64::from(settings.rate[0]) / f64::from(settings.rate[1])
            ));
        }
        _ => {}
    }
}
pub(super) fn retime_helpers(
    ui: &Ui,
    node: &mut crate::Node,
    at: fold_foundation::Time,
    evaluated: Option<&P>,
    response: &mut EditResponse,
) {
    let Ok(prepared) = node.prepare() else {
        return;
    };
    let P::Retime { settings } = prepared.config() else {
        return;
    };
    let settings = settings.clone();
    let Some(_menu) = fold_ui::sdk::toolbar::menu_button(
        ui,
        "Timing actions",
        "Reset, reverse or freeze the source-frame curve",
    ) else {
        return;
    };
    let mut next = node.clone();
    let result = if ui.menu_item("Reset timing") {
        settings
            .identity_curve()
            .and_then(|c| next.animation.insert("source_frame.0".into(), c))
    } else if ui.menu_item("Reverse") {
        let mut curve = settings.identity_curve();
        if let Ok(c) = &mut curve {
            for k in &mut c.keys {
                k.value = settings.last - k.value;
            }
        }
        curve.and_then(|c| next.animation.insert("source_frame.0".into(), c))
    } else if ui.menu_item("Freeze here") {
        let value = match evaluated {
            Some(P::Retime { settings }) => settings.source_frame,
            _ => settings.source_frame,
        };
        let mut c = fold_animation::Curve::default();
        c.insert(at, value);
        next.animation.insert("source_frame.0".into(), c)
    } else {
        return;
    };
    match result {
        Ok(()) => {
            *node = next;
            response.changed = true;
            response.finished = true;
        }
        Err(e) => ui.text_wrapped(e),
    }
}
