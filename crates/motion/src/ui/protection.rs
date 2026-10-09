//! Explicit artist lock actions; unlocking never travels through authoring commands.
use super::state::State;
use crate::model::Construction;
use fold_foundation::ObjectId;
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_ui::sdk::{self, imgui::Ui};

pub(super) fn menu(ui: &Ui, host: &mut dyn DesktopClient, state: &mut State) -> bool {
    let Some(value) = state.construction.as_ref() else {
        return false;
    };
    let selected = &host.state().selection.objects;
    let definition = state.graph_definition.unwrap_or(value.root.definition);
    let mut entries = vec![];
    for asset in &value.definitions {
        for input in &asset.inputs {
            if selected.contains(&input.id) || (selected.is_empty() && asset.id == definition) {
                entries.push((input.id, input.name.clone(), input.protected));
            }
        }
        for node in &asset.nodes {
            if selected.contains(&node.id) {
                let operation =
                    crate::registry::resolve(&node.operation, node.version, &node.mode).ok();
                for (key, parameter) in &node.parameters {
                    let label = operation
                        .as_ref()
                        .and_then(|op| op.parameters.get(key))
                        .map_or(key.as_str(), |port| port.title.as_str());
                    entries.push((
                        parameter.id,
                        format!("{} · {label}", node.name),
                        parameter.protected,
                    ));
                }
            }
        }
        for instance in &asset.instances {
            if selected.contains(&instance.id) {
                for (input, parameter) in &instance.overrides {
                    let label = value
                        .definition(instance.definition)
                        .ok()
                        .and_then(|asset| asset.inputs.iter().find(|i| i.id == *input))
                        .map_or("Unresolved control", |i| i.name.as_str());
                    entries.push((
                        parameter.id,
                        format!("{} · {label}", instance.name),
                        parameter.protected,
                    ));
                }
            }
        }
    }
    if definition == value.root.definition {
        for (input, parameter) in &value.root.overrides {
            if selected.is_empty() || selected.contains(input) {
                let label = value
                    .definition(definition)
                    .ok()
                    .and_then(|asset| asset.inputs.iter().find(|i| i.id == *input))
                    .map_or("Unresolved control", |i| i.name.as_str());
                entries.push((
                    parameter.id,
                    format!("Main override · {label}"),
                    parameter.protected,
                ));
            }
        }
    }
    // Group locks may be inherited through nested membership. Make every declared
    // group available by name so the artist can clear the actual owning lock.
    for group in &value.presentation.groups {
        entries.push((group.id, format!("Group · {}", group.name), group.locked));
    }
    if entries.is_empty() {
        return false;
    }
    let mut action = None;
    if let Some(_menu) = sdk::toolbar::icon_menu(ui, "protection", "Lock or unlock controls") {
        for (id, label, locked) in entries {
            let _id = ui.push_id(&id.to_string());
            let verb = if locked { "Unlock" } else { "Lock" };
            if ui.menu_item(format!("{verb} {label}")) {
                action = Some((id, locked));
            }
        }
    }
    if let Some((id, locked)) = action {
        if locked {
            unlock(host, state, id);
        } else {
            let mut after = value.clone();
            if lock(&mut after, id) {
                state.commit(host, after);
            }
        }
        return true;
    }
    false
}

pub(super) fn unlock(host: &mut dyn DesktopClient, state: &mut State, id: ObjectId) {
    let Some(document) = state.document else {
        return;
    };
    let expected = host
        .snapshot()
        .filter(|s| s.revision() == state.base)
        .and_then(|snapshot| {
            crate::editing::unlock_document_for_artist(
                snapshot.state().documents.get(&document)?,
                &[id],
            )
            .ok()
        })
        .and_then(|document| Construction::from_document(&document).ok());
    host.command(DesktopCommand::UnlockControls {
        document,
        targets: vec![id],
        base: state.base,
    });
    let actual = host.snapshot().and_then(|snapshot| {
        snapshot
            .state()
            .documents
            .get(&document)
            .and_then(|document| Construction::from_document(document).ok())
    });
    let changed = expected.is_some() && expected == actual;
    state.sync(host);
    if changed {
        state.error.clear();
    } else {
        state.error = format!("Control was not unlocked. {}", host.state().status);
    }
}

fn lock(value: &mut Construction, id: ObjectId) -> bool {
    if let Some(parameter) = value.root.overrides.values_mut().find(|p| p.id == id) {
        parameter.protected = true;
        return true;
    }
    for asset in &mut value.definitions {
        for input in &mut asset.inputs {
            if input.id == id {
                input.protected = true;
                return true;
            }
        }
        for parameter in asset
            .nodes
            .iter_mut()
            .flat_map(|n| n.parameters.values_mut())
            .chain(
                asset
                    .instances
                    .iter_mut()
                    .flat_map(|i| i.overrides.values_mut()),
            )
        {
            if parameter.id == id {
                parameter.protected = true;
                return true;
            }
        }
    }
    if let Some(group) = value.presentation.groups.iter_mut().find(|g| g.id == id) {
        group.locked = true;
        return true;
    }
    false
}
