//! Composition-level actions and nested-source controls.
use super::state::State;
use crate::nodes::composition::{KIND, Settings};
#[cfg(test)]
use fold_foundation::DocumentId;
use fold_foundation::{ObjectId, Time};
use fold_platform::desktop::{DesktopClient, DesktopCommand, ViewLocation};
use fold_ui::sdk::{imgui::Ui, time_view};

#[cfg(test)]
pub(super) fn nest(state: &mut State, host: &mut dyn DesktopClient, empty: bool) {
    use fold_platform::{desktop::Selection, packages::CommandRequest};
    state.cancel(host);
    let Some(document) = state.document else {
        return;
    };
    let Some(snapshot) = host.snapshot() else {
        return;
    };
    let objects: Vec<_> = state
        .motion
        .as_ref()
        .and_then(|m| m.scene.as_ref())
        .map(|s| {
            s.objects
                .iter()
                .filter(|o| !empty && state.selected.iter().any(|id| o.owns(*id)))
                .map(|o| o.id)
                .collect()
        })
        .unwrap_or_default();
    if !empty && objects.is_empty() {
        state.error = "Select a layer or group to precompose".into();
        return;
    }
    let child = DocumentId::new();
    let name = state
        .motion
        .as_ref()
        .and_then(|m| m.scene.as_ref())
        .and_then(|s| s.objects.iter().find(|o| objects.contains(&o.id)))
        .map(|o| format!("{} Composition", o.name))
        .unwrap_or_else(|| "Nested Composition".into());
    host.command(DesktopCommand::Extension(CommandRequest {
        id: crate::authoring::composition::NEST.into(),
        base: snapshot.revision(),
        arguments: serde_json::to_vec(&crate::authoring::composition::Args {
            document,
            child,
            name,
            objects,
        })
        .unwrap(),
    }));
    if host
        .snapshot()
        .is_some_and(|s| s.state().documents.contains_key(&child))
    {
        state.sync(host);
        state.error.clear();
        let selected = state
            .motion
            .as_ref()
            .and_then(|m| {
                m.graph.nodes.iter().find(|n| {
                    n.kind == KIND
                        && n.settings::<Settings>()
                            .ok()
                            .and_then(|s| s.source)
                            .is_some_and(|s| s.document == child)
                })
            })
            .map(|n| n.id);
        state.selected = selected.into_iter().collect();
        host.command(DesktopCommand::Select(Selection {
            document: Some(document),
            objects: state.selected.clone(),
        }));
    } else {
        state.error = host.state().status.clone();
    }
}

pub(super) fn open(
    state: &mut State,
    host: &mut dyn DesktopClient,
    id: ObjectId,
) -> Result<(), String> {
    let settings = state
        .motion
        .as_ref()
        .ok_or("No composition")?
        .graph
        .node(id)?
        .settings::<Settings>()?;
    let source = settings
        .source
        .as_ref()
        .ok_or("Choose a source composition")?;
    let mut request = settings
        .request(
            host.state()
                .navigation
                .last()
                .map(|v| v.time)
                .unwrap_or(Time::ZERO),
        )?
        .unwrap();
    request.info = host.video_info(source.document)?;
    let time = request
        .mapped_time()?
        .ok_or("The playhead is outside this composition's source range")?;
    state.cancel(host);
    host.command(DesktopCommand::Navigate(ViewLocation {
        document: source.document,
        time,
        label: "Composition".into(),
    }));
    state.sync(host);
    state.error.clear();
    Ok(())
}

pub(super) fn source(
    ui: &Ui,
    host: &mut dyn DesktopClient,
    state: &mut State,
    id: ObjectId,
) -> bool {
    let Some(node) = state
        .motion
        .as_ref()
        .and_then(|m| m.graph.node(id).ok())
        .filter(|n| n.kind == KIND)
    else {
        return false;
    };
    let Ok(mut settings) = node.settings::<Settings>() else {
        return false;
    };
    let Some(reference) = &settings.source else {
        ui.text_disabled("Place a composition from Project.");
        return true;
    };
    let info = match host.video_info(reference.document) {
        Ok(info) => info,
        Err(error) => {
            ui.text_wrapped(format!("Composition source unavailable: {error}"));
            return true;
        }
    };
    let source_name = host
        .snapshot()
        .and_then(|s| {
            s.state()
                .organization
                .items
                .iter()
                .find(|item| item.id == fold_project::ItemId::Document(reference.document))
                .map(|item| item.name.clone())
        })
        .unwrap_or_else(|| "Composition".into());
    ui.text_wrapped(format!("Source: {source_name}"));
    let locked = state
        .motion
        .as_ref()
        .and_then(|m| m.scene.as_ref())
        .is_some_and(|s| s.objects.iter().any(|o| o.owns(id) && o.locked));
    let disabled = ui.begin_disabled_with_cond(locked);
    let mut source_in = time_view::frame(settings.source_start, info.rate) as i32;
    ui.text("Source in (frames)");
    ui.set_next_item_width(ui.content_region_avail()[0]);
    if ui.input_int("##source-in", &mut source_in) && source_in >= 0 {
        settings.source_start = time_view::time(i64::from(source_in), info.rate).unwrap();
        crate::authoring::node(state.motion.as_mut().unwrap(), id)
            .unwrap()
            .settings["source_start"] = serde_json::to_value(settings.source_start).unwrap();
        state.property_editing = true;
        state.preview(host);
    }
    let finish = ui.is_item_deactivated_after_edit();
    drop(disabled);
    if state.property_editing && ui.is_key_pressed(fold_ui::sdk::imgui::Key::Escape) {
        state.cancel(host);
    } else if state.property_editing && finish {
        state.commit(host);
    }
    let time = host
        .state()
        .navigation
        .last()
        .map(|v| v.time)
        .unwrap_or(Time::ZERO);
    if let Ok(Some(mut request)) = settings.request(time) {
        request.info = info;
        match request.mapped_time() {
            Ok(Some(local)) => {
                ui.text_disabled(format!(
                    "Source time: {}/{} s",
                    local.numerator(),
                    local.denominator()
                ));
                if ui.button("Open composition")
                    && let Err(error) = open(state, host, id)
                {
                    state.error = error;
                }
            }
            Ok(None) => ui.text_disabled("Outside source range · transparent"),
            Err(error) => ui.text_wrapped(error),
        }
    }
    if !state.error.is_empty() {
        ui.text_wrapped(&state.error);
    }
    true
}

pub(super) fn settings(ui: &Ui, host: &mut dyn DesktopClient, state: &mut State) {
    let Some(info) = &mut state.composition_settings else {
        return;
    };
    let mut open = true;
    let mut apply = false;
    let mut cancel = false;
    ui.window("Graph settings")
        .opened(&mut open)
        .flags(fold_ui::sdk::imgui::WindowFlags::ALWAYS_AUTO_RESIZE)
        .build(|| {
            let mut values = [
                info.width as i32,
                info.height as i32,
                info.frames as i32,
                info.rate[0] as i32,
                info.rate[1] as i32,
            ];
            for (label, value) in [
                "Width",
                "Height",
                "Duration (frames)",
                "Rate numerator",
                "Rate denominator",
            ]
            .into_iter()
            .zip(&mut values)
            {
                ui.input_int(label, value);
            }
            if values.iter().all(|v| *v > 0) {
                *info = fold_media::VideoInfo {
                    width: values[0] as u32,
                    height: values[1] as u32,
                    frames: values[2] as u32,
                    rate: [values[3] as u32, values[4] as u32],
                };
            }
            apply = ui.button("Apply");
            ui.same_line();
            cancel = ui.button("Cancel") || ui.is_key_pressed(fold_ui::sdk::imgui::Key::Escape);
        });
    if apply {
        let info = info.clone();
        match info.validate() {
            Ok(()) => {
                state.change(host, |m| {
                    m.info = info;
                    Ok(None)
                });
                state.composition_settings = None;
            }
            Err(error) => state.error = error,
        }
    } else if cancel || !open {
        state.composition_settings = None;
    }
}
