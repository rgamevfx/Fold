//! Compositor controls use the shared inspector rows and edit lifecycle.
pub(super) use fold_ui::sdk::property_rows::{check, combo, row};
use fold_ui::sdk::{EditResponse, imgui::Ui};

pub(super) fn blur(
    ui: &Ui,
    filter: &mut fold_render::operations::BlurFilter,
    edges: &mut fold_render::operations::Edges,
    response: &mut EditResponse,
) {
    use fold_render::operations::{BlurFilter, Edges};
    combo(
        ui,
        "Filter",
        filter,
        &[(BlurFilter::Gaussian, "Gaussian"), (BlurFilter::Box, "Box")],
        response,
    );
    combo(
        ui,
        "Edges",
        edges,
        &[
            (Edges::Transparent, "Black outside"),
            (Edges::Clamp, "Extend"),
        ],
        response,
    );
}

pub(super) fn transform(ui: &Ui, settings: &mut crate::Transform, response: &mut EditResponse) {
    use crate::transform::{ShutterOffset, SkewOrder};
    use fold_render::operations::{Edges, Filter};
    combo(
        ui,
        "Skew order",
        &mut settings.skew_order,
        &[(SkewOrder::XY, "XY"), (SkewOrder::YX, "YX")],
        response,
    );
    check(ui, "Invert", &mut settings.invert, response);
    ui.separator();
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
    check(ui, "Clamp", &mut settings.clamp, response);
    ui.same_line();
    let mut black = settings.edges == Edges::Transparent;
    check(ui, "Black outside", &mut black, response);
    settings.edges = if black {
        Edges::Transparent
    } else {
        Edges::Clamp
    };
    if let Some(_shutter) = ui.tree_node("Motion blur") {
        row(ui, "Samples");
        let changed = fold_ui::sdk::imgui::Drag::new("##samples")
            .range(0, 64)
            .flags(fold_ui::sdk::imgui::DragFlags::ALWAYS_CLAMP)
            .build(ui, &mut settings.motion_samples);
        response.item(ui, changed);
        let mut shutter = fold_animation::seconds(settings.shutter);
        row(ui, "Shutter");
        let changed = fold_ui::sdk::imgui::Drag::new("##shutter")
            .speed(0.01)
            .try_display_format("%.3f")
            .expect("shutter format")
            .range(0., 2.)
            .flags(fold_ui::sdk::imgui::DragFlags::ALWAYS_CLAMP)
            .build(ui, &mut shutter);
        if changed && shutter.is_finite() {
            settings.shutter =
                fold_foundation::Time::new((shutter * 1000000.).round() as i64, 1000000).unwrap();
        }
        response.item(ui, changed);
        combo(
            ui,
            "Offset",
            &mut settings.shutter_offset,
            &[
                (ShutterOffset::Start, "Start"),
                (ShutterOffset::Center, "Center"),
                (ShutterOffset::End, "End"),
            ],
            response,
        );
    }
}

pub(super) fn grade(
    ui: &Ui,
    settings: &mut fold_render::operations::Grade,
    response: &mut EditResponse,
) {
    check(ui, "Black clamp", &mut settings.black_clamp, response);
    ui.same_line();
    check(ui, "White clamp", &mut settings.white_clamp, response);
    check(ui, "Invert", &mut settings.invert, response);
    if let Some(_processing) = ui.tree_node("Processing") {
        check(ui, "Unpremultiply", &mut settings.unpremultiply, response);
    }
}
