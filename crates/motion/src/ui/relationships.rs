//! Artist-facing relationship controls over the authored scene model.
use crate::{
    Motion,
    scene::{ConstraintKind, Mask, MaskOperation, MaskSpace, Object},
};
use fold_foundation::ObjectId;
use fold_ui::sdk::{
    EditResponse,
    imgui::{Drag, Ui},
};
pub(super) enum Action {
    Parent(Option<ObjectId>, bool),
    Constraint(ObjectId, ConstraintKind),
    LocalMask,
}
pub(super) fn draw(
    ui: &Ui,
    m: &Motion,
    object: &mut Object,
    response: &mut EditResponse,
    clock: (fold_foundation::Time, bool),
) -> Option<Action> {
    let scene = m.scene.as_ref()?;
    let name = |id: ObjectId| {
        scene
            .objects
            .iter()
            .find(|o| o.id == id)
            .map(|o| o.name.as_str())
            .unwrap_or("Missing object")
    };
    let _disabled = ui.begin_disabled_with_cond(object.locked);
    let mut action = None;
    if let Some(_menu) = ui.begin_combo("Group", object.parent.map(name).unwrap_or("Scene root")) {
        for preserve in [true, false] {
            if let Some(_menu) = ui.begin_menu(if preserve {
                "Preserve current placement"
            } else {
                "Keep local coordinates"
            }) {
                if ui.selectable("Scene root") {
                    action = Some(Action::Parent(None, preserve));
                }
                for o in &scene.objects {
                    if o.id != object.id
                        && m.graph
                            .node(o.source.node)
                            .is_ok_and(|n| n.kind == "fold.motion.scene_children")
                    {
                        let _id = ui.push_id(&format!("{:?}", o.id));
                        if ui.selectable(&o.name) {
                            action = Some(Action::Parent(Some(o.id), preserve));
                        }
                    }
                }
            }
        }
    }
    scalar(
        ui,
        clock,
        ("scene.opacity", "Opacity"),
        &mut object.opacity,
        &mut object.animation,
        response,
    );
    if ui.checkbox("Isolate group effects", &mut object.isolated) {
        response.changed = true;
        response.finished = true;
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(
            "Combine contents before group opacity and masks. Masks always isolate their owner.",
        );
    }
    if ui.collapsing_header("Constraints", fold_ui::sdk::imgui::TreeNodeFlags::empty()) {
        if let Some(_menu) = ui.begin_combo("Add constraint", "Choose relationship") {
            for kind in ConstraintKind::ALL {
                if let Some(_menu) = ui.begin_menu(kind.label()) {
                    for o in &scene.objects {
                        if o.id != object.id {
                            let _id = ui.push_id(&format!("{:?}", o.id));
                            if ui.selectable(&o.name) {
                                action = Some(Action::Constraint(o.id, kind));
                            }
                        }
                    }
                }
            }
        }
        let mut remove = None;
        let mut move_up = None;
        for (i, c) in object.constraints.iter_mut().enumerate() {
            let _id = ui.push_id(&format!("{:?}", c.id));
            ui.text(c.kind.label());
            if let Some(_combo) = ui.begin_combo("Target", name(c.target)) {
                for o in &scene.objects {
                    if o.id != object.id && ui.selectable(format!("{}##{:?}", o.name, o.id)) {
                        c.target = o.id;
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
            scalar(
                ui,
                clock,
                ("influence", "Influence"),
                &mut c.influence,
                &mut c.animation,
                response,
            );
            if c.kind == ConstraintKind::Path {
                scalar(
                    ui,
                    clock,
                    ("progress", "Path progress"),
                    &mut c.progress,
                    &mut c.animation,
                    response,
                );
                if ui.checkbox("Orient to path", &mut c.orient) {
                    response.changed = true;
                    response.finished = true;
                }
            }
            let mut offset = [c.offset[4], c.offset[5]];
            response.changed |= Drag::new("Offset X / Y")
                .speed(0.25)
                .build_array(ui, &mut offset);
            response.finished |= ui.is_item_deactivated_after_edit();
            c.offset[4] = offset[0];
            c.offset[5] = offset[1];
            if ui.small_button("Remove") {
                remove = Some(i);
            }
            ui.same_line();
            if i > 0 && ui.small_button("Earlier") {
                move_up = Some(i);
            }
        }
        if let Some(i) = remove {
            object.constraints.remove(i);
            response.changed = true;
            response.finished = true;
        }
        if let Some(i) = move_up {
            object.constraints.swap(i, i - 1);
            response.changed = true;
            response.finished = true;
        }
    }
    if ui.collapsing_header("Masks", fold_ui::sdk::imgui::TreeNodeFlags::empty()) {
        if ui.small_button("New local mask") {
            action = Some(Action::LocalMask);
        }
        if let Some(_combo) = ui.begin_combo("Reference mask", "Choose object") {
            for o in &scene.objects {
                if o.id != object.id && ui.selectable(format!("{}##{:?}", o.name, o.id)) {
                    object.masks.push(Mask::new(o.id, MaskSpace::World));
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
        let mut remove = None;
        let mut move_up = None;
        for (i, mask) in object.masks.iter_mut().enumerate() {
            let _id = ui.push_id(&format!("{:?}", mask.id));
            if let Some(_combo) = ui.begin_combo("Source", name(mask.source)) {
                for o in &scene.objects {
                    if o.id != object.id && ui.selectable(format!("{}##{:?}", o.name, o.id)) {
                        mask.source = o.id;
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
            if let Some(_combo) = ui.begin_combo(
                "Space",
                if mask.space == MaskSpace::Local {
                    "Owner local"
                } else {
                    "Scene world"
                },
            ) {
                for (label, space) in [
                    ("Owner local", MaskSpace::Local),
                    ("Scene world", MaskSpace::World),
                ] {
                    if ui.selectable(label) {
                        mask.space = space;
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
            if let Some(_combo) = ui.begin_combo("Operation", format!("{:?}", mask.operation)) {
                for op in [
                    MaskOperation::Intersect,
                    MaskOperation::Add,
                    MaskOperation::Subtract,
                ] {
                    if ui.selectable(format!("{op:?}")) {
                        mask.operation = op;
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
            if ui.checkbox("Invert", &mut mask.invert) {
                response.changed = true;
                response.finished = true;
            }
            scalar(
                ui,
                clock,
                ("opacity", "Mask opacity"),
                &mut mask.opacity,
                &mut mask.animation,
                response,
            );
            scalar(
                ui,
                clock,
                ("feather", "Feather (px)"),
                &mut mask.feather,
                &mut mask.animation,
                response,
            );
            if ui.small_button("Remove") {
                remove = Some(i);
            }
            ui.same_line();
            if i > 0 && ui.small_button("Earlier") {
                move_up = Some(i);
            }
        }
        if let Some(i) = remove {
            object.masks.remove(i);
            response.changed = true;
            response.finished = true;
        }
        if let Some(i) = move_up {
            object.masks.swap(i, i - 1);
            response.changed = true;
            response.finished = true;
        }
    }
    action
}

fn scalar(
    ui: &Ui,
    clock: (fold_foundation::Time, bool),
    property: (&str, &str),
    value: &mut f64,
    channels: &mut crate::scene::animation::Channels,
    response: &mut EditResponse,
) {
    let mut values = [*value];
    fold_ui::sdk::animated_property::AnimatedProperty {
        id: property.0,
        label: property.1,
        components: &["Value"],
        unit: "",
        range: Some([0., if property.0 == "feather" { 256. } else { 1. }]),
        color: None,
    }
    .draw(ui, &mut values, channels, clock.0, clock.1, response);
    *value = values[0];
}
