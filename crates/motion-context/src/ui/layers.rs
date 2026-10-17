//! Layer hierarchy and keyframes are one view over the native Motion document.
use super::state::State;
use fold_foundation::ObjectId;
use fold_motion_canvas::{
    design,
    editing::{self, Edit},
    hierarchy,
    model::{Canvas, Content},
    tools,
};
use fold_platform::desktop::{DesktopCommand, TransportAction};
use fold_ui::sdk::{
    self, ExtensionUi,
    animation_editor::{Channel, Context, Editor, LayerControls, Track},
    toolbar::{self, ToolbarIcon},
};

fn ordered(canvas: &Canvas, ids: &[ObjectId], out: &mut Vec<ObjectId>) {
    for id in ids.iter().rev() {
        out.push(*id);
        if let Content::Group { children, .. } = &canvas.objects[id].content {
            ordered(canvas, children, out);
        }
    }
}
pub(super) fn draw(context: ExtensionUi<'_>, state: &mut State, editor: &mut Editor) {
    let ExtensionUi { ui, host } = context;
    let Some(canvas) = state.canvas().cloned() else {
        ui.text_disabled("Create or open a Motion document to begin.");
        return;
    };
    let Some(document) = state.document else {
        return;
    };
    let result = (|| -> Result<(), String> {
        let disabled = ui.begin_disabled_with_cond(state.busy() || host.state().playing);
        let mut immediate = None;
        let mut select = None;
        if let Some(_menu) = toolbar::menu_button(ui, "Add", "Add artwork") {
            for (kind, label) in [
                ("text", "Text"),
                ("rectangle", "Rectangle"),
                ("ellipse", "Ellipse"),
                ("group", "Group"),
                ("script", "Procedural artwork"),
            ] {
                if ui.menu_item(label) {
                    let id = ObjectId::new();
                    let mut object = tools::new_object(kind)?;
                    object.transform.x.value = canvas.width as f64 / 2.;
                    object.transform.y.value = canvas.height as f64 / 2.;
                    immediate = Some(vec![Edit::Insert {
                        id,
                        object: Box::new(object),
                        parent: None,
                        index: canvas.roots.len(),
                    }]);
                    select = Some(vec![id]);
                }
            }
        }
        ui.same_line();
        if let Some(_menu) = toolbar::icon_menu(ui, "arrange", "Arrange and align artwork") {
            if ui.menu_item("Group selection (Ctrl+G)") {
                let (id, edits) = hierarchy::group(&canvas, &state.selected)?;
                immediate = Some(edits);
                select = Some(vec![id]);
            }
            for (label, mode) in [
                ("Bring forward", hierarchy::Order::Forward),
                ("Bring to front", hierarchy::Order::Front),
                ("Send backward", hierarchy::Order::Backward),
                ("Send to back", hierarchy::Order::Back),
            ] {
                if ui.menu_item(label) {
                    immediate = Some(hierarchy::arrange(&canvas, &state.selected, mode)?);
                }
            }
            ui.separator();
            for (label, mode) in [
                ("Align left", design::Align::Left),
                ("Align center", design::Align::Center),
                ("Align right", design::Align::Right),
                ("Align top", design::Align::Top),
                ("Align middle", design::Align::Middle),
                ("Align bottom", design::Align::Bottom),
                (
                    "Distribute horizontally",
                    design::Align::DistributeHorizontal,
                ),
                ("Distribute vertically", design::Align::DistributeVertical),
            ] {
                if ui.menu_item(label) {
                    immediate = Some(design::align(
                        &canvas,
                        &state.selected,
                        state.time,
                        mode,
                        host.state().auto_key,
                    )?);
                }
            }
        }
        ui.same_line();
        if toolbar::icon_button(
            ui,
            "settings",
            ToolbarIcon::Settings,
            "Canvas size, frame rate and duration",
        ) {
            state.settings = Some((canvas.clone(), canvas.clone()));
            ui.open_popup("motion-settings");
        }
        if let Some(_popup) = ui.begin_popup("motion-settings") {
            if let Some((expected, draft)) = &mut state.settings {
                let mut width = draft.width as i32;
                let mut height = draft.height as i32;
                let mut rate = draft.rate.map(|v| v as i32);
                let mut duration = fold_animation::seconds(draft.duration);
                ui.input_int("Width", &mut width);
                ui.input_int("Height", &mut height);
                ui.input_int("Rate numerator", &mut rate[0]);
                ui.input_int("Rate denominator", &mut rate[1]);
                ui.input_double("Duration (seconds)", &mut duration);
                draft.width = width.max(1) as u32;
                draft.height = height.max(1) as u32;
                draft.rate = rate.map(|v| v.max(1) as u32);
                if duration.is_finite() && duration > 0. {
                    draft.duration = fold_foundation::Time::new(
                        (duration * 1_000_000.).round() as i64,
                        1_000_000,
                    )
                    .map_err(|e| e.to_string())?;
                }
                if ui.button("Apply") {
                    immediate = Some(vec![Edit::Settings {
                        expected: Box::new(expected.clone()),
                        width: draft.width,
                        height: draft.height,
                        rate: draft.rate,
                        duration: draft.duration,
                    }]);
                    ui.close_current_popup();
                }
                ui.same_line();
                if ui.button("Cancel") {
                    ui.close_current_popup();
                }
            }
        }

        if ui.is_window_focused()
            && !ui.io().want_text_input()
            && ui.io().key_ctrl()
            && ui.is_key_pressed(sdk::imgui::Key::G)
        {
            let (id, edits) = hierarchy::group(&canvas, &state.selected)?;
            immediate = Some(edits);
            select = Some(vec![id]);
        }
        drop(disabled);
        if let Some(edits) = immediate {
            state.preview(host, edits)?;
            state.commit(host)?;
            if let Some(selected) = select {
                state.select(host, selected);
            }
            return Ok(());
        }
        ui.separator();
        let mut ids = vec![];
        ordered(&canvas, &canvas.roots, &mut ids);
        let tracks: Vec<_> = ids
            .iter()
            .map(|id| {
                let object = &canvas.objects[id];
                Track {
                    id: *id,
                    parent: hierarchy::placement(&canvas, *id).ok().and_then(|v| v.0),
                    label: object.name.clone(),
                    locked: editing::editable(&canvas, *id).is_err(),
                    range: None,
                    layer: Some(LayerControls {
                        accepts_children: matches!(object.content, Content::Group { .. }),
                        visible: object.visible,
                        icon: match object.content {
                            Content::Text { .. } => "T",
                            Content::Group { .. } => "G",
                            Content::Script { .. } => "ƒ",
                            Content::Image { .. } => "I",
                            _ => "S",
                        },
                    }),
                }
            })
            .collect();
        let mut bindings = vec![];
        let mut channels = vec![];
        for id in &ids {
            let object = &canvas.objects[id];
            for (label, scalar) in object.named_controls() {
                if let Some(curve) = &scalar.animation {
                    bindings.push((*id, scalar.id));
                    channels.push(Channel {
                        object: *id,
                        node_label: object.name.clone(),
                        property: scalar.id.to_string(),
                        property_label: label.into(),
                        component: String::new(),
                        path: scalar.id.to_string(),
                        curve: curve.clone(),
                    });
                }
            }
        }
        let mut auto_key = host.state().auto_key;
        let frames = (sdk::time_view::frame(canvas.duration, canvas.rate).ceil() as u32).max(1);
        let disabled = ui.begin_disabled_with_cond(state.busy() || host.state().playing);
        let response = editor.draw(
            ui,
            &mut channels,
            Context {
                cache: &Default::default(),
                document,
                generation: state.generation,
                time: state.time,
                rate: canvas.rate,
                frames,
                nodes: &state.selected,
                tracks: &tracks,
                auto_key: &mut auto_key,
            },
        );
        drop(disabled);
        if auto_key != host.state().auto_key {
            host.command(DesktopCommand::AutoKey(auto_key));
        }
        if let Some(time) = response.seek {
            host.command(DesktopCommand::Transport(TransportAction::Jump(
                (sdk::time_view::frame(time, canvas.rate).round() as u32).min(frames - 1),
            )));
        }
        if let Some(id) = response.select_node {
            let mut selected = if ui.io().key_shift() || ui.io().key_ctrl() {
                state.selected.clone()
            } else {
                vec![]
            };
            if let Some(i) = selected.iter().position(|v| *v == id) {
                selected.remove(i);
            } else {
                selected.push(id);
            }
            state.select(host, selected);
        }
        if response.edit.cancelled {
            state.cancel(host);
            return Ok(());
        }
        let mut edits = vec![];
        if response.edit.changed {
            for (channel, (object, scalar)) in channels.iter().zip(&bindings) {
                let expected = state.before.as_ref().ok_or("No Motion document")?.objects[object]
                    .scalars()
                    .into_iter()
                    .find(|s| s.id == *scalar)
                    .ok_or("Missing animation property")?
                    .clone();
                let mut value = expected.clone();
                value.animation = Some(channel.curve.clone());
                if value != expected {
                    edits.push(Edit::Parameter {
                        object: *object,
                        expected,
                        value,
                    });
                }
            }
        }
        let mut immediate = false;
        if let Some((id, visible)) = response.visibility {
            let expected = canvas.objects[&id].clone();
            let mut object = expected.clone();
            object.visible = visible;
            edits.push(Edit::Replace {
                id,
                expected: Box::new(expected),
                object: Box::new(object),
            });
            immediate = true;
        }
        if let Some((object, value)) = response.lock {
            edits.push(Edit::Lock {
                object,
                expected: canvas.objects[&object].locked,
                value,
            });
            immediate = true;
        }
        if response.delete_layers {
            for id in design::selection_roots(&canvas, &state.selected)? {
                edits.push(Edit::Delete {
                    id,
                    expected: Box::new(canvas.objects[&id].clone()),
                });
            }
            immediate = true;
        }
        if let Some((id, other, after)) = response.reorder {
            let (parent, index) = hierarchy::placement(&canvas, other)?;
            let (old_parent, old_index) = hierarchy::placement(&canvas, id)?;
            // The layer list is front-to-back; native paint order is reversed.
            let mut index = index + usize::from(!after);
            if parent == old_parent && old_index < index {
                index -= 1;
            }
            edits.extend(reparent(
                &canvas,
                id,
                parent,
                index,
                state.time,
                host.state().auto_key,
            )?);
            immediate = true;
        }
        if let Some((id, parent)) = response.reparent {
            let count = hierarchy::children(&canvas, parent)?.len();
            let old = hierarchy::placement(&canvas, id)?.0;
            edits.extend(reparent(
                &canvas,
                id,
                parent,
                count - usize::from(old == parent),
                state.time,
                host.state().auto_key,
            )?);
            immediate = true;
        }
        if !edits.is_empty() {
            state.preview(host, edits)?;
        }
        if response.edit.finished || immediate {
            state.commit(host)?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        state.cancel(host);
        state.error = error;
    }
    if !state.error.is_empty() {
        ui.text_wrapped(&state.error);
    }
}

fn reparent(
    canvas: &Canvas,
    id: ObjectId,
    parent: Option<ObjectId>,
    index: usize,
    time: fold_foundation::Time,
    auto_key: bool,
) -> Result<Vec<Edit>, String> {
    let mut edits = vec![Edit::Move { id, parent, index }];
    if hierarchy::placement(canvas, id)?.0 != parent {
        let before = tools::transforms(canvas, time)?[&id];
        let moved = editing::apply(canvas, &edits)?;
        let after = tools::transforms(&moved, time)?[&id];
        edits.extend(design::transform(
            &moved,
            &[id],
            time,
            before.compose(after.inverse()?),
            auto_key,
        )?);
    }
    Ok(edits)
}
