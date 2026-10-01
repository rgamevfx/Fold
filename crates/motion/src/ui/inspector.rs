use super::state::Shared;
use crate::{
    fields::Datum,
    graph::{Input, registry},
    package,
};
use fold_ui::sdk::{
    EditResponse as Response, ExtensionUi, NumericProperty, Panel, UiId,
    imgui::{Drag, Ui},
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
        let mut state = self.state.borrow_mut();
        state.sync(host);
        let Some(&selected) = state.selected.first().filter(|_| state.selected.len() == 1) else {
            ui.text_wrapped(
                "Select a motion node. Inspector controls edit the same inputs as graph wiring.",
            );
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
        let info = motion.info.clone();
        let Some(node) = motion.graph.nodes.iter_mut().find(|n| n.id == selected) else {
            return;
        };
        ui.text(
            registry::find(&node.kind)
                .map(|d| d.name)
                .unwrap_or(&node.kind),
        );
        ui.text_disabled(format!("{selected:?}"));
        ui.separator();
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
                ui.text(format!("{}  {}", key, socket.unit));
                match node.inputs.get_mut(&key) {
                    Some(Input::Link(link)) => {
                        ui.text_disabled(format!("Driven: {:?} / {}", link.node, link.socket));
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
                        datum(ui, value, &mut response);
                        if matches!(value, Datum::Scalar(_) | Datum::Vector(_) | Datum::Color(_)) {
                            if ui.small_button("Animate input") {
                                animate = Some(key.clone());
                            }
                            ui.same_line();
                            if ui.small_button("Publish control") {
                                publish = Some(key.clone());
                            }
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
        ui.separator();
        ui.text("Node settings");
        if node.kind == "fold.motion.text" {
            if let Some(_tree) = ui.tree_node("Bundled font license") {
                ui.text_wrapped(crate::document::FONT_LICENSE);
            }
            if let Some(_combo) = ui.begin_combo("Font", "Embedded font resource") {
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
                    datum(ui, &mut key.value, &mut response);
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
            if response.finished && state.editing {
                state.commit(host);
            }
        }
        ui.separator();
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
        ui.text_wrapped(&host.state().status);
    }
}
fn datum(ui: &Ui, value: &mut Datum, response: &mut Response) {
    match value {
        Datum::Scalar(v) => {
            NumericProperty {
                id: "value",
                label: "Value",
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
            for (i, label) in ["Red", "Green", "Blue", "Alpha"].iter().enumerate() {
                NumericProperty {
                    id: ["r", "g", "b", "a"][i],
                    label,
                    unit: "",
                    speed: 0.005,
                    range: Some([0., 1.]),
                    default: None,
                }
                .draw(ui, &mut v[i], response);
            }
        }
        Datum::Bool(v) => {
            let changed = ui.checkbox("Value", v);
            response.item(ui, changed);
        }
        Datum::Text(v) => {
            let changed = ui.input_text("Text", v).build();
            response.item(ui, changed);
        }
        Datum::Id(v) => ui.text_disabled(format!("Stable identity: {v}")),
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
                ui.text(key);
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
            } else if let Some(_combo) = ui.begin_combo("##choice", text.as_str()) {
                for choice in choices {
                    if ui.selectable(choice) {
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
