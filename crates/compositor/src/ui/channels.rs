//! Shared channel selectors keep technical routing out of the graph canvas.
use crate::{ChannelMapping, ChannelSource, Effect};
use fold_render::channels::ChannelName;
use fold_ui::sdk::{EditResponse, imgui::Ui};

fn changed(response: &mut EditResponse) {
    response.changed = true;
    response.finished = true;
}

pub(super) fn effect(
    ui: &Ui,
    effect: &mut Effect,
    available: &[ChannelName],
    mask_channels: &[ChannelName],
    response: &mut EditResponse,
) {
    if let Some(_menu) = ui.begin_combo(
        "Channels",
        if fold_render::channels::RGBA
            .iter()
            .all(|name| effect.channels.iter().any(|c| c.as_str() == *name))
            && effect.channels.len() == 4
        {
            "RGBA"
        } else {
            "Selected channels"
        },
    ) {
        for name in available {
            let mut enabled = effect.channels.iter().any(|c| c == name);
            if ui.checkbox(name.as_str(), &mut enabled) {
                effect.channels.retain(|c| c != name);
                if enabled {
                    effect.channels.push(name.clone());
                }
                changed(response);
            }
        }
    }
    if effect.mask.is_some() {
        if ui.checkbox("Disable mask", &mut effect.mask_disabled) {
            changed(response);
        }
        let _disabled = ui.begin_disabled_with_cond(effect.mask_disabled);
        if let Some(_menu) = ui.begin_combo("Mask channel", effect.mask_channel.as_str()) {
            for name in mask_channels {
                if ui.selectable(name.as_str()) {
                    effect.mask_channel = name.clone();
                    changed(response);
                }
            }
        }
        if !mask_channels.contains(&effect.mask_channel) {
            ui.text_wrapped("The selected mask channel is missing.");
        }
        if ui.checkbox("Invert mask", &mut effect.invert) {
            changed(response);
        }
    }
}

#[derive(Default)]
pub(super) struct ShuffleDraft {
    new_layer: String,
    channel: String,
    second: bool,
    pending_input: Option<(bool, String)>,
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
    let mut seen = std::collections::BTreeSet::new();
    let destinations: Vec<String> = mappings
        .iter()
        .map(|m| layer(&m.destination).to_owned())
        .filter(|name| seen.insert(name.clone()))
        .collect();
    let second_visible = destinations.len() > 1 || draft.second;
    for row in 0..if second_visible { 2 } else { 1 } {
        let _id = ui.push_id(&row.to_string());
        let old_output = destinations
            .get(row)
            .map(String::as_str)
            .unwrap_or(if row == 0 { "rgba" } else { "" });
        let first = mappings
            .iter()
            .find(|m| layer(&m.destination) == old_output);
        let (mut second, mut input) = match first.map(|m| &m.source) {
            Some(ChannelSource::A(name)) => (false, layer(name).to_owned()),
            Some(ChannelSource::B(name)) => (true, layer(name).to_owned()),
            _ => (
                false,
                layers(a)
                    .into_iter()
                    .find(|l| *l == "rgba")
                    .or_else(|| layers(a).into_iter().next())
                    .unwrap_or("")
                    .to_owned(),
            ),
        };
        if row == 1
            && old_output.is_empty()
            && let Some(pending) = &draft.pending_input
        {
            (second, input) = pending.clone();
        }
        let mut output = old_output.to_owned();
        let spacing = ui.clone_style().item_spacing()[0];
        let width = ((ui.content_region_avail()[0] - spacing) * 0.5).max(40.);
        let left = ui.cursor_pos()[0];
        ui.text(if row == 0 { "Input" } else { "Input 2" });
        ui.same_line_with_pos(left + width + spacing);
        ui.text(if row == 0 { "Output" } else { "Output 2" });
        let mut edited = false;
        let mut create_layer = false;
        {
            let _width = ui.push_item_width(width);
            let custom = mappings
                .iter()
                .filter(|m| layer(&m.destination) == old_output)
                .any(|m| match &m.source {
                    ChannelSource::A(name) => second || layer(name) != input,
                    ChannelSource::B(name) => !second || layer(name) != input,
                    _ => true,
                });
            let label = if custom {
                "Custom".into()
            } else if second {
                format!("B: {input}")
            } else {
                input.clone()
            };
            if let Some(_combo) = ui.begin_combo(
                "##input",
                if label.is_empty() {
                    "Select layer"
                } else {
                    &label
                },
            ) {
                for (is_b, names) in [(false, a), (true, b)] {
                    for name in layers(names) {
                        let label = if is_b {
                            format!("B: {name}")
                        } else {
                            name.to_owned()
                        };
                        if ui
                            .selectable_config(&label)
                            .selected(is_b == second && name == input)
                            .build()
                        {
                            second = is_b;
                            input = name.to_owned();
                            edited = true;
                            if row == 1 && old_output.is_empty() {
                                draft.pending_input = Some((second, input.clone()));
                            }
                        }
                    }
                }
            }
            ui.same_line();
            if let Some(_combo) = ui.begin_combo(
                "##output",
                if output.is_empty() {
                    "Select layer"
                } else {
                    &output
                },
            ) {
                let outputs: std::collections::BTreeSet<_> = layers(a)
                    .into_iter()
                    .chain(destinations.iter().map(String::as_str))
                    .chain(["rgba"])
                    .collect();
                for name in outputs {
                    if destinations
                        .iter()
                        .enumerate()
                        .any(|(i, n)| i != row && n == name)
                    {
                        continue;
                    }
                    if ui.selectable_config(name).selected(name == output).build() {
                        output = name.to_owned();
                        edited = true;
                    }
                }
                if ui.selectable("New layer…") {
                    draft.new_layer.clear();
                    create_layer = true;
                }
            }
        }
        if create_layer {
            ui.open_popup("New layer");
        }
        if let Some(_popup) = ui.begin_popup("New layer") {
            ui.input_text("Name", &mut draft.new_layer).build();
            let valid = ChannelName::try_from(format!("{}.red", draft.new_layer)).is_ok()
                && !draft.new_layer.is_empty()
                && !destinations.contains(&draft.new_layer);
            let _disabled = ui.begin_disabled_with_cond(!valid || input.is_empty());
            if ui.button("Create") {
                output = draft.new_layer.clone();
                edited = true;
                ui.close_current_popup();
            }
        }
        if edited && !output.is_empty() {
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
        if row == 1 && ui.small_button("Remove second route") {
            mappings.retain(|m| layer(&m.destination) != old_output);
            draft.second = false;
            changed(response);
        }
        if destinations.len() > 2 {
            ui.text_wrapped("Additional routes are available in Channels.");
        }
    }
    if !second_visible {
        let _disabled = ui.begin_disabled_with_cond(a.is_empty());
        if ui.small_button("Add second route") {
            if mappings.is_empty() {
                let names = layers(a);
                let input = names
                    .get("rgba")
                    .copied()
                    .or_else(|| names.first().copied())
                    .unwrap_or("");
                if route(mappings, a, false, input, "", "rgba").is_ok() {
                    changed(response);
                }
            }
            draft.second = true;
        }
    }
    if let Some(_channels) = ui.tree_node("Channels") {
        channel_routes(ui, mappings, a, b, draft, response);
    }
}
fn channel_routes(
    ui: &Ui,
    mappings: &mut Vec<ChannelMapping>,
    a: &[ChannelName],
    b: &[ChannelName],
    draft: &mut ShuffleDraft,
    response: &mut EditResponse,
) {
    let destination = &mut draft.channel;
    let mut remove = None;
    for (index, mapping) in mappings.iter_mut().enumerate() {
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

pub(super) fn remove(
    ui: &Ui,
    channels: &mut Vec<ChannelName>,
    keep: &mut bool,
    available: &[ChannelName],
    response: &mut EditResponse,
) {
    if ui.checkbox("Keep selected channels", keep) {
        changed(response);
    }
    for name in available {
        let mut enabled = channels.contains(name);
        if ui.checkbox(name.as_str(), &mut enabled) {
            channels.retain(|c| c != name);
            if enabled {
                channels.push(name.clone());
            }
            changed(response);
        }
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
