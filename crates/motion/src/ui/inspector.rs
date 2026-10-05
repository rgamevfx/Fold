use super::state::Shared;
use crate::{
    fields::Datum,
    graph::{Input, registry},
    package,
};
use fold_ui::sdk::{
    EditResponse as Response, ExtensionUi, NumericProperty, Panel, UiId,
    imgui::{Drag, Ui},
    toolbar::{icon_menu, tooltip},
};
pub struct Inspector {
    pub state: Shared,
}
impl Panel for Inspector {
    fn id(&self) -> &'static str {
        package::INSPECTOR
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::MOTION)
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let aces = host
            .snapshot()
            .is_some_and(|s| fold_platform::color::project(&s).ok().flatten().is_some());
        let mut state = self.state.borrow_mut();
        state.sync(host);
        let Some(&selected) = state.selected.first().filter(|_| state.selected.len() == 1) else {
            ui.text_wrapped("Select an object, modifier or node to edit its properties.");
            return;
        };
        let _scope = state.document.map(|document| {
            UiId {
                package: crate::PACKAGE,
                panel: package::INSPECTOR,
                document,
                object: selected,
            }
            .scope(ui)
        });
        if state.group.is_none() && super::scene_inspector::draw(ui, host, &mut state, selected) {
            return;
        }
        let Some(mut motion) = state.view_motion() else {
            return;
        };
        let objects: Vec<_> = state
            .motion
            .as_ref()
            .and_then(|m| m.scene.as_ref())
            .map(|s| s.objects.iter().map(|o| (o.id, o.name.clone())).collect())
            .unwrap_or_default();
        let mut signature = motion
            .graph
            .node(selected)
            .and_then(|node| motion.signature(node));
        let fonts: Vec<_> = motion
            .fonts
            .iter()
            .map(|(id, font)| (*id, font.name.clone()))
            .collect();
        let groups: Vec<_> = motion
            .groups
            .iter()
            .map(|(id, g)| (*id, g.name.clone()))
            .collect();
        let names: std::collections::BTreeMap<_, _> = motion
            .graph
            .nodes
            .iter()
            .map(|n| {
                (
                    n.id,
                    registry::find(&n.kind)
                        .map(|d| d.name)
                        .unwrap_or(&n.kind)
                        .to_owned(),
                )
            })
            .collect();
        let time = host
            .state()
            .navigation
            .last()
            .map(|v| v.time)
            .unwrap_or(fold_foundation::Time::ZERO);
        let group_id = motion
            .graph
            .node(selected)
            .ok()
            .and_then(|n| n.settings::<crate::nodes::interface::GroupSettings>().ok())
            .and_then(|s| s.group);
        let port_labels: std::collections::BTreeMap<_, _> = group_id
            .and_then(|id| motion.groups.get(&id))
            .map(|g| {
                g.inputs
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let Some(node) = motion.graph.nodes.iter_mut().find(|n| n.id == selected) else {
            return;
        };
        ui.text(
            registry::find(&node.kind)
                .map(|d| d.name)
                .unwrap_or(&node.kind),
        );
        ui.separator();
        if let Some(group) = group_id {
            if ui.button("Edit Graph") {
                state.cancel(host);
                state.parents.push(None);
                state.group = Some(group);
                state.network_requested = true;
                state.selected.clear();
                return;
            }
            ui.same_line();
            if ui.button("Make unique") {
                state.change(host, |m| {
                    let mut definition = m.groups.get(&group).ok_or("missing group")?.clone();
                    definition.name.push_str(" Copy");
                    let id = fold_foundation::ObjectId::new();
                    m.groups.insert(id, definition);
                    crate::authoring::node(m, selected)?.settings["group"] =
                        serde_json::to_value(id).unwrap();
                    Ok(Some(selected))
                });
                return;
            }
            ui.text_disabled("Graph edits affect every instance of this group. Make unique for a local variation.");
        }
        if node.kind == "fold.motion.output"
            && aces
            && let (Some(snapshot), Some(document)) = (host.snapshot(), state.document)
        {
            match fold_platform::color::output(&snapshot, document) {
                Ok(mut transform) => {
                    if fold_ui::sdk::color_controls::output(
                        ui,
                        &host.state().color_choices,
                        &mut transform,
                    ) {
                        state.cancel(host);
                        host.command(fold_platform::desktop::DesktopCommand::SetOutputColor {
                            document,
                            transform,
                        });
                        state.sync(host);
                        return;
                    }
                }
                Err(error) => ui.text_wrapped(error),
            }
        }
        let mut response = Response::default();
        let mut assign_group = None;
        let mut animate = None;
        let mut publish = None;
        let mut focus = None;
        if let Some(_menu) = icon_menu(ui, "node-controls", "Drivers and published controls") {
            let inputs: Vec<_> = node
                .inputs
                .iter()
                .filter_map(|(key, input)| matches!(input, Input::Value(_)).then_some(key.clone()))
                .collect();
            for key in inputs {
                if let Some(_submenu) = ui.begin_menu(property_label(&key)) {
                    if ui.menu_item("Create keyframe driver") {
                        animate = Some(key.clone());
                    }
                    if ui.menu_item(if state.group.is_some() {
                        "Expose group control"
                    } else {
                        "Publish control"
                    }) {
                        publish = Some(key);
                    }
                }
            }
        }

        if registry::find(&node.kind).is_ok_and(|d| {
            d.inputs
                .iter()
                .any(|s| s.kind == crate::fields::Kind::Generic)
        }) {
            let current = node
                .settings
                .get("value_type")
                .and_then(|v| v.as_str())
                .unwrap_or("Scalar");
            if let Some(_combo) = ui.begin_combo("Value type", current) {
                for (name, kind) in [
                    ("Scalar", crate::fields::Kind::Scalar),
                    ("Vector", crate::fields::Kind::Vector),
                    ("Color", crate::fields::Kind::Color),
                ] {
                    if ui.selectable(name) {
                        node.settings["value_type"] = serde_json::to_value(kind).unwrap();
                        signature = registry::find(&node.kind).and_then(|d| d.signature(node));
                        if let Ok((sockets, _)) = &signature {
                            for socket in sockets {
                                if matches!(node.inputs.get(&socket.id),Some(Input::Value(v)) if v.kind()!=socket.kind)
                                {
                                    if let Some(value) = &socket.default {
                                        node.inputs
                                            .insert(socket.id.clone(), Input::Value(value.clone()));
                                    } else {
                                        node.inputs.remove(&socket.id);
                                    }
                                }
                            }
                        }
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
        }
        if let Ok((sockets, _)) = signature {
            for socket in sockets {
                let key = socket.id;
                if !node.inputs.contains_key(&key)
                    && let Some(default) = &socket.default
                {
                    node.inputs
                        .insert(key.clone(), Input::Value(default.clone()));
                }
                let _id = ui.push_id(&key);
                if let Some(Input::Value(value)) = node.inputs.get_mut(&key)
                    && let Some((components, mut values)) =
                        crate::animation::parameters::components(value)
                    && key != "alignment"
                {
                    fold_ui::sdk::animated_property::AnimatedProperty {
                        id: &key,
                        label: &port_labels
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| property_label(&key)),
                        components,
                        unit: socket.unit,
                        range: None,
                        color: matches!(value, Datum::Color(_)).then_some(aces),
                    }
                    .draw(
                        ui,
                        &mut values,
                        &mut node.animation,
                        time,
                        state.auto_key,
                        &mut response,
                    );
                    crate::animation::parameters::set_components(value, &values);
                    continue;
                }
                ui.text(property_label(&key));
                if !socket.unit.is_empty() && key != "alignment" {
                    ui.same_line();
                    ui.text_disabled(&socket.unit);
                }
                match node.inputs.get_mut(&key) {
                    Some(Input::Link(link)) => {
                        ui.text_disabled(format!(
                            "Driven by {}",
                            names
                                .get(&link.node)
                                .map(String::as_str)
                                .unwrap_or("missing node")
                        ));
                        if ui.small_button("Open driver") {
                            focus = Some(link.node);
                        }
                        ui.same_line();
                        if ui.small_button("Disconnect") {
                            node.inputs.remove(&key);
                            response.changed = true;
                            response.finished = true;
                        }
                    }
                    Some(Input::Value(value)) => {
                        if node.kind == "fold.motion.text" && key == "alignment" {
                            alignment(ui, value, aces, &mut response);
                        } else {
                            datum(ui, value, aces, &mut response);
                        }
                    }
                    None => {
                        if let Some(default) = socket.default {
                            if ui.small_button("Use default") {
                                node.inputs.insert(key, Input::Value(default));
                                response.changed = true;
                                response.finished = true;
                            }
                        } else {
                            ui.text_disabled("Connect this input in the graph");
                        }
                    }
                }
            }
        }
        if node.kind == "fold.motion.text" {
            let current = node
                .settings
                .get("font")
                .and_then(|v| serde_json::from_value::<fold_foundation::ObjectId>(v.clone()).ok());
            let font_name = fonts
                .iter()
                .find(|(id, _)| Some(*id) == current)
                .map(|(_, name)| name.as_str())
                .unwrap_or("Missing font");
            if let Some(_combo) = ui.begin_combo("Font", font_name) {
                for (id, name) in fonts {
                    if ui.selectable(name) {
                        node.settings["font"] = serde_json::to_value(id).unwrap();
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
        } else if node.kind == "fold.motion.group_instance" {
            if let Some(_combo) = ui.begin_combo("Group", "Choose reusable group") {
                for (id, name) in groups {
                    if ui.selectable(name) {
                        assign_group = Some(id);
                    }
                }
            }
        } else if node.kind == "fold.motion.object_reference" {
            let current = node
                .settings
                .get("object")
                .and_then(|v| serde_json::from_value::<fold_foundation::ObjectId>(v.clone()).ok());
            let label = objects
                .iter()
                .find(|(id, _)| Some(*id) == current)
                .map(|(_, name)| name.as_str())
                .unwrap_or("Choose object");
            if let Some(_combo) = ui.begin_combo("Object", label) {
                for (id, name) in objects {
                    let _id = ui.push_id(&format!("{id:?}"));
                    if ui.selectable(name) {
                        node.settings["object"] = serde_json::to_value(id).unwrap();
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
            let mut world = node
                .settings
                .get("world")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            if ui.checkbox("Use world coordinates", &mut world) {
                node.settings["world"] = world.into();
                response.changed = true;
                response.finished = true;
            }
            if world {
                let mut owner = node
                    .settings
                    .get("owner_space")
                    .and_then(|v| v.as_bool())
                    .unwrap_or(false);
                if ui.checkbox("Relative to modifier owner", &mut owner) {
                    node.settings["owner_space"] = owner.into();
                    response.changed = true;
                    response.finished = true;
                }
            }
        } else if node.kind == "fold.motion.path" {
            super::path_attributes::draw(
                ui,
                &mut node.settings,
                aces,
                &mut response,
                node.id,
                &mut state.path_attribute_names,
            );
            if let Some(segments) = node.settings.get_mut("segments") {
                settings(ui, segments, &mut response, 0);
            }
        } else if node.kind == "fold.motion.keyframes" {
            ui.text_disabled("Edit keys in the Animation panel.");
        } else {
            settings(ui, &mut node.settings, &mut response, 0);
        }
        if node.kind == "fold.motion.path" && ui.button("Add cubic segment") {
            if let Some(segments) = node
                .settings
                .get_mut("segments")
                .and_then(|v| v.as_array_mut())
            {
                if segments.is_empty() {
                    segments.push(serde_json::json!({"Move":[0.,0.]}));
                }
                segments.push(serde_json::json!({"Cubic":[[30.,0.],[70.,100.],[100.,100.]]}));
                response.changed = true;
                response.finished = true;
            }
        }
        let time = host
            .state()
            .navigation
            .last()
            .map(|v| v.time)
            .unwrap_or_else(|| {
                fold_foundation::Time::new(
                    i64::from(host.state().frame) * i64::from(host.state().rate[1]),
                    host.state().rate[0],
                )
                .unwrap()
            });
        let action = if let Some(group) = assign_group {
            Some(crate::authoring::assign_group(&mut motion, selected, group).map(|_| selected))
        } else if let Some(key) = animate {
            Some(crate::authoring::keyframe_input(
                &mut motion,
                selected,
                &key,
                time,
            ))
        } else {
            publish.map(|key| {
                if let Some(group) = state.group {
                    crate::authoring::scene::expose_group_input(&mut motion, group, selected, &key)
                } else {
                    crate::authoring::expose_input(&mut motion, selected, &key)
                }
            })
        };
        if let Some(result) = action {
            match result {
                Ok(id) => {
                    focus = Some(id);
                    response.changed = true;
                    response.finished = true;
                }
                Err(error) => state.error = error,
            }
        }
        if let Some(id) = focus {
            state.selected = vec![id];
            state.generation += 1;
        }
        response.cancel_on_escape(ui, state.editing && state.property_editing);
        if response.cancelled {
            state.cancel(host);
        } else {
            if response.changed {
                state.property_editing = true;
                state.replace_view(motion);
                state.preview(host);
            }
            // Closing a popup can remove its numeric control before it reports
            // deactivation. Finish that preview once no control owns the edit.
            if state.property_editing
                && state.editing
                && (response.finished || !ui.is_any_item_active())
            {
                state.commit(host);
            }
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}
pub(super) fn property_label(key: &str) -> String {
    if key == "domain" {
        return "Scope".into();
    }
    key.split('_')
        .map(|word| {
            let mut chars = word.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().collect::<String>() + chars.as_str())
                .unwrap_or_default()
        })
        .collect::<Vec<_>>()
        .join(" ")
}
fn alignment(ui: &Ui, value: &mut Datum, aces: bool, response: &mut Response) {
    let Datum::Scalar(v) = value else {
        datum(ui, value, aces, response);
        return;
    };
    let name = if *v == 0. {
        "Left"
    } else if *v == 0.5 {
        "Center"
    } else if *v == 1. {
        "Right"
    } else {
        "Custom"
    };
    if let Some(_combo) = ui.begin_combo("##alignment", name) {
        for (label, next) in [("Left", 0.), ("Center", 0.5), ("Right", 1.)] {
            if ui.selectable(label) && *v != next {
                *v = next;
                response.changed = true;
                response.finished = true;
            }
        }
        ui.separator();
        NumericProperty {
            id: "custom",
            label: "Custom",
            unit: "",
            speed: 0.01,
            range: None,
            default: None,
        }
        .draw(ui, v, response);
        tooltip(
            ui,
            "Continuous alignment: 0 is left, 0.5 is centered, 1 is right",
        );
    }
}
pub(super) fn datum(ui: &Ui, value: &mut Datum, aces: bool, response: &mut Response) {
    match value {
        Datum::Scalar(v) => {
            NumericProperty {
                id: "value",
                label: "",
                unit: "",
                speed: 0.1,
                range: None,
                default: None,
            }
            .draw(ui, v, response);
        }
        Datum::Vector(v) => {
            for (i, label) in ["X", "Y"].iter().enumerate() {
                NumericProperty {
                    id: ["x", "y"][i],
                    label,
                    unit: "",
                    speed: 0.1,
                    range: None,
                    default: None,
                }
                .draw(ui, &mut v[i], response);
            }
        }
        Datum::Color(v) => {
            fold_ui::sdk::color_controls::authored(ui, v, aces, response);
        }
        Datum::Bool(v) => {
            let changed = ui.checkbox("##value", v);
            response.item(ui, changed);
        }
        Datum::Text(v) => {
            let changed = ui.input_text("##text", v).build();
            response.item(ui, changed);
        }
        Datum::Id(v) => ui.text_disabled(format!("Stable identity: {v}")),
    }
}
fn setting_label(value: &str) -> &str {
    match value {
        "Points" => "Each path point",
        "Instances" => "Each copy",
        "Glyphs" => "Each glyph",
        "Children" => "Each child",
        _ => value,
    }
}
fn settings(ui: &Ui, value: &mut serde_json::Value, response: &mut Response, depth: usize) {
    if depth > 8 {
        return;
    }
    match value {
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if key == "id" || key == "value_type" {
                    continue;
                }
                let _id = ui.push_id(key.as_str());
                ui.text(property_label(key));
                if key == "combine" {
                    if let Some(text) = value.as_str()
                        && let Some(_combo) = ui.begin_combo("Mode", text)
                    {
                        for choice in ["Replace", "Add", "Multiply"] {
                            if ui.selectable(choice) {
                                *value = serde_json::Value::String(choice.into());
                                response.changed = true;
                                response.finished = true;
                            }
                        }
                    }
                } else {
                    settings(ui, value, response, depth + 1);
                }
            }
        }
        serde_json::Value::Array(values) => {
            for (i, value) in values.iter_mut().enumerate() {
                let _id = ui.push_id(i as i32);
                settings(ui, value, response, depth + 1);
            }
        }
        serde_json::Value::Number(number) => {
            let mut v = number.as_f64().unwrap_or_default();
            let changed = Drag::new("##number").speed(0.1).build(ui, &mut v);
            if changed && let Some(number) = serde_json::Number::from_f64(v) {
                *value = serde_json::Value::Number(number);
            }
            response.item(ui, changed);
        }
        serde_json::Value::String(text) => {
            let choices: &[&str] = if [
                "Add", "Subtract", "Multiply", "Divide", "Min", "Max", "Absolute", "Negate",
            ]
            .contains(&text.as_str())
            {
                &[
                    "Add", "Subtract", "Multiply", "Divide", "Min", "Max", "Absolute", "Negate",
                ]
            } else if ["Replace"].contains(&text.as_str()) {
                &["Replace", "Add", "Multiply"]
            } else if ["Sine", "Triangle", "Square"].contains(&text.as_str()) {
                &["Sine", "Triangle", "Square"]
            } else if ["Points", "Instances", "Glyphs", "Children"].contains(&text.as_str()) {
                &["Points", "Instances", "Glyphs", "Children"]
            } else {
                &[]
            };
            if choices.is_empty() {
                let changed = ui.input_text("##text", text).build();
                response.item(ui, changed);
            } else if let Some(_combo) = ui.begin_combo("##choice", setting_label(text)) {
                for choice in choices {
                    if ui.selectable(setting_label(choice)) {
                        *text = (*choice).into();
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
        }
        serde_json::Value::Bool(v) => {
            let changed = ui.checkbox("##bool", v);
            response.item(ui, changed);
        }
        serde_json::Value::Null => ui.text_disabled("Not assigned"),
    }
}
