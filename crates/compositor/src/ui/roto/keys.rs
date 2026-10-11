//! Shape keys are synchronized scalar tracks, not a second animation model.
use crate::{Node, OperationConfig, animation::Property, roto::Roto};
use fold_foundation::{ObjectId, Time};
use std::collections::BTreeSet;

pub(super) fn geometry(property: &str) -> Option<ObjectId> {
    let mut parts = property.split('/');
    if parts.next()? != "roto" {
        return None;
    }
    let id = parts.next()?;
    if !matches!(
        parts.next()?.split('.').next()?,
        "point" | "feather" | "falloff"
    ) {
        return None;
    }
    serde_json::from_value(serde_json::Value::String(id.into())).ok()
}
fn properties(settings: &Roto, shape: ObjectId) -> impl Iterator<Item = Property> {
    settings
        .properties()
        .into_iter()
        .filter(move |p| geometry(&p.id) == Some(shape))
}
pub(super) fn times(node: &Node, shape: ObjectId) -> Result<BTreeSet<Time>, String> {
    let mut result = BTreeSet::new();
    for path in node
        .animation
        .bindings()
        .keys()
        .filter(|p| geometry(p) == Some(shape))
    {
        if let Some(curve) = node.animation.curve(path)? {
            result.extend(curve.keys.iter().map(|k| k.time));
        }
    }
    Ok(result)
}
/// Caller owns an atomic draft; do not publish until every channel succeeds.
pub(super) fn insert(
    node: &mut Node,
    settings: &Roto,
    shape: ObjectId,
    time: Time,
) -> Result<(), String> {
    if !settings.root.editable(shape) {
        return Err("Roto shape is locked".into());
    }
    for p in properties(settings, shape) {
        for (i, value) in p.values.into_iter().enumerate() {
            let path = crate::animation::path(&p.id, i);
            if node.animation.driven(&path) {
                return Err("Roto control is driven; edit its source".into());
            }
            let mut curve = node.animation.curve(&path)?.unwrap_or_default();
            curve.insert(time, value);
            node.animation.insert(path, curve)?;
        }
    }
    Ok(())
}
pub(super) fn set(
    node: &mut Node,
    sampled: &Roto,
    shape: ObjectId,
    time: Time,
    remove: bool,
) -> Result<(), String> {
    if sampled.root.find(shape).is_none_or(|l| l.shape.is_none()) {
        return Err("Select a shape".into());
    }
    if !sampled.root.editable(shape) {
        return Err("Roto shape is locked".into());
    }
    let mut next = node.clone();
    if remove {
        let OperationConfig::Roto { mut settings } = node.prepare()?.config().clone() else {
            return Err("Not a Roto node".into());
        };
        let mut bases: std::collections::BTreeMap<_, _> = properties(&settings, shape)
            .map(|p| (p.id, p.values))
            .collect();
        for p in properties(sampled, shape) {
            let mut base = bases.remove(&p.id).ok_or("Missing Roto control")?;
            for (i, value) in p.values.into_iter().enumerate() {
                let path = crate::animation::path(&p.id, i);
                if let Some(mut curve) = next.animation.curve(&path)? {
                    curve.keys.retain(|k| k.time != time);
                    if curve.keys.is_empty() {
                        next.animation.remove(&path)?;
                        base[i] = value;
                    } else {
                        next.animation.insert(path, curve)?;
                    }
                }
            }
            settings.set_property(&p.id, &base)?;
        }
        next.set_configuration(OperationConfig::Roto { settings })?;
    } else {
        insert(&mut next, sampled, shape, time)?;
    }
    *node = next;
    Ok(())
}

/// Compact, selection-scoped controls. A filled diamond means a key exists here.
pub(super) fn row(
    ui: &fold_ui::sdk::imgui::Ui,
    editor: &mut super::Editor,
    state: &mut crate::ui::state::State,
    host: &mut dyn fold_platform::desktop::DesktopClient,
    node_id: ObjectId,
    sampled: &Roto,
    time: Time,
) -> bool {
    use fold_platform::desktop::{DesktopCommand, TransportAction};
    use fold_ui::sdk::toolbar::{ToolbarIcon, icon_button, tooltip};
    let shape = editor
        .layer
        .filter(|id| sampled.root.find(*id).is_some_and(|l| l.shape.is_some()));
    let Some(graph) = state.graph.as_ref() else {
        return false;
    };
    let Ok(node) = graph.node(node_id) else {
        return false;
    };
    let times = match shape.map(|shape| times(node, shape)).transpose() {
        Ok(times) => times.unwrap_or_default(),
        Err(error) => {
            state.error = error;
            return false;
        }
    };
    let previous = times.iter().rev().find(|t| **t < time).copied();
    let next = times.iter().find(|t| **t > time).copied();
    let keyed = times.contains(&time);
    let editable = shape.is_some_and(|shape| sampled.root.editable(shape));
    let busy = state.editing || editor.active();
    let mut seek = None;
    let toggle;
    {
        let _disabled = ui.begin_disabled_with_cond(busy);
        {
            let _disabled = ui.begin_disabled_with_cond(previous.is_none());
            if icon_button(
                ui,
                "roto-previous-key",
                ToolbarIcon::PreviousKey,
                "Previous shape key",
            ) {
                seek = previous;
            }
        }
        ui.same_line();
        {
            let _disabled = ui.begin_disabled_with_cond(!editable);
            toggle = icon_button(
                ui,
                "roto-shape-key",
                ToolbarIcon::Keyframe(keyed),
                if keyed {
                    "Remove shape key at this frame"
                } else {
                    "Add shape key at this frame"
                },
            );
        }
        ui.same_line();
        {
            let _disabled = ui.begin_disabled_with_cond(next.is_none());
            if icon_button(ui, "roto-next-key", ToolbarIcon::NextKey, "Next shape key") {
                seek = next;
            }
        }
        ui.same_line();
        ui.checkbox("Auto key##roto", &mut editor.auto_key);
        tooltip(
            ui,
            "Automatically key the complete shape when editing its points or feather. Layer transforms use their own keys.",
        );
    }
    if let Some(time) = seek {
        let frame = time
            .to_ticks(
                graph.info.rate[0],
                graph.info.rate[1],
                fold_foundation::Rounding::Nearest,
            )
            .unwrap_or(0);
        host.command(DesktopCommand::Transport(TransportAction::Jump(
            (frame.max(0) as u32).min(graph.info.frames.saturating_sub(1)),
        )));
        return true;
    }
    if toggle {
        let result = state
            .graph
            .as_mut()
            .unwrap()
            .node_mut(node_id)
            .and_then(|node| set(node, sampled, shape.unwrap(), time, keyed));
        match result {
            Ok(()) => state.commit(host),
            Err(error) => state.error = error,
        }
        return true;
    }
    false
}
