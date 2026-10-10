//! Shared channel selectors keep technical routing out of the graph canvas.
use crate::{ChannelMapping, ChannelSource, Effect};
use fold_render::channels::ChannelName;
use fold_ui::sdk::{EditResponse, imgui::Ui};

fn changed(response: &mut EditResponse) {
    response.changed = true;
    response.finished = true;
}

pub(super) fn selection(
    ui: &Ui,
    effect: &mut Effect,
    available: &[ChannelName],
    response: &mut EditResponse,
) {
    selector(ui, "Channels", &mut effect.channels, available, response);
}

pub(super) fn selector(
    ui: &Ui,
    label: &str,
    selected: &mut Vec<ChannelName>,
    available: &[ChannelName],
    response: &mut EditResponse,
) {
    let _id = ui.push_id(label);
    super::controls::row(ui, label);
    let rgba = fold_render::channels::RGBA;
    let names: Vec<_> = selected.iter().map(|n| n.as_str()).collect();
    let summary = if names.is_empty() {
        "none".to_owned()
    } else if names == rgba {
        "rgba".into()
    } else if names == rgba[..3] {
        "rgb".into()
    } else if names == rgba[3..] {
        "alpha".into()
    } else if names.len() == 1 {
        names[0].to_owned()
    } else if layer(&selected[0]) != "rgba"
        && selected.iter().all(|n| layer(n) == layer(&selected[0]))
        && available
            .iter()
            .filter(|n| layer(n) == layer(&selected[0]))
            .all(|n| selected.contains(n))
    {
        layer(&selected[0]).to_owned()
    } else {
        format!("{} channels", names.len())
    };
    if let Some(_menu) = ui.begin_combo("##channels", summary) {
        for (label, components) in [
            ("none", &[][..]),
            ("rgba", &rgba[..]),
            ("rgb", &rgba[..3]),
            ("alpha", &rgba[3..]),
        ] {
            if ui.selectable(label) {
                *selected = components
                    .iter()
                    .map(|n| n.to_string().try_into().unwrap())
                    .collect();
                changed(response);
            }
        }
        for name in layers(available).into_iter().filter(|name| *name != "rgba") {
            if ui.selectable(name) {
                *selected = available
                    .iter()
                    .filter(|n| layer(n) == name)
                    .cloned()
                    .collect();
                changed(response);
            }
        }
        ui.separator();
        let choices: std::collections::BTreeSet<_> = available
            .iter()
            .cloned()
            .chain(rgba.map(|n| n.to_owned().try_into().unwrap()))
            .chain(selected.iter().cloned())
            .collect();
        for name in choices {
            let mut enabled = selected.contains(&name);
            if ui.checkbox(name.as_str(), &mut enabled) {
                selected.retain(|c| c != &name);
                if enabled {
                    selected.push(name);
                }
                changed(response);
            }
        }
    }
}

pub(super) fn effect(
    ui: &Ui,
    effect: &mut Effect,
    mask_channels: &[ChannelName],
    response: &mut EditResponse,
) {
    let width = super::controls::row(ui, "Mask");
    let invert_width = ui.calc_text_size("Invert")[0] + ui.frame_height() + 12.;
    ui.set_next_item_width((width - invert_width).max(40.));
    let label = if effect.mask.is_none() || effect.mask_disabled {
        "none"
    } else {
        effect.mask_channel.as_str()
    };
    if let Some(_menu) = ui.begin_combo("##mask", label) {
        if ui.selectable("none") {
            effect.mask_disabled = true;
            changed(response);
        }
        for name in mask_channels {
            if ui.selectable(name.as_str()) {
                effect.mask_channel = name.clone();
                effect.mask_disabled = false;
                changed(response);
            }
        }
    }
    ui.same_line();
    let _disabled = ui.begin_disabled_with_cond(effect.mask.is_none() || effect.mask_disabled);
    super::controls::check(ui, "Invert##mask", &mut effect.invert, response);
    drop(_disabled);
    if effect.mask.is_some()
        && !effect.mask_disabled
        && !mask_channels.contains(&effect.mask_channel)
    {
        ui.text_wrapped("The selected mask channel is missing.");
    }
}

#[derive(Default)]
pub(super) struct ShuffleDraft {
    new_layer: String,
    channel: String,
    pending_input: [Option<(bool, String)>; 2],
    picked: Option<ChannelSource>,
    selection: Option<(fold_foundation::DocumentId, fold_foundation::ObjectId)>,
}
impl ShuffleDraft {
    pub fn select(
        &mut self,
        selection: Option<(fold_foundation::DocumentId, fold_foundation::ObjectId)>,
    ) {
        if self.selection != selection {
            *self = Self {
                selection,
                ..Default::default()
            };
        }
    }
}
fn layer(name: &ChannelName) -> &str {
    name.as_str().rsplit_once('.').unwrap().0
}
fn layers(names: &[ChannelName]) -> std::collections::BTreeSet<&str> {
    names.iter().map(layer).collect()
}

/// Replace a displayed route, removing its previous destinations. All sources
/// still refer to the original A/B images, preserving simultaneous Shuffle math.
fn route(
    mappings: &mut Vec<ChannelMapping>,
    names: &[ChannelName],
    second: bool,
    input: &str,
    old_output: &str,
    output: &str,
) -> Result<(), String> {
    let additions: Vec<_> = names
        .iter()
        .filter(|name| layer(name) == input)
        .map(|name| {
            let component = name.as_str().rsplit_once('.').unwrap().1;
            Ok(ChannelMapping {
                destination: ChannelName::try_from(format!("{output}.{component}"))?,
                source: if second {
                    ChannelSource::B(name.clone())
                } else {
                    ChannelSource::A(name.clone())
                },
            })
        })
        .collect::<Result<_, String>>()?;
    if additions.is_empty() {
        return Err("Select an input layer".into());
    }
    let position = mappings
        .iter()
        .position(|m| layer(&m.destination) == old_output)
        .unwrap_or(mappings.len());
    let mut next = mappings.clone();
    next.retain(|m| layer(&m.destination) != old_output && layer(&m.destination) != output);
    next.splice(
        position.min(next.len())..position.min(next.len()),
        additions,
    );
    if next.len() > 256 {
        return Err("Shuffle supports at most 256 channel mappings".into());
    }
    *mappings = next;
    Ok(())
}

pub(super) fn shuffle(
    ui: &Ui,
    mappings: &mut Vec<ChannelMapping>,
    a: &[ChannelName],
    b: &[ChannelName],
    draft: &mut ShuffleDraft,
    response: &mut EditResponse,
) {
    // Unconnected inputs still expose the standard channel layout for authoring.
    let rgba: Vec<ChannelName> = fold_render::channels::RGBA
        .iter()
        .map(|n| (*n).to_owned().try_into().unwrap())
        .collect();
    let a = if a.is_empty() { &rgba } else { a };
    let b = if b.is_empty() { &rgba } else { b };
    let mut seen = std::collections::BTreeSet::new();
    let destinations: Vec<String> = mappings
        .iter()
        .map(|m| layer(&m.destination).to_owned())
        .filter(|name| seen.insert(name.clone()))
        .collect();
    let spacing = ui.clone_style().item_spacing()[0];
    let width = ((ui.content_region_avail()[0] - spacing) * 0.5).max(40.);
    let left = ui.cursor_pos()[0];
    ui.text("Input");
    ui.same_line_with_pos(left + width + spacing);
    ui.text("Output");
    let mut displayed = Vec::new();
    for index in 0..2 {
        let _id = ui.push_id(index as i32);
        let old_output = destinations
            .get(index)
            .map(String::as_str)
            .unwrap_or(if index == 0 { "rgba" } else { "" });
        let first = mappings.iter().find(|m| {
            layer(&m.destination) == old_output
                && matches!(m.source, ChannelSource::A(_) | ChannelSource::B(_))
        });
        let (mut second, mut input) = match first.map(|m| &m.source) {
            Some(ChannelSource::A(name)) => (false, layer(name).to_owned()),
            Some(ChannelSource::B(name)) => (true, layer(name).to_owned()),
            _ => draft.pending_input[index]
                .clone()
                .unwrap_or((true, if index == 0 { "rgba" } else { "" }.into())),
        };
        let mut output = old_output.to_owned();
        let mut edited = false;
        ui.set_next_item_width(42.);
        if let Some(_combo) = ui.begin_combo("##branch", if second { "B" } else { "A" }) {
            for (value, label) in [(true, "B"), (false, "A")] {
                if ui
                    .selectable_config(label)
                    .selected(second == value)
                    .build()
                {
                    second = value;
                    edited = true;
                }
            }
        }
        ui.same_line();
        ui.set_next_item_width((width - 42. - spacing).max(25.));
        if let Some(_combo) =
            ui.begin_combo("##input", if input.is_empty() { "none" } else { &input })
        {
            if index == 1 && ui.selectable("none") {
                input.clear();
                output.clear();
                edited = true;
            }
            for name in layers(if second { b } else { a }) {
                if ui.selectable_config(name).selected(name == input).build() {
                    input = name.into();
                    edited = true;
                }
            }
        }
        ui.same_line();
        ui.set_next_item_width(width);
        let mut new_layer = false;
        if let Some(_combo) =
            ui.begin_combo("##output", if output.is_empty() { "none" } else { &output })
        {
            if index == 1 && ui.selectable("none") {
                output.clear();
                edited = true;
            }
            let outputs: std::collections::BTreeSet<_> = layers(b)
                .into_iter()
                .chain(layers(a))
                .chain(destinations.iter().map(String::as_str))
                .chain(["rgba"])
                .collect();
            for name in outputs {
                if destinations
                    .iter()
                    .enumerate()
                    .any(|(i, n)| i != index && n == name)
                    || (index == 1 && destinations.is_empty() && name == "rgba")
                {
                    continue;
                }
                if ui.selectable_config(name).selected(name == output).build() {
                    output = name.into();
                    edited = true;
                }
            }
            if ui.selectable("New layer…") {
                draft.new_layer.clear();
                new_layer = true;
            }
        }
        if new_layer {
            ui.open_popup("New layer");
        }
        if let Some(_popup) = ui.begin_popup("New layer") {
            ui.input_text("Name", &mut draft.new_layer).build();
            let valid = !draft.new_layer.is_empty()
                && ChannelName::try_from(format!("{}.red", draft.new_layer)).is_ok()
                && !destinations.contains(&draft.new_layer);
            let _disabled = ui.begin_disabled_with_cond(!valid || input.is_empty());
            if ui.button("Create") {
                output = draft.new_layer.clone();
                edited = true;
                ui.close_current_popup();
            }
        }
        if edited {
            draft.pending_input[index] = Some((second, input.clone()));
            if output.is_empty() {
                mappings.retain(|m| layer(&m.destination) != old_output);
                changed(response);
            } else if !input.is_empty() {
                // Materialize the primary route before adding a second layer.
                if index == 1 && mappings.is_empty() {
                    let _ = route(mappings, b, true, "rgba", "", "rgba");
                }
                match route(
                    mappings,
                    if second { b } else { a },
                    second,
                    &input,
                    old_output,
                    &output,
                ) {
                    Ok(()) => changed(response),
                    Err(error) => ui.text_wrapped(error),
                }
            }
        }
        if !output.is_empty() {
            displayed.push(output);
        }
    }
    ui.spacing();
    for output in &displayed {
        let _id = ui.push_id(output);
        if displayed.len() > 1 {
            ui.text_disabled(output);
        }
        routing(ui, mappings, a, b, output, draft, response);
    }
    if let Some(_channels) = ui.tree_node("Other channels") {
        channel_routes(ui, mappings, a, b, draft, response, &displayed);
    }
}
/// Click or drag a source to an output. Wires expose channel swaps at a glance.
fn routing(
    ui: &Ui,
    mappings: &mut Vec<ChannelMapping>,
    a: &[ChannelName],
    b: &[ChannelName],
    output: &str,
    draft: &mut ShuffleDraft,
    response: &mut EditResponse,
) {
    if output.is_empty() {
        return;
    }
    use fold_ui::sdk::imgui::{MouseButton, StyleColor};
    let colors = fold_ui::sdk::EditorColors::from_ui(ui);
    let components = ["red", "green", "blue", "alpha"];
    if ui.is_key_pressed(fold_ui::sdk::imgui::Key::Escape) {
        draft.picked = None;
    }
    let start = ui.cursor_screen_pos();
    let width = ui.content_region_avail()[0];
    let step = ui.frame_height_with_spacing();
    let field = (width * 0.24).min(90.);
    let constants = ui.frame_height() * 2. + 4.;
    let sources: Vec<_> = components
        .iter()
        .filter_map(|c| {
            let route = mappings.iter().find(|m| {
                layer(&m.destination) == output
                    && matches!(m.source, ChannelSource::A(_) | ChannelSource::B(_))
            });
            let (names, second, from) = match route.map(|r| &r.source) {
                Some(ChannelSource::B(n)) => (b, true, layer(n)),
                Some(ChannelSource::A(n)) => (a, false, layer(n)),
                _ => (b, true, "rgba"),
            };
            names
                .iter()
                .find(|n| n.as_str() == format!("{from}.{c}"))
                .map(|n| {
                    (
                        c.to_string(),
                        if second {
                            ChannelSource::B(n.clone())
                        } else {
                            ChannelSource::A(n.clone())
                        },
                    )
                })
        })
        .collect();
    let wire_start = start[0] + field + 5.;
    let wire_end = start[0] + width - field - constants - 9.;
    for (i, (label, source)) in sources.iter().enumerate() {
        let _id = ui.push_id(&format!("source-{i}"));
        let (x, y, item_width) = (start[0], start[1] + i as f32 * step, field);
        ui.set_cursor_screen_pos([x, y]);
        let _selected = (draft.picked.as_ref() == Some(source))
            .then(|| ui.push_style_color(StyleColor::Button, colors.selected));
        let accent =
            fold_ui::sdk::CHANNEL_COLORS[components.iter().position(|c| *c == label).unwrap_or(3)];
        let _text = ui.push_style_color(StyleColor::Text, accent);
        let clicked = ui.button_with_size(label, [item_width, ui.frame_height()]);
        if clicked || (ui.is_item_hovered() && ui.is_mouse_clicked(MouseButton::Left)) {
            draft.picked = Some(source.clone());
        }
    }
    for (i, component) in components.iter().enumerate() {
        let name: ChannelName = format!("{output}.{component}").try_into().unwrap();
        let source = mappings
            .iter()
            .find(|m| m.destination == name)
            .map(|m| m.source.clone())
            .unwrap_or_else(|| ChannelSource::B(name.clone()));
        let y = start[1] + i as f32 * step;
        if let Some(index) = sources.iter().position(|(_, s)| *s == source) {
            let from = [
                wire_start,
                start[1] + index as f32 * step + ui.frame_height() * 0.5,
            ];
            let to = [wire_end, y + ui.frame_height() * 0.5];
            ui.get_window_draw_list()
                .add_bezier_curve(
                    from,
                    [from[0] + (wire_end - wire_start) * 0.4, from[1]],
                    [to[0] - (wire_end - wire_start) * 0.4, to[1]],
                    to,
                    fold_ui::sdk::CHANNEL_COLORS[index.min(3)],
                )
                .thickness(1.5)
                .build();
        }
        ui.set_cursor_screen_pos([start[0] + width - field - constants - 4., y]);
        for (label, value) in [("0", ChannelSource::Zero), ("1", ChannelSource::One)] {
            let _id = ui.push_id(&format!("constant-{i}-{label}"));
            let _selected =
                (source == value).then(|| ui.push_style_color(StyleColor::Button, colors.selected));
            if ui.button_with_size(label, [ui.frame_height(), ui.frame_height()]) {
                if let Some(mapping) = mappings.iter_mut().find(|m| m.destination == name) {
                    mapping.source = value;
                } else {
                    mappings.push(ChannelMapping {
                        destination: name.clone(),
                        source: value,
                    });
                }
                changed(response);
            }
            ui.same_line_with_spacing(0., 4.);
        }
        ui.set_cursor_screen_pos([start[0] + width - field, y]);
        let _id = ui.push_id(&format!("dest-{i}"));
        let _text = ui.push_style_color(StyleColor::Text, fold_ui::sdk::CHANNEL_COLORS[i]);
        let clicked = ui.button_with_size(component, [field, ui.frame_height()]);
        if (clicked || (ui.is_item_hovered() && ui.is_mouse_released(MouseButton::Left)))
            && let Some(source) = draft.picked.take()
        {
            if let Some(mapping) = mappings.iter_mut().find(|m| m.destination == name) {
                mapping.source = source;
            } else {
                mappings.push(ChannelMapping {
                    destination: name.clone(),
                    source,
                });
            }
            changed(response);
        }
        if let Some(_menu) = ui.begin_popup_context_item() {
            for (branch, names) in [(false, a), (true, b)] {
                for channel in names {
                    if ui.selectable(format!(
                        "{}: {}",
                        if branch { "B" } else { "A" },
                        channel.as_str()
                    )) {
                        let source = if branch {
                            ChannelSource::B(channel.clone())
                        } else {
                            ChannelSource::A(channel.clone())
                        };
                        if let Some(mapping) = mappings
                            .iter_mut()
                            .find(|m| m.destination.as_str() == format!("{output}.{component}"))
                        {
                            mapping.source = source;
                        } else {
                            mappings.push(ChannelMapping {
                                destination: format!("{output}.{component}").try_into().unwrap(),
                                source,
                            });
                        }
                        changed(response);
                    }
                }
            }
        }
    }
    ui.set_cursor_screen_pos([start[0], start[1] + 4. * step]);
}

fn channel_routes(
    ui: &Ui,
    mappings: &mut Vec<ChannelMapping>,
    a: &[ChannelName],
    b: &[ChannelName],
    draft: &mut ShuffleDraft,
    response: &mut EditResponse,
    visible_layers: &[String],
) {
    let destination = &mut draft.channel;
    let mut remove = None;
    for (index, mapping) in mappings.iter_mut().enumerate().filter(|(_, mapping)| {
        let (layer, channel) = mapping.destination.as_str().rsplit_once('.').unwrap();
        !visible_layers.iter().any(|shown| shown == layer)
            || !["red", "green", "blue", "alpha"].contains(&channel)
    }) {
        let _id = ui.push_id(&index.to_string());
        let label = match &mapping.source {
            ChannelSource::A(name) => format!("A: {name}"),
            ChannelSource::B(name) => format!("B: {name}"),
            ChannelSource::Zero => "0".into(),
            ChannelSource::One => "1".into(),
        };
        if let Some(_combo) = ui.begin_combo(mapping.destination.as_str(), &label) {
            for (input, names) in [("A", a), ("B", b)] {
                for name in names {
                    if ui.selectable(format!("{input}: {name}")) {
                        mapping.source = if input == "A" {
                            ChannelSource::A(name.clone())
                        } else {
                            ChannelSource::B(name.clone())
                        };
                        changed(response);
                    }
                }
            }
            for (label, source) in [("0", ChannelSource::Zero), ("1", ChannelSource::One)] {
                if ui.selectable(label) {
                    mapping.source = source;
                    changed(response);
                }
            }
        }
        if let Some(_menu) = ui.begin_popup_context_item()
            && ui.menu_item("Remove mapping")
        {
            remove = Some(index);
        }
    }
    if let Some(index) = remove {
        mappings.remove(index);
        changed(response);
    }
    ui.input_text("Destination", destination)
        .hint("layer.channel")
        .build();
    let name = ChannelName::try_from(destination.clone()).ok();
    let valid = name
        .as_ref()
        .is_some_and(|name| !mappings.iter().any(|m| &m.destination == name));
    let _disabled = ui.begin_disabled_with_cond(!valid || mappings.len() >= 256);
    if ui.button("Add channel")
        && let Some(name) = name
    {
        mappings.push(ChannelMapping {
            destination: name,
            source: a
                .first()
                .cloned()
                .map(ChannelSource::A)
                .unwrap_or(ChannelSource::Zero),
        });
        destination.clear();
        changed(response);
    }
}

pub(super) fn cull(
    ui: &Ui,
    channels: &mut [Vec<ChannelName>; 4],
    keep: &mut bool,
    available: &[ChannelName],
    response: &mut EditResponse,
) {
    super::controls::combo(
        ui,
        "Operation",
        keep,
        &[(false, "Remove"), (true, "Keep")],
        response,
    );
    for (selected, label) in
        channels
            .iter_mut()
            .zip(["Channels", "Channels 2", "Channels 3", "Channels 4"])
    {
        selector(ui, label, selected, available, response);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn changing_layer_routes_preserves_second_route_order_and_unmapped_channels() {
        let names: Vec<ChannelName> = [
            "diffuse.red",
            "diffuse.green",
            "diffuse.blue",
            "diffuse.alpha",
            "specular.red",
            "specular.green",
            "specular.blue",
        ]
        .into_iter()
        .map(|n| n.to_owned().try_into().unwrap())
        .collect();
        let mut mappings = Vec::new();
        route(&mut mappings, &names, false, "diffuse", "", "rgba").unwrap();
        route(&mut mappings, &names, true, "specular", "", "reflection").unwrap();
        route(&mut mappings, &names, false, "specular", "rgba", "rgba").unwrap();
        assert_eq!(layer(&mappings[0].destination), "rgba");
        assert_eq!(mappings.len(), 6);
        assert!(
            mappings[3..]
                .iter()
                .all(|m| layer(&m.destination) == "reflection"
                    && matches!(m.source, ChannelSource::B(_)))
        );
        route(&mut mappings, &names, false, "diffuse", "rgba", "new_layer").unwrap();
        assert!(mappings.iter().all(|m| layer(&m.destination) != "rgba"));
        assert_eq!(layer(&mappings[0].destination), "new_layer");
        let before = mappings.clone();
        assert!(route(&mut mappings, &names, false, "missing", "new_layer", "rgba").is_err());
        assert_eq!(mappings, before);
    }
}
