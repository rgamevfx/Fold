use super::{Editor, Tool};
use crate::roto::{Combine, Layer};
use fold_foundation::{ObjectId, Time};
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::{
    EditResponse,
    imgui::{Drag, StyleColor, Ui},
    toolbar::{ToolbarIcon, icon_button, tooltip},
};
pub(super) fn toolbar(ui: &Ui, editor: &mut Editor, reserve: f32) {
    let width = (ui.content_region_avail()[0] - reserve).max(1.);
    let slots = ((width / (ui.frame_height() + ui.clone_style().item_spacing()[0] + 4.)).floor()
        as usize)
        .clamp(1, 5);
    let tools = [
        (
            Tool::Select,
            ToolbarIcon::Select,
            "Select (V)\nDrag points or box. Corners scale. Ctrl-corner pins.\nTop handle rotates. Shift adds points.\nDouble-click curve inserts. Alt breaks tangents.",
        ),
        (
            Tool::Rectangle,
            ToolbarIcon::Rectangle,
            "Rectangle (R) — Shift constrains to square",
        ),
        (
            Tool::Ellipse,
            ToolbarIcon::Ellipse,
            "Ellipse (E) — Shift constrains to circle",
        ),
        (
            Tool::Bezier,
            ToolbarIcon::Pen,
            "Bézier (B) — click for corners; drag for tangents; Enter closes",
        ),
        (
            Tool::BSpline,
            ToolbarIcon::Curve,
            "B-spline (S) — click to add points; Enter closes",
        ),
    ];
    for (i, (tool, icon, tip)) in tools.iter().enumerate() {
        if i >= slots && slots < 5 {
            break;
        }
        if i > 0 {
            ui.same_line();
        }
        if slots < 5 && i == slots - 1 {
            if icon_button(ui, "roto-more", ToolbarIcon::More, "More Roto tools") {
                ui.open_popup("roto-tools");
            }
            if let Some(_popup) = ui.begin_popup("roto-tools") {
                for (tool, _, tip) in &tools[i..] {
                    if ui.menu_item(tip) {
                        editor.tool = *tool;
                        editor.reset_gesture();
                    }
                }
            }
            break;
        }
        let _active = (editor.tool == *tool).then(|| {
            ui.push_style_color(StyleColor::Button, ui.style_color(StyleColor::ButtonActive))
        });
        if icon_button(ui, tip, *icon, tip) {
            editor.tool = *tool;
            editor.reset_gesture();
        }
    }
}
fn tree(ui: &Ui, l: &mut Layer, editor: &mut Editor, response: &mut EditResponse) {
    let _id = ui.push_id(&l.id.to_string());
    let label = format!(
        "{}{}{}",
        l.name,
        if l.locked { " (locked)" } else { "" },
        if l.enabled { "" } else { " (hidden)" }
    );
    let token = ui
        .tree_node_config("layer")
        .label(&label)
        .open_on_arrow(true)
        .span_avail_width(true)
        .leaf(l.shape.is_some())
        .selected(editor.layer == Some(l.id))
        .opened(true, fold_ui::sdk::imgui::Condition::Once)
        .push();
    if ui.is_item_clicked() {
        editor.layer = Some(l.id);
        editor.points.clear();
    }
    if let Some(_popup) = ui.begin_popup_context_item() {
        super::super::controls::check(ui, "Enabled", &mut l.enabled, response);
        super::super::controls::check(ui, "Locked", &mut l.locked, response);
    }
    if let Some(_tree) = token {
        for child in &mut l.children {
            tree(ui, child, editor, response);
        }
    }
}
fn remove(l: &mut Layer, id: ObjectId) -> bool {
    if let Some(i) = l.children.iter().position(|v| v.id == id) {
        l.children.remove(i);
        true
    } else {
        l.children.iter_mut().any(|c| remove(c, id))
    }
}
fn reorder(l: &mut Layer, id: ObjectId, delta: isize) -> bool {
    if let Some(i) = l.children.iter().position(|v| v.id == id) {
        let next = i as isize + delta;
        if next >= 0 && (next as usize) < l.children.len() {
            l.children.swap(i, next as usize);
            return true;
        }
        false
    } else {
        l.children.iter_mut().any(|c| reorder(c, id, delta))
    }
}
pub(in crate::ui) fn draw_inspector(
    ui: &Ui,
    state: &mut super::super::state::State,
    host: &mut dyn DesktopClient,
    id: ObjectId,
    time: Time,
    auto_key: bool,
) {
    let shared = state.roto.clone();
    let mut editor = shared.borrow_mut();
    editor.select(id);
    let Ok(before) = state.roto_at(id, time) else {
        return;
    };
    let Some(graph) = state.graph.as_ref() else {
        return;
    };
    let mut settings = before.clone();
    editor.layer = editor
        .layer
        .filter(|id| settings.root.find(*id).is_some())
        .or(Some(settings.root.id));
    let mut response = EditResponse::default();
    ui.text("Roto");
    ui.separator();
    super::super::controls::row(ui, "Channels");
    let preview = settings
        .channels
        .iter()
        .map(|c| c.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    if let Some(_combo) = ui.begin_combo("##roto-channels", preview) {
        let names = graph
            .node(id)
            .ok()
            .and_then(|n| n.input_ids().first().copied().flatten())
            .and_then(|input| graph.channel_names_with(input, &|r| host.channels(r)).ok())
            .unwrap_or_default();
        let names: std::collections::BTreeSet<_> = names
            .into_iter()
            .chain(fold_render::channels::RGBA.map(|n| n.to_owned().try_into().unwrap()))
            .collect();
        for name in names {
            let mut on = settings.channels.contains(&name);
            if ui.checkbox(name.as_str(), &mut on) {
                settings.channels.retain(|n| n != &name);
                if on {
                    settings.channels.push(name);
                }
                response.changed = true;
                response.finished = true;
            }
        }
    }
    // The shared compositor currently evaluates a fixed-format raster. Do not
    // offer a switch that promises an unimplemented unbounded data window.
    {
        let _disabled = ui.begin_disabled();
        ui.checkbox("Clip to format", &mut settings.clip_to_format);
    }
    tooltip(
        ui,
        "The current compositor output is bounded by its format.",
    );
    super::super::controls::check(ui, "Premultiply", &mut settings.premultiply, &mut response);
    {
        let _disabled = ui.begin_disabled_with_cond(editor.gesture.is_some());
        toolbar(ui, &mut editor, 0.);
    }
    if super::keys::row(ui, &mut editor, state, host, id, &before, time) {
        return;
    }
    ui.child_window("roto-shapes")
        .size([0., ui.frame_height() * 6.])
        .build(ui, || {
            tree(ui, &mut settings.root, &mut editor, &mut response)
        });
    if icon_button(ui, "roto-group", ToolbarIcon::Add, "Add layer") {
        let layer = Layer::group("Layer");
        let next = layer.id;
        let target = editor
            .layer
            .and_then(|id| settings.root.find(id))
            .filter(|l| l.shape.is_none())
            .map(|l| l.id)
            .unwrap_or(settings.root.id);
        if settings.root.editable(target) {
            settings.root.find_mut(target).unwrap().children.push(layer);
            editor.layer = Some(next);
            response.changed = true;
            response.finished = true;
        }
    }
    ui.same_line();
    if let Some(_menu) = fold_ui::sdk::toolbar::menu_button(ui, "Shape", "Shape actions") {
        if let Some(selected) = editor.layer {
            let editable = settings.root.editable(selected);
            let _disabled = ui.begin_disabled_with_cond(!editable);
            if ui.menu_item("Move up") {
                response.changed |= reorder(&mut settings.root, selected, -1);
                response.finished = true;
            }
            if ui.menu_item("Move down") {
                response.changed |= reorder(&mut settings.root, selected, 1);
                response.finished = true;
            }
            if selected != settings.root.id && ui.menu_item("Delete") {
                response.changed |= remove(&mut settings.root, selected);
                response.finished = true;
                editor.layer = Some(settings.root.id);
            }
        }
    }
    let selected = editor.layer.unwrap_or(settings.root.id);
    let editable = settings.root.editable(selected);
    let _disabled = ui.begin_disabled_with_cond(!editable);
    if let Some(l) = settings.root.find_mut(selected) {
        super::super::controls::row(ui, "Name");
        let changed = ui.input_text("##roto-name", &mut l.name).build();
        response.item(ui, changed);
        super::super::controls::combo(
            ui,
            "Operation",
            &mut l.combine,
            &[
                (Combine::Over, "Over"),
                (Combine::Add, "Add"),
                (Combine::Subtract, "Subtract"),
                (Combine::Intersect, "Intersect"),
                (Combine::Replace, "Replace"),
            ],
            &mut response,
        );
        super::super::controls::check(ui, "Invert", &mut l.invert, &mut response);
    }
    let Some(node) = state.graph.as_mut().and_then(|g| g.node_mut(id).ok()) else {
        return;
    };
    // Use the shared animation widgets and tracks, restricted to this selection.
    let prefix = format!("roto/{selected}/");
    let mut curves = node.animation.project().unwrap_or_default();
    let mut properties = settings
        .properties()
        .into_iter()
        .filter(|p| p.id.starts_with(&prefix) && !p.id.contains("/point/"))
        .collect::<Vec<_>>();
    properties.sort_by_key(|p| match p.id.rsplit('/').next().unwrap_or("") {
        "opacity" => 0,
        "feather" => 1,
        "falloff" => 2,
        "translate" => 3,
        "rotate" => 4,
        "scale" => 5,
        _ => 6,
    });
    let mut transform = None;
    let mut transform_visible = false;
    for mut property in properties {
        let part = property.id.rsplit('/').next().unwrap_or("");
        if part == "translate" {
            transform = ui.tree_node("Transform");
            transform_visible = transform.is_some();
        }
        if matches!(part, "translate" | "rotate" | "scale" | "center") && !transform_visible {
            continue;
        }

        let label = property
            .label
            .rsplit(" · ")
            .next()
            .unwrap_or(&property.label);
        let bound = vec![None; property.values.len()];
        fold_ui::sdk::animated_property::AnimatedProperty {
            id: &property.id,
            label,
            components: property.channels,
            unit: property.unit,
            range: property.range,
            color: None,
        }
        .draw_compact(
            ui,
            &mut property.values,
            &mut curves,
            time,
            if matches!(part, "feather" | "falloff") {
                editor.auto_key
            } else {
                auto_key
            },
            &mut response,
            &mut |_, _| {},
            None,
            &bound,
        );
        if let Err(e) = settings.set_property(&property.id, &property.values) {
            state.error = e;
        }
    }
    drop(transform);
    if let Some(_details) = ui.tree_node("Selected points") {
        if editor.points.is_empty() {
            ui.text_disabled("Select points in the viewer.");
        }
        for mut property in settings.properties().into_iter().filter(|p| {
            p.id.starts_with(&prefix)
                && editor
                    .points
                    .iter()
                    .any(|id| p.id.contains(&format!("/point/{id}/")))
        }) {
            let label = property
                .label
                .rsplit(" · ")
                .next()
                .unwrap_or(&property.label);
            let bound = vec![None; property.values.len()];
            fold_ui::sdk::animated_property::AnimatedProperty {
                id: &property.id,
                label,
                components: property.channels,
                unit: "px",
                range: None,
                color: None,
            }
            .draw_compact(
                ui,
                &mut property.values,
                &mut curves,
                time,
                editor.auto_key,
                &mut response,
                &mut |_, _| {},
                None,
                &bound,
            );
            if let Err(e) = settings.set_property(&property.id, &property.values) {
                state.error = e;
            }
        }
    }
    if let Some(_details) = ui.tree_node("Lifetime and motion blur") {
        if let Some(l) = settings.root.find_mut(selected) {
            let mut enabled = l.lifetime.is_some();
            if ui.checkbox("Limit lifetime", &mut enabled) {
                l.lifetime = enabled.then_some([
                    Time::ZERO,
                    state
                        .original
                        .as_ref()
                        .unwrap()
                        .info
                        .time(state.original.as_ref().unwrap().info.frames)
                        .unwrap(),
                ]);
                response.changed = true;
                response.finished = true;
            }
            if let Some(range) = &mut l.lifetime {
                for (i, label) in ["Start (s)", "End (s)"].into_iter().enumerate() {
                    super::super::controls::row(ui, label);
                    let mut seconds = fold_animation::seconds(range[i]);
                    let changed = Drag::new(format!("##life{i}"))
                        .speed(0.01)
                        .build(ui, &mut seconds);
                    if changed && seconds.is_finite() {
                        if let Ok(t) = Time::new((seconds * 1e6).round() as i64, 1_000_000) {
                            range[i] = t;
                        }
                    }
                    response.item(ui, changed);
                }
            }
        }
        super::super::controls::row(ui, "Samples");
        let changed = Drag::new("##roto-samples")
            .range(0, 64)
            .build(ui, &mut settings.motion_samples);
        response.item(ui, changed);
        super::super::controls::row(ui, "Shutter");
        let mut shutter = fold_animation::seconds(settings.shutter);
        let changed = Drag::new("##roto-shutter")
            .speed(0.01)
            .range(0., 2.)
            .build(ui, &mut shutter);
        if changed {
            settings.shutter =
                Time::new((shutter * 1e6).round() as i64, 1_000_000).unwrap_or(settings.shutter);
        }
        response.item(ui, changed);
        use crate::transform::ShutterOffset;
        super::super::controls::combo(
            ui,
            "Offset",
            &mut settings.shutter_offset,
            &[
                (ShutterOffset::Start, "Start"),
                (ShutterOffset::Center, "Center"),
                (ShutterOffset::End, "End"),
            ],
            &mut response,
        );
    }
    drop(_disabled);
    response.cancel_on_escape(ui, state.property_editing);
    if response.cancelled {
        state.cancel(host);
    } else if response.changed {
        let result = (|| {
            for property in settings.properties() {
                if super::keys::geometry(&property.id).is_some() {
                    let values = property
                        .values
                        .iter()
                        .enumerate()
                        .map(|(i, value)| {
                            curves
                                .get(&crate::animation::path(&property.id, i))
                                .and_then(|curve| curve.sample(time))
                                .unwrap_or(*value)
                        })
                        .collect::<Vec<_>>();
                    settings.set_property(&property.id, &values)?;
                }
            }
            let mut draft = node.clone();
            draft.animation.replace(&curves)?;
            super::edit(&mut draft, &before, settings, time, editor.auto_key)?;
            *node = draft;
            Ok::<_, String>(())
        })();
        match result {
            Ok(()) => {
                state.property_editing = true;
                state.preview(host);
            }
            Err(e) => state.error = e,
        }
    }
    if state.property_editing && state.editing && (response.finished || !ui.is_any_item_active()) {
        state.commit(host);
    }
    if !state.error.is_empty() {
        ui.text_wrapped(&state.error);
    }
}
