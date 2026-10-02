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
    pub curves: super::curves::Curves,
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
            ui.text_wrapped("Select a node to edit its properties.");
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
        let Some(mut motion) = state.view_motion() else {
            return;
        };
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
        let info = motion.info.clone();
        let Some(node) = motion.graph.nodes.iter_mut().find(|n| n.id == selected) else {
            return;
        };
        ui.text(
            registry::find(&node.kind)
                .map(|d| d.name)
                .unwrap_or(&node.kind),
        );
        ui.separator();
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
        let mut animate = None;
        let mut publish = None;
        let mut focus = None;
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
                let _id = ui.push_id(&key);
                ui.text(property_label(&key));
                let animatable = matches!(
                    node.inputs.get(&key),
                    Some(Input::Value(
                        Datum::Scalar(_) | Datum::Vector(_) | Datum::Color(_)
                    ))
                );
                if animatable {
                    ui.same_line();
                    if let Some(_menu) =
                        icon_menu(ui, "property-actions", "Animation and published controls")
                    {
                        if ui.menu_item("Animate") {
                            animate = Some(key.clone());
                        }
                        if ui.menu_item("Publish control") {
                            publish = Some(key.clone());
                        }
                    }
                }
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
                        node.settings["group"] = serde_json::to_value(id).unwrap();
                        node.inputs.clear();
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
        } else if node.kind == "fold.motion.keyframes" {
            if let Ok(mut track) = node.settings::<crate::animation::Track>() {
                let (changed, finished) = self.curves.draw(ui, node.id, &mut track, &info);
                response.changed |= changed;
                response.finished |= finished;
                let time = fold_foundation::Time::new(
                    i64::from(host.state().frame) * i64::from(host.state().rate[1]),
                    host.state().rate[0],
                )
                .unwrap();
                if ui.button("Add key at playhead") {
                    if !track.keys.iter().any(|k| k.time == time) {
                        let value = track.sample(time).unwrap_or(Datum::Scalar(0.));
                        track.keys.push(crate::animation::Key {
                            time,
                            value,
                            interpolation: crate::animation::Interpolation::Linear,
                        });
                        track.keys.sort_by_key(|k| k.time);
                        node.settings = serde_json::to_value(&track).unwrap();
                        response.changed = true;
                        response.finished = true;
                    }
                }
                let mut remove = None;
                for (i, key) in track.keys.iter_mut().enumerate() {
                    let _id = ui.push_id(i as i32);
                    ui.separator();
                    ui.text(format!("Key {}", i + 1));
                    let mut numerator = key.time.numerator();
                    let mut denominator = key.time.denominator();
                    let changed = Drag::new("Time numerator")
                        .speed(1.)
                        .build(ui, &mut numerator);
                    response.item(ui, changed);
                    let changed_d = Drag::new("Time denominator")
                        .range(1, u32::MAX)
                        .build(ui, &mut denominator);
                    response.item(ui, changed_d);
                    if (changed || changed_d)
                        && let Ok(t) = fold_foundation::Time::new(numerator, denominator)
                    {
                        key.time = t;
                    }
                    datum(ui, &mut key.value, aces, &mut response);
                    if let Some(_combo) =
                        ui.begin_combo("Interpolation", format!("{:?}", key.interpolation))
                    {
                        for (name, value) in [
                            ("Hold", crate::animation::Interpolation::Hold),
                            ("Linear", crate::animation::Interpolation::Linear),
                            ("Ease", crate::animation::Interpolation::Ease),
                        ] {
                            if ui.selectable(name) {
                                key.interpolation = value;
                                response.changed = true;
                                response.finished = true;
                            }
                        }
                    }
                    if ui.small_button("Delete key") {
                        remove = Some(i);
                    }
                }
                if let Some(i) = remove
                    && track.keys.len() > 1
                {
                    track.keys.remove(i);
                    response.changed = true;
                    response.finished = true;
                }
                if response.changed {
                    track.keys.sort_by_key(|k| k.time);
                    node.settings = serde_json::to_value(track).unwrap();
                }
            }
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
        let action = if let Some(key) = animate {
            Some(crate::authoring::keyframe_input(
                &mut motion,
                selected,
                &key,
                time,
            ))
        } else {
            publish.map(|key| crate::authoring::expose_input(&mut motion, selected, &key))
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
        response.cancel_on_escape(ui, state.editing);
        if response.cancelled {
            state.cancel(host);
        } else {
            if response.changed {
                state.replace_view(motion);
                state.preview(host);
            }
            // Closing a popup can remove its numeric control before it reports
            // deactivation. Finish that preview once no control owns the edit.
            if state.editing && (response.finished || !ui.is_any_item_active()) {
                state.commit(host);
            }
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}
fn property_label(key: &str) -> String {
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
fn datum(ui: &Ui, value: &mut Datum, aces: bool, response: &mut Response) {
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
