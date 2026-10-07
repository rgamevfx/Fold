//! Shared stack controls. Hosts own persistence, locking and preview transactions.
use super::{EditResponse, animated_property::AnimatedProperty, imgui::Ui};
use fold_foundation::{ObjectId, Time};
use fold_platform::{
    desktop::DesktopClient,
    effects::{Instance, Interface},
};
use fold_project::{DocumentRef, ItemId};
use std::collections::BTreeMap;

pub fn name(host: &dyn DesktopClient, reference: &DocumentRef) -> String {
    host.snapshot()
        .map(|s| {
            fold_platform::browser::item_name(&s, ItemId::Document(reference.document), "Effect")
        })
        .unwrap_or_else(|| "Effect".into())
}
pub fn new_instance(definition: DocumentRef, interface: &Interface) -> Instance {
    Instance {
        id: ObjectId::new(),
        definition,
        version: interface.version,
        values: Default::default(),
        animation: Default::default(),
        enabled: true,
        extensions: Default::default(),
    }
}
/// Returns navigation intent; callers finish/cancel an edit before navigating.
pub fn stack(
    ui: &Ui,
    host: &dyn DesktopClient,
    effects: &mut Vec<Instance>,
    time: Time,
    response: &mut EditResponse,
) -> Option<DocumentRef> {
    if !ui.collapsing_header("Effects", super::imgui::TreeNodeFlags::DEFAULT_OPEN) {
        return None;
    }
    let mut navigation = None;
    let mut remove = None;
    let mut swap = None;
    let count = effects.len();
    for (i, effect) in effects.iter_mut().enumerate() {
        let _id = ui.push_id(&format!("{:?}", effect.id));
        if ui.checkbox("##enabled", &mut effect.enabled) {
            response.changed = true;
            response.finished = true;
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Enable effect");
        }
        ui.same_line();
        if let Some(_tree) = ui.tree_node(name(host, &effect.definition)) {
            parameters(ui, host, effect, time, response);
            if ui.small_button("Edit Graph") {
                navigation = Some(effect.definition.clone());
            }
            if i > 0 && ui.small_button("Move up") {
                swap = Some((i, i - 1));
            }
            if i + 1 < count {
                ui.same_line();
                if ui.small_button("Move down") {
                    swap = Some((i, i + 1));
                }
            }
            if ui.small_button("Remove effect") {
                remove = Some(i);
            }
        }
    }
    if let Some(i) = remove {
        effects.remove(i);
        response.changed = true;
        response.finished = true;
    } else if let Some((a, b)) = swap {
        effects.swap(a, b);
        response.changed = true;
        response.finished = true;
    }
    if let Some((reference, interface)) = picker(ui, host, "Add effect", &["source"]) {
        effects.push(new_instance(reference, &interface));
        response.changed = true;
        response.finished = true;
    }
    navigation
}
pub fn picker(
    ui: &Ui,
    host: &dyn DesktopClient,
    label: &str,
    images: &[&str],
) -> Option<(DocumentRef, Interface)> {
    if ui.small_button(label) {
        ui.open_popup(label);
    }
    let _popup = ui.begin_popup(label)?;
    let snapshot = host.snapshot()?;
    let mut found = false;
    for id in snapshot.state().documents.keys() {
        let Ok(interface) = host.effect_interface(*id) else {
            continue;
        };
        if interface.images.len() != images.len()
            || !images
                .iter()
                .all(|n| interface.images.iter().any(|v| v == n))
        {
            continue;
        }
        found = true;
        let _id = ui.push_id(&format!("{id:?}"));
        let reference = DocumentRef {
            document: *id,
            output: "video".into(),
            extensions: Default::default(),
        };
        if ui.selectable(name(host, &reference)) {
            return Some((reference, interface));
        }
    }
    if !found {
        ui.text_wrapped("No matching published definitions in this project.");
    }
    None
}
/// Shared version diagnostics and compatible migration for every instance host.
pub fn parameters(
    ui: &Ui,
    host: &dyn DesktopClient,
    effect: &mut Instance,
    time: Time,
    response: &mut EditResponse,
) {
    match host.effect_interface(effect.definition.document) {
        Ok(interface) if interface.version == effect.version => controls(
            ui,
            effect,
            &interface,
            time,
            host.state().auto_key,
            response,
        ),
        Ok(interface) => {
            ui.text_wrapped("Definition interface changed.");
            let mut updated = effect.clone();
            match updated.adopt_interface(&interface) {
                Ok(()) => {
                    if ui.small_button("Update compatible interface") {
                        *effect = updated;
                        response.changed = true;
                        response.finished = true;
                    }
                }
                Err(error) => ui.text_wrapped(error),
            }
        }
        Err(e) => ui.text_wrapped(e),
    }
}
pub fn controls(
    ui: &Ui,
    effect: &mut Instance,
    interface: &Interface,
    time: Time,
    auto_key: bool,
    response: &mut EditResponse,
) {
    for control in &interface.controls {
        let mut values = effect
            .values
            .get(&control.id)
            .unwrap_or(&control.default)
            .clone();
        if values.len() != control.default.len() {
            ui.text_wrapped("Control type changed; migrate the instance.");
            continue;
        }
        let mut animation = BTreeMap::new();
        if let Some(channels) = effect.animation.get(&control.id) {
            for (i, c) in channels.iter().enumerate() {
                animation.insert(format!("{}.{}", control.id, i), c.clone());
            }
        }
        let mut edit = EditResponse::default();
        AnimatedProperty {
            id: &control.id,
            label: &control.label,
            components: &["X", "Y", "Z", "W"][..values.len()],
            unit: "",
            range: control.range,
            color: None,
        }
        .draw(ui, &mut values, &mut animation, time, auto_key, &mut edit);
        if edit.changed {
            effect.values.insert(control.id.clone(), values);
            let channels: Vec<_> = (0..control.default.len())
                .map(|i| {
                    animation
                        .remove(&format!("{}.{}", control.id, i))
                        .unwrap_or_default()
                })
                .collect();
            if channels.iter().any(|c| !c.keys.is_empty()) {
                let mut channels = channels;
                for (i, c) in channels.iter_mut().enumerate() {
                    if c.keys.is_empty() {
                        c.insert(time, effect.values[&control.id][i]);
                    }
                }
                effect.animation.insert(control.id.clone(), channels);
            } else {
                effect.animation.remove(&control.id);
            }
        }
        response.began |= edit.began;
        response.changed |= edit.changed;
        response.finished |= edit.finished;
        response.cancelled |= edit.cancelled;
    }
}
