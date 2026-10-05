//! Compact property keying shared by feature inspectors. Callers retain the
//! authored base separately from the sampled value and own the transaction.
use crate::sdk::{
    EditResponse,
    imgui::{Drag, DragFlags, Key, MouseButton, Ui},
    toolbar::tooltip,
};
use fold_animation::Curve;
use fold_foundation::Time;
use std::collections::BTreeMap;

pub struct AnimatedProperty<'a> {
    pub id: &'a str,
    pub label: &'a str,
    pub components: &'a [&'a str],
    pub unit: &'a str,
    pub range: Option<[f64; 2]>,
    /// Color properties retain the shared sRGB picker; bool selects ACES working conversion.
    pub color: Option<bool>,
}
#[derive(Clone, Copy)]
enum Action {
    Insert,
    Remove,
    Clear,
}
fn menu_items(ui: &Ui, keyed: bool, animated: bool) -> Option<Action> {
    let mut action = None;
    {
        if ui.menu_item(if keyed {
            "Update keyframe"
        } else {
            "Add keyframe"
        }) {
            action = Some(Action::Insert);
        }
        if ui.menu_item_enabled_selected_no_shortcut("Remove keyframe", false, keyed) {
            action = Some(Action::Remove);
        }
        if ui.menu_item_enabled_selected_no_shortcut("Remove animation", false, animated) {
            action = Some(Action::Clear);
        }
    }
    action
}
fn interpolation_menu(
    ui: &Ui,
    paths: &[String],
    channels: &mut BTreeMap<String, Curve>,
    response: &mut EditResponse,
) {
    if paths.iter().any(|p| channels.contains_key(p))
        && let Some(_menu) = ui.begin_menu("Interpolation")
    {
        for (label, mode) in [
            ("Linear", fold_animation::Interpolation::Linear),
            ("Hold", fold_animation::Interpolation::Hold),
            ("Bezier", fold_animation::Interpolation::Bezier),
        ] {
            if ui.menu_item(label) {
                for path in paths {
                    if let Some(curve) = channels.get_mut(path) {
                        for key in &mut curve.keys {
                            key.interpolation = mode;
                        }
                    }
                }
                response.changed = true;
                response.finished = true;
            }
        }
    }
}
impl AnimatedProperty<'_> {
    pub fn draw(
        &self,
        ui: &Ui,
        base: &mut [f64],
        channels: &mut BTreeMap<String, Curve>,
        time: Time,
        auto_key: bool,
        response: &mut EditResponse,
    ) {
        self.draw_with_menu(ui, base, channels, time, auto_key, response, &mut |_| {});
    }

    /// Append feature actions to both the label and component context menus.
    pub fn draw_with_menu(
        &self,
        ui: &Ui,
        base: &mut [f64],
        channels: &mut BTreeMap<String, Curve>,
        time: Time,
        auto_key: bool,
        response: &mut EditResponse,
        menu: &mut dyn FnMut(&Ui),
    ) {
        let _id = ui.push_id(self.id);
        let paths: Vec<_> = (0..base.len())
            .map(|i| format!("{}.{i}", self.id))
            .collect();
        let values: Vec<_> = paths
            .iter()
            .zip(base.iter())
            .map(|(p, b)| channels.get(p).and_then(|c| c.sample(time)).unwrap_or(*b))
            .collect();
        let animated = paths.iter().any(|p| channels.contains_key(p));
        let keyed = paths
            .iter()
            .any(|p| channels.get(p).is_some_and(|c| c.at(time).is_some()));
        let mut group = None;
        let start = ui.cursor_screen_pos();
        let height = ui.frame_height();
        let width = ui.content_region_avail()[0];
        let label_width = (width * 0.36).clamp(height, 120.);
        ui.invisible_button_flags(
            "label",
            [label_width, height],
            crate::sdk::imgui::ButtonFlags::ENABLE_NAV,
        );
        let label_focused = ui.is_item_focused();
        let label_hovered = ui.is_item_hovered();
        if label_hovered && ui.io().key_alt() && ui.is_mouse_clicked(MouseButton::Left) {
            group = Some(Action::Insert);
        }
        if label_focused && (ui.is_key_pressed(Key::Enter) || ui.is_key_pressed(Key::Space)) {
            group = Some(Action::Insert);
        }
        if self.color.is_some()
            && label_hovered
            && !ui.io().key_alt()
            && ui.is_mouse_clicked(MouseButton::Left)
        {
            ui.open_popup("color-picker");
        }
        let mut edit_color =
            |ui: &Ui, channels: &mut BTreeMap<String, Curve>, response: &mut EditResponse| {
                if let Some(aces) = self.color
                    && let Ok(mut color) = <[f64; 4]>::try_from(values.as_slice())
                {
                    let locked = paths
                        .iter()
                        .any(|p| channels.get(p).is_some_and(|c| c.at(time).is_none()))
                        && !auto_key;
                    let _disabled = locked.then(|| ui.begin_disabled());
                    let before = color;
                    let mut edit = EditResponse::default();
                    crate::sdk::color_controls::authored(ui, &mut color, aces, &mut edit);
                    for (i, (&value, &previous)) in color.iter().zip(&before).enumerate() {
                        if value != previous && value.is_finite() {
                            if auto_key || channels.contains_key(&paths[i]) {
                                channels
                                    .entry(paths[i].clone())
                                    .or_default()
                                    .insert(time, value);
                            } else {
                                base[i] = value;
                            }
                        }
                    }
                    response.changed |= edit.changed;
                    response.finished |= edit.finished;
                }
            };
        if let Some(_popup) = ui.begin_popup_context_item() {
            group = menu_items(ui, keyed, animated).or(group);
            interpolation_menu(ui, &paths, channels, response);
            menu(ui);
            if self.color.is_some()
                && let Some(_picker) = ui.begin_menu("Color picker")
            {
                edit_color(ui, channels, response);
            }
        }
        if let Some(_picker) = ui.begin_popup("color-picker") {
            edit_color(ui, channels, response);
        }
        tooltip(
            ui,
            &format!(
                "{}{} — Alt-click to key all components; right-click for animation{}",
                self.label,
                if self.unit.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", self.unit)
                },
                if self.color.is_some() {
                    "; click label or swatch for color"
                } else {
                    ""
                }
            ),
        );
        let draw = ui.get_window_draw_list();
        let colors = crate::sdk::EditorColors::from_ui(ui);
        if label_focused {
            draw.add_rect(
                start,
                [start[0] + label_width, start[1] + height],
                colors.selected,
            )
            .build();
        }
        let center = [start[0] + height * 0.27, start[1] + height * 0.5];
        let r = height * 0.13;
        let points = vec![
            [center[0], center[1] - r],
            [center[0] + r, center[1]],
            [center[0], center[1] + r],
            [center[0] - r, center[1]],
        ];
        draw.add_polyline(
            points,
            if animated {
                colors.playhead
            } else {
                colors.muted
            },
        )
        .closed(true)
        .filled(keyed)
        .build();
        if animated && !keyed {
            draw.add_line(
                [center[0] - r * 0.6, center[1]],
                [center[0] + r * 0.6, center[1]],
                colors.playhead,
            )
            .build();
        }
        let swatch = self.color.and_then(|aces| {
            <[f64; 4]>::try_from(values.as_slice())
                .ok()
                .map(|v| fold_platform::color::to_picker(v, aces).map(|v| v.clamp(0., 1.) as f32))
        });
        let swatch_width = if swatch.is_some() { 12. } else { 0. };
        if let Some(color) = swatch {
            draw.add_rect(
                [start[0] + label_width - 12., start[1] + 4.],
                [start[0] + label_width - 2., start[1] + height - 4.],
                color,
            )
            .filled(true)
            .build();
        }
        draw.with_clip_rect(
            start,
            [
                start[0] + label_width - 2. - swatch_width,
                start[1] + height,
            ],
            || {
                draw.add_text(
                    [
                        start[0] + height * 0.55,
                        start[1] + (height - ui.text_line_height()) * 0.5,
                    ],
                    colors.text,
                    self.label,
                );
            },
        );
        let spacing = ui.clone_style().item_spacing()[0];
        let minimum = self
            .components
            .iter()
            .map(|name| ui.calc_text_size(format!("{name} 0.000"))[0] + 12.)
            .fold(0., f32::max);
        let mut columns =
            (((width - label_width) / (minimum + spacing)).floor() as usize).clamp(1, base.len());
        if base.len() == 4 && columns == 3 {
            columns = 2;
        }
        let component_width =
            ((width - label_width - spacing * columns as f32) / columns as f32).max(18.);
        for (i, path) in paths.iter().enumerate() {
            if i > 0 && i % columns == 0 {
                ui.dummy([label_width, height]);
            }
            ui.same_line();
            let _component = ui.push_id(i as i32);
            ui.set_next_item_width(component_width);
            let mut value = values[i];
            let is_animated = channels.contains_key(path);
            let is_keyed = channels.get(path).is_some_and(|c| c.at(time).is_some());
            let locked = is_animated && !is_keyed && !auto_key;
            // Alt-click is consumed before Drag can enter a value-edit gesture.
            let alt = ui.io().key_alt();
            let disabled = (locked || alt).then(|| ui.begin_disabled());
            let integer = self.unit == "integer";
            let format = if integer {
                "%.0f".into()
            } else if self.components.len() > 1 {
                format!("{} %.3f", self.components[i])
            } else {
                "%.3f".into()
            };
            let mut drag = Drag::new("##value")
                .speed(if integer { 1. } else { 0.1 })
                .try_display_format(format)
                .expect("component numeric format");
            if let Some([min, max]) = self.range {
                drag = drag.range(min, max).flags(DragFlags::ALWAYS_CLAMP);
            }
            let changed = drag.build(ui, &mut value) && value.is_finite();
            if integer {
                value = value.round();
            }
            let min = ui.item_rect_min();
            let max = ui.item_rect_max();
            response.item(ui, changed);
            drop(disabled);
            let mouse = ui.io().mouse_pos();
            let hovered = mouse[0] >= min[0]
                && mouse[0] < max[0]
                && mouse[1] >= min[1]
                && mouse[1] < max[1]
                && ui.is_window_hovered();
            let mut action = group;
            if hovered && alt && ui.is_mouse_clicked(MouseButton::Left) {
                action = Some(Action::Insert);
            }
            // Explicit popup also works on disabled, between-key values.
            if hovered && ui.is_mouse_clicked(MouseButton::Right) {
                ui.open_popup("channel");
            }
            if let Some(_popup) = ui.begin_popup("channel") {
                interpolation_menu(ui, std::slice::from_ref(path), channels, response);
                menu(ui);
                if ui.menu_item(if is_keyed {
                    "Update keyframe"
                } else {
                    "Add keyframe"
                }) {
                    action = Some(Action::Insert);
                }
                if ui.menu_item_enabled_selected_no_shortcut("Remove keyframe", false, is_keyed) {
                    action = Some(Action::Remove);
                }
                if ui.menu_item_enabled_selected_no_shortcut("Remove animation", false, is_animated)
                {
                    action = Some(Action::Clear);
                }
            }
            if hovered {
                ui.tooltip_text(format!(
                    "{} {}{}{}\nAlt-click: add keyframe · Right-click: animation",
                    self.label,
                    self.components[i],
                    if self.unit.is_empty() {
                        String::new()
                    } else {
                        format!(" ({})", self.unit)
                    },
                    if locked {
                        "\nAdd a key or enable Auto Key to edit between keys."
                    } else {
                        ""
                    }
                ));
            }
            if is_animated {
                draw.add_line(
                    [min[0] + 2., max[1] - 2.],
                    [max[0] - 2., max[1] - 2.],
                    colors.playhead,
                )
                .thickness(if is_keyed { 2. } else { 1. })
                .build();
            }
            if changed {
                if is_keyed || auto_key {
                    channels
                        .entry(path.clone())
                        .or_default()
                        .insert(time, value);
                } else {
                    base[i] = value;
                }
            }
            if let Some(action) = action {
                match action {
                    Action::Insert => {
                        channels
                            .entry(path.clone())
                            .or_default()
                            .insert(time, values[i]);
                    }
                    Action::Remove => {
                        if let Some(curve) = channels.get_mut(path) {
                            curve.keys.retain(|k| k.time != time);
                        }
                        if channels.get(path).is_some_and(|c| c.keys.is_empty()) {
                            channels.remove(path);
                            base[i] = values[i];
                        }
                    }
                    Action::Clear => {
                        channels.remove(path);
                        base[i] = values[i];
                    }
                }
                response.changed = true;
                response.finished = true;
            }
            if integer
                && (changed || matches!(action, Some(Action::Insert)))
                && let Some(key) = channels
                    .get_mut(path)
                    .and_then(|c| c.keys.iter_mut().find(|k| k.time == time))
            {
                key.value = key.value.round();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sdk::imgui::{Condition, Context};
    #[test]
    fn integer_property_keys_interpolate_between_counts() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([500., 250.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut channels: BTreeMap<String, Curve> = BTreeMap::new();
        let mut start = [0.; 2];
        for (seconds, count) in [(0, 0.), (1, 120.)] {
            // Author the displayed value, then key the numeric component as in the inspector.
            let mut base = [count];
            for frame in 0..6 {
                context
                    .io_mut()
                    .add_mouse_pos_event([start[0] + 170., start[1] + 8.]);
                context.io_mut().add_key_event(Key::ModAlt, true);
                context
                    .io_mut()
                    .add_mouse_button_event(MouseButton::Left, frame == 4);
                let ui = context.frame();
                ui.window("Inspector")
                    .position([0.; 2], Condition::Always)
                    .size([400., 200.], Condition::Always)
                    .build(|| {
                        start = ui.cursor_screen_pos();
                        // An existing curve samples its last value; seed the desired new key
                        // before exercising the same component update action.
                        if seconds == 1 && frame == 0 {
                            channels
                                .get_mut("count.0")
                                .unwrap()
                                .insert(Time::new(1, 1).unwrap(), count);
                        }
                        AnimatedProperty {
                            id: "count",
                            label: "Copies",
                            components: &[""],
                            unit: "integer",
                            range: None,
                            color: None,
                        }
                        .draw(
                            ui,
                            &mut base,
                            &mut channels,
                            Time::new(seconds, 1).unwrap(),
                            false,
                            &mut EditResponse::default(),
                        );
                    });
                drop(context.render_legacy());
            }
        }
        assert_eq!(
            channels["count.0"].sample(Time::new(1, 2).unwrap()),
            Some(60.)
        );
    }
    #[test]
    fn alt_click_keys_component_or_group_without_changing_authored_values() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([500., 250.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut base = [10., 20.];
        let mut channels = BTreeMap::new();
        let time = Time::new(1001, 24000).unwrap();
        let mut start = [0.; 2];
        let mut draw = |context: &mut Context| {
            let mut response = EditResponse::default();
            let ui = context.frame();
            ui.window("Inspector")
                .position([0.; 2], Condition::Always)
                .size([400., 200.], Condition::Always)
                .build(|| {
                    start = ui.cursor_screen_pos();
                    AnimatedProperty {
                        id: "position",
                        label: "Position",
                        components: &["X", "Y"],
                        unit: "px",
                        range: None,
                        color: None,
                    }
                    .draw(
                        ui,
                        &mut base,
                        &mut channels,
                        time,
                        false,
                        &mut response,
                    );
                });
            drop(context.render_legacy());
            (start, channels.clone(), response)
        };
        for _ in 0..3 {
            draw(&mut context);
        }
        let (start, _, _) = draw(&mut context);
        context
            .io_mut()
            .add_mouse_pos_event([start[0] + 170., start[1] + 8.]);
        context.io_mut().add_key_event(Key::ModAlt, true);
        draw(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        let (_, keys, response) = draw(&mut context);
        assert!(response.changed && response.finished);
        assert_eq!(keys.len(), 1);
        assert_eq!(keys["position.0"].at(time).unwrap().value, 10.);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        draw(&mut context);
        context
            .io_mut()
            .add_mouse_pos_event([start[0] + 25., start[1] + 8.]);
        draw(&mut context);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        let (_, keys, response) = draw(&mut context);
        assert!(response.changed);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys["position.1"].at(time).unwrap().value, 20.);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        context.io_mut().add_key_event(Key::ModAlt, false);
        draw(&mut context);
        drop(draw);
        assert_eq!(base, [10., 20.]);
    }
}

#[cfg(test)]
mod layout_tests {
    use super::*;
    use crate::sdk::imgui::{Condition, Context};
    #[test]
    fn four_components_wrap_without_horizontal_overflow_or_value_changes() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([700., 600.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut values = [0.1, 0.2, 0.3, 1.];
        let mut curves = BTreeMap::new();
        for width in [500., 266., 180.] {
            for frame in 0..4 {
                let ui = context.frame();
                ui.window("Inspector")
                    .position([0.; 2], Condition::Always)
                    .size([width, 500.], Condition::Always)
                    .build(|| {
                        let mut response = EditResponse::default();
                        AnimatedProperty {
                            id: "color",
                            label: "Color",
                            components: &["R", "G", "B", "A"],
                            unit: "",
                            range: None,
                            color: Some(true),
                        }
                        .draw(
                            ui,
                            &mut values,
                            &mut curves,
                            Time::ZERO,
                            false,
                            &mut response,
                        );
                        assert!(!response.changed);
                        if frame >= 2 {
                            assert_eq!(ui.scroll_max_x(), 0., "overflow at width {width}");
                        }
                    });
                drop(context.render_legacy());
            }
        }
        assert_eq!(values, [0.1, 0.2, 0.3, 1.]);
        assert!(curves.is_empty());
    }
}
