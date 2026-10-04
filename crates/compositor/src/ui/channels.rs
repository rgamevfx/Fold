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
    source_layer: String,
    second: bool,
    destination_layer: String,
    channel: String,
}

fn layer_mapping(
    ui: &Ui,
    mappings: &mut Vec<ChannelMapping>,
    a: &[ChannelName],
    b: &[ChannelName],
    draft: &mut ShuffleDraft,
    response: &mut EditResponse,
) {
    let layers = |names: &[ChannelName]| {
        names
            .iter()
            .map(|name| name.as_str().rsplit_once('.').unwrap().0.to_owned())
            .collect::<std::collections::BTreeSet<_>>()
    };
    if draft.source_layer.is_empty() {
        draft.source_layer = layers(a).into_iter().next().unwrap_or_default();
    }
    let label = format!(
        "{}: {}",
        if draft.second { "B" } else { "A" },
        draft.source_layer
    );
    if let Some(_combo) = ui.begin_combo("Source layer", &label) {
        for (second, names) in [(false, a), (true, b)] {
            for layer in layers(names) {
                if ui.selectable(format!("{}: {layer}", if second { "B" } else { "A" })) {
                    draft.source_layer = layer;
                    draft.second = second;
                }
            }
        }
    }
    ui.input_text("Destination layer", &mut draft.destination_layer)
        .hint("rgba")
        .build();
    let destination = if draft.destination_layer.is_empty() {
        "rgba"
    } else {
        &draft.destination_layer
    };
    let names = if draft.second { b } else { a };
    let additions: Result<Vec<_>, _> = names
        .iter()
        .filter_map(|name| {
            let (layer, component) = name.as_str().rsplit_once('.').unwrap();
            (layer == draft.source_layer).then(|| {
                ChannelName::try_from(format!("{destination}.{component}")).map(|destination| {
                    ChannelMapping {
                        destination,
                        source: if draft.second {
                            ChannelSource::B(name.clone())
                        } else {
                            ChannelSource::A(name.clone())
                        },
                    }
                })
            })
        })
        .collect();
    let merged = additions
        .ok()
        .filter(|items| !items.is_empty())
        .map(|items| {
            let mut result = mappings.clone();
            result.retain(|old| !items.iter().any(|new| new.destination == old.destination));
            result.extend(items);
            result
        })
        .filter(|items| items.len() <= 256);
    let _disabled = ui.begin_disabled_with_cond(merged.is_none());
    if ui.button("Map layer")
        && let Some(merged) = merged
    {
        *mappings = merged;
        changed(response);
    }
}

pub(super) fn shuffle(
    ui: &Ui,
    mappings: &mut Vec<ChannelMapping>,
    a: &[ChannelName],
    b: &[ChannelName],
    draft: &mut ShuffleDraft,
    response: &mut EditResponse,
) {
    layer_mapping(ui, mappings, a, b, draft, response);
    ui.separator();
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
