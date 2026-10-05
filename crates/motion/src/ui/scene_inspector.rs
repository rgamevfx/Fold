//! Scene metadata and navigation reuse the ordinary animated node inspector.
use super::state::State;
use fold_foundation::ObjectId;
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_ui::sdk::{imgui::Ui, time_view};

pub(super) fn draw(
    ui: &Ui,
    host: &mut dyn DesktopClient,
    state: &mut State,
    selected: ObjectId,
) -> bool {
    let Some(mut object) = state
        .motion
        .as_ref()
        .and_then(|m| m.scene.as_ref())
        .and_then(|s| s.objects.iter().find(|o| o.owns(selected)))
        .cloned()
    else {
        return false;
    };
    let mut changed = false;
    let locked = ui.begin_disabled_with_cond(object.locked);
    let name_changed = ui.input_text("Name", &mut object.name).build();
    changed |= name_changed;
    let name_finished = ui.is_item_deactivated_after_edit();
    changed |= ui.checkbox("Visible", &mut object.visible);
    drop(locked);
    ui.same_line();
    changed |= ui.checkbox("Locked", &mut object.locked);
    let mut finished = name_finished || (changed && !name_changed);
    let info = state.motion.as_ref().unwrap().info.clone();
    let mut start = time_view::frame(object.start, info.rate) as i32;
    let mut end = time_view::frame(object.end, info.rate) as i32;
    {
        let _disabled = ui.begin_disabled_with_cond(object.locked);
        if ui.input_int("In frame", &mut start)
            && let Ok(t) = time_view::time(i64::from(start), info.rate)
        {
            object.start = t;
            changed = true;
        }
        finished |= ui.is_item_deactivated_after_edit();
        if ui.input_int("Out frame", &mut end)
            && let Ok(t) = time_view::time(i64::from(end), info.rate)
        {
            object.end = t;
            changed = true;
        }
        finished |= ui.is_item_deactivated_after_edit();
    }
    let mut relationship_response = fold_ui::sdk::EditResponse::default();
    let action = super::relationships::draw(
        ui,
        state.motion.as_ref().unwrap(),
        &mut object,
        &mut relationship_response,
        (
            host.state()
                .navigation
                .last()
                .map(|n| n.time)
                .unwrap_or(fold_foundation::Time::ZERO),
            state.auto_key,
        ),
    );
    changed |= relationship_response.changed;
    finished |= relationship_response.finished;
    if let Some(action) = action {
        let time = host
            .state()
            .navigation
            .last()
            .map(|n| n.time)
            .unwrap_or(fold_foundation::Time::ZERO);
        state.change_scene(host,|m| {
            match action {
                super::relationships::Action::Parent(parent,preserve)=>crate::authoring::scene::reparent(m,object.id,parent,time,preserve)?,
                super::relationships::Action::Constraint(target,kind)=>crate::authoring::scene::add_constraint(m,object.id,target,kind,time,true)?,
                super::relationships::Action::LocalMask=>{
                    let mask=crate::authoring::scene::create_object(m,"fold.motion.path")?;
                    crate::authoring::node(m,mask)?.settings["segments"]=serde_json::json!([{"Move":[-100.,-50.]},{"Line":[100.,-50.]},{"Line":[100.,50.]},{"Line":[-100.,50.]},"Close"]);
                    let scene=m.scene.as_mut().unwrap();
                    let source=scene.objects.iter_mut().find(|o|o.id==mask).unwrap();source.visible=false;source.name=format!("{} mask",object.name);
                    scene.objects.iter_mut().find(|o|o.id==object.id).unwrap().masks.push(crate::scene::Mask::new(mask,crate::scene::MaskSpace::Local));
                    return Ok(Some(mask));
                }
            }
            Ok(Some(object.id))
        });
        return true;
    }
    if changed {
        if let Some(scene) = state.motion.as_mut().and_then(|m| m.scene.as_mut()) {
            *scene
                .objects
                .iter_mut()
                .find(|o| o.id == object.id)
                .unwrap() = object.clone();
        }
        state.property_editing = true;
        state.preview(host);
    }
    if state.property_editing && ui.is_key_pressed(fold_ui::sdk::imgui::Key::Escape) {
        state.cancel(host);
        return true;
    }
    if state.property_editing && finished {
        state.commit(host);
    }
    let mut choices = vec![(object.id, "Content".to_string())];
    if let Some(id) = object.transform {
        choices.push((id, "Transform".into()));
    }
    if let Some(id) = object.appearance {
        choices.push((id, "Appearance".into()));
    }
    if let Some(motion) = &state.motion {
        for &id in &object.modifiers {
            let name = motion
                .graph
                .node(id)
                .ok()
                .and_then(|n| n.settings::<crate::nodes::interface::GroupSettings>().ok())
                .and_then(|s| s.group)
                .and_then(|id| motion.groups.get(&id))
                .map(|g| g.name.clone())
                .unwrap_or("Modifier".into());
            choices.push((id, name));
        }
    }
    let label = choices
        .iter()
        .find(|(id, _)| *id == selected)
        .map(|(_, s)| s.as_str())
        .unwrap_or("Content");
    if let Some(_combo) = ui.begin_combo("Properties", label) {
        for (id, label) in choices {
            let _id = ui.push_id(&format!("{id:?}"));
            if ui.selectable(label) {
                state.cancel(host);
                state.selected = vec![id];
                host.command(DesktopCommand::Select(Selection {
                    document: state.document,
                    objects: vec![id],
                }));
                return true;
            }
        }
    }
    if object.modifiers.contains(&selected) && !object.locked {
        for (label, delta) in [("Earlier", -1), ("Later", 1)] {
            if ui.small_button(label) {
                state.change_scene(host, |m| {
                    crate::authoring::scene::move_modifier(m, object.id, selected, delta)?;
                    Ok(Some(selected))
                });
                return true;
            }
            ui.same_line();
        }
        if ui.small_button("Remove modifier") {
            state.change_scene(host, |m| {
                crate::authoring::scene::remove_modifier(m, object.id, selected)?;
                Ok(Some(object.id))
            });
            return true;
        }
    }
    ui.separator();
    object.locked
}
