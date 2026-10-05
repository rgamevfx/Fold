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
        super::group_interface::window(ui, host, &mut state);
        let Some(&selected) = state.selected.first().filter(|_| state.selected.len() == 1) else {
            ui.text_wrapped("Select an object, modifier or node to edit its properties.");
            return;
        };
        if let Some((driver, parent)) = state.inspector_return
            && driver == selected
            && let Some(m) = state.view_motion()
            && let Ok(node) = m.graph.node(parent)
            && ui.small_button(format!("Back to {}", super::graph::node_label(&m, node)))
        {
            state.selected = vec![parent];
            state.inspector_return = None;
            host.command(fold_platform::desktop::DesktopCommand::Select(
                fold_platform::desktop::Selection {
                    document: state.document,
                    objects: vec![parent],
                },
            ));
            return;
        }
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
            .map(|n| (n.id, super::graph::node_label(&motion, n)))
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
        let port_sections: std::collections::BTreeMap<_, _> = group_id
            .and_then(|id| motion.groups.get(&id))
            .map(|g| {
                g.inputs
                    .iter()
                    .map(|p| (p.id.clone(), (p.section.clone(), p.advanced)))
                    .collect()
            })
            .unwrap_or_default();
        let port_labels: std::collections::BTreeMap<_, _> = group_id
            .and_then(|id| motion.groups.get(&id))
            .map(|g| {
                g.inputs
                    .iter()
                    .map(|p| (p.id.clone(), p.name.clone()))
                    .collect()
            })
            .unwrap_or_default();
        let gradient_path = crate::authoring::scene::assets::reference(&motion, selected, "path");
        // Content references are edited through their actual reference nodes.
        // This works for any published group content port, not only shipped assets.
        if group_id.is_some() && state.group.is_none() {
            let ports: Vec<_> = signature
                .as_ref()
                .ok()
                .map(|s| {
                    s.0.iter()
                        .filter(|p| p.kind == crate::fields::Kind::Content && p.id != "content")
                        .map(|p| p.id.clone())
                        .collect()
                })
                .unwrap_or_default();
            for port in ports {
                let _id = ui.push_id(&port);
                let current = crate::authoring::scene::assets::reference(&motion, selected, &port);
                let label = objects
                    .iter()
                    .find(|(id, _)| Some(*id) == current)
                    .map(|(_, name)| name.as_str())
                    .unwrap_or("Choose object");
                let port_label = port_labels.get(&port).map(String::as_str).unwrap_or(&port);
                ui.set_next_item_width(
                    (ui.content_region_avail()[0] - ui.calc_text_size(port_label)[0] - 94.)
                        .max(50.),
                );
                if let Some(_combo) = ui.begin_combo(port_label, label) {
                    for (id, name) in &objects {
                        if *id != selected && ui.selectable(format!("{name}##{id:?}")) {
                            state.change_scene(host, |m| {
                                crate::authoring::scene::assets::set_reference(
                                    m,
                                    selected,
                                    &port,
                                    *id,
                                    port == "source",
                                )?;
                                Ok(Some(selected))
                            });
                            return;
                        }
                    }
                }
                ui.same_line();
                if ui.small_button("Pick") {
                    state.reference_pick = Some((selected, port.clone()));
                    state.scene_scope = None;
                    return;
                }
                if let Some(id) = current {
                    ui.same_line();
                    if ui.small_button("Edit") {
                        state.cancel(host);
                        state.selected = vec![id];
                        state.source_history.push(selected);
                        state.scene_scope = state
                            .motion
                            .as_ref()
                            .and_then(|m| m.graph.node(id).ok())
                            .filter(|n| n.kind == "fold.motion.scene_children")
                            .map(|_| id);
                        state.network_object = Some(id);
                        host.command(fold_platform::desktop::DesktopCommand::Select(
                            fold_platform::desktop::Selection {
                                document: state.document,
                                objects: vec![id],
                            },
                        ));
                        return;
                    }
                }
            }
        }
        let Some(node) = motion.graph.nodes.iter_mut().find(|n| n.id == selected) else {
            return;
        };
        ui.text(
            group_id
                .and_then(|id| motion.groups.get(&id))
                .map(|g| g.name.as_str())
                .unwrap_or_else(|| {
                    registry::find(&node.kind)
                        .map(|d| d.name)
                        .unwrap_or(&node.kind)
                }),
        );
        if let Some(group) = group_id
            && let Some(_menu) = ui.begin_popup_context_item_with_label(Some("property-actions"))
        {
            if ui.menu_item("Edit node interface") {
                state.interface_target = Some(group);
            }
        }
        ui.separator();
        if let Some(group) = group_id {
            if ui.button("Edit Graph") {
                state.cancel(host);
                let parent = state.group;
                state.parents.push(parent);
                state.group = Some(group);
                state.network_requested = true;
                state.selected.clear();
                return;
            }
            ui.same_line();
            if ui.small_button("Make unique") {
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
            tooltip(
                ui,
                "Graph edits are shared. Make unique creates an independent tool definition.",
            );
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
        let mut expose_socket = None;
        let mut add_oscillator = None;
        let mut add_color_ramp = None;
        super::color_ramp::draw(ui, node, &mut motion.groups, aces, &mut response);
        super::oscillator::draw(ui, node, &mut motion.groups, time, &mut response);
        if let Some(_menu) = icon_menu(ui, "node-controls", "Drivers and published controls") {
            if let Some(group) = group_id {
                if ui.menu_item("Edit node interface") {
                    state.interface_target = Some(group);
                }
                ui.separator();
            }
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
                        "Promote to group input"
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
            let mut section = String::new();
            let mut section_open = true;
            for socket in sockets {
                let key = socket.id;
                if node.kind == "fold.motion.math"
                    && key == "b"
                    && matches!(
                        node.settings.get("operation").and_then(|v| v.as_str()),
                        Some("Absolute" | "Negate" | "Round" | "Floor" | "Ceil" | "Truncate")
                    )
                {
                    continue;
                }
                let socket_visible = super::graph::socket_exposed(node, &key);
                if group_id.is_some()
                    && state.group.is_none()
                    && socket.kind == crate::fields::Kind::Content
                {
                    continue;
                }
                if let Some((name, advanced)) = port_sections.get(&key)
                    && *name != section
                {
                    section = name.clone();
                    section_open = name.is_empty()
                        || ui.collapsing_header(
                            name,
                            if *advanced {
                                fold_ui::sdk::imgui::TreeNodeFlags::empty()
                            } else {
                                fold_ui::sdk::imgui::TreeNodeFlags::DEFAULT_OPEN
                            },
                        );
                }
                if !section_open {
                    continue;
                }
                if super::oscillator::is_oscillator(node) {
                    let bpm = matches!(node.inputs.get("time_mode"), Some(Input::Value(Datum::Text(v))) if v == "BPM");
                    if (key == "duration" && bpm) || (key == "bpm" && !bpm) {
                        continue;
                    }
                    if matches!(key.as_str(), "shape" | "time_mode")
                        && let Some(Input::Value(Datum::Text(value))) = node.inputs.get_mut(&key)
                    {
                        let choices = if key == "shape" {
                            crate::nodes::oscillator::SHAPES
                        } else {
                            &["Seconds", "BPM"]
                        };
                        let label = if key == "shape" { "Shape" } else { "Timing" };
                        if let Some(_combo) = ui.begin_combo(label, value.as_str()) {
                            for &choice in choices {
                                if ui.selectable(choice) {
                                    *value = choice.into();
                                    response.changed = true;
                                    response.finished = true;
                                }
                            }
                        }
                        continue;
                    }
                }

                if !node.inputs.contains_key(&key)
                    && let Some(default) = &socket.default
                {
                    node.inputs
                        .insert(key.clone(), Input::Value(default.clone()));
                }
                let _id = ui.push_id(&key);
                let mut property_menu = |ui: &Ui| {
                    ui.separator();
                    if matches!(
                        socket.kind,
                        crate::fields::Kind::Scalar
                            | crate::fields::Kind::Vector
                            | crate::fields::Kind::Color
                    ) && let Some(_menu) = ui.begin_menu("Add Driver")
                    {
                        if ui.menu_item("Oscillator") {
                            add_oscillator = Some(key.clone());
                        }
                        if socket.kind == crate::fields::Kind::Color && ui.menu_item("Color Ramp") {
                            add_color_ramp = Some(key.clone());
                        }
                    }
                    if ui.menu_item(if socket_visible {
                        "Hide input socket"
                    } else {
                        "Expose input"
                    }) {
                        expose_socket = Some((key.clone(), !socket_visible));
                    }
                    if state.group.is_some() && ui.menu_item("Promote to group input") {
                        publish = Some(key.clone());
                    }
                };
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
                    .draw_with_menu(
                        ui,
                        &mut values,
                        &mut node.animation,
                        time,
                        state.auto_key,
                        &mut response,
                        &mut property_menu,
                    );
                    crate::animation::parameters::set_components(value, &values);
                    continue;
                }
                if let Some(Input::Value(Datum::Bool(value))) = node.inputs.get_mut(&key) {
                    let changed = ui.checkbox(
                        port_labels
                            .get(&key)
                            .cloned()
                            .unwrap_or_else(|| property_label(&key)),
                        value,
                    );
                    response.item(ui, changed);
                    if let Some(_menu) =
                        ui.begin_popup_context_item_with_label(Some("property-actions"))
                    {
                        property_menu(ui);
                    }
                    continue;
                }
                ui.text(
                    port_labels
                        .get(&key)
                        .cloned()
                        .unwrap_or_else(|| property_label(&key)),
                );
                if !socket.unit.is_empty() && key != "alignment" {
                    ui.same_line();
                    ui.text_disabled(&socket.unit);
                }
                match node.inputs.get_mut(&key) {
                    Some(Input::Link(link)) => {
                        if ui.small_button(format!(
                            "Edit {}",
                            names
                                .get(&link.node)
                                .map(String::as_str)
                                .unwrap_or("driver")
                        )) {
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
                        if let Some(_menu) =
                            ui.begin_popup_context_item_with_label(Some("property-actions"))
                        {
                            property_menu(ui);
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
            if let Some(_menu) = icon_menu(ui, "definition", "Replace tool definition") {
                if let Some(_combo) = ui.begin_combo("Definition", "Choose reusable group") {
                    for (id, name) in groups {
                        if ui.selectable(name) {
                            assign_group = Some(id);
                        }
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
            let local = node
                .settings
                .get("local")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let world = node
                .settings
                .get("world")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let owner = node
                .settings
                .get("owner_space")
                .and_then(|v| v.as_bool())
                .unwrap_or(false);
            let label = if local {
                "Source local"
            } else if world && owner {
                "Relative to owner"
            } else if world {
                "Scene world"
            } else {
                "Source parent"
            };
            if let Some(_combo) = ui.begin_combo("Coordinates", label) {
                for (label, local, world, owner) in [
                    ("Source local", true, false, false),
                    ("Source parent", false, false, false),
                    ("Scene world", false, true, false),
                    ("Relative to owner", false, true, true),
                ] {
                    if ui.selectable(label) {
                        node.settings["local"] = local.into();
                        node.settings["world"] = world.into();
                        node.settings["owner_space"] = owner.into();
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
        } else if node.kind == "fold.motion.path" {
            if ui.small_button("Edit points in viewer") {
                state.point_edit_requested = true;
            }
            super::gradient::draw(ui, &mut node.settings, aces, &mut response);
            super::path_attributes::draw(
                ui,
                &mut node.settings,
                aces,
                &mut response,
                node.id,
                &mut state.path_attribute_names,
            );
            if ui.collapsing_header("Geometry data", fold_ui::sdk::imgui::TreeNodeFlags::empty())
                && let Some(segments) = node.settings.get_mut("segments")
            {
                settings(ui, segments, &mut response, 0);
            }
        } else if node.kind == "fold.motion.keyframes" {
            ui.text_disabled("Edit keys in the Animation panel.");
        } else if !matches!(
            node.kind.as_str(),
            "fold.motion.oscillator" | "fold.motion.color_ramp"
        ) {
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
        if let Some(path) = gradient_path
            && let Ok(path) = crate::authoring::node(&mut motion, path)
        {
            if ui.collapsing_header(
                "Path gradient · shared source",
                fold_ui::sdk::imgui::TreeNodeFlags::DEFAULT_OPEN,
            ) {
                super::gradient::draw(ui, &mut path.settings, aces, &mut response);
            }
        }
        if let Some((key, visible)) = expose_socket {
            let target = crate::authoring::node(&mut motion, selected).unwrap();
            let visibility = target
                .extensions
                .entry("fold.motion.input_visibility".into())
                .or_insert_with(|| serde_json::json!({}));
            if !visibility.is_object() {
                *visibility = serde_json::json!({});
            }
            visibility[&key] = serde_json::json!(visible);
            response.changed = true;
            response.finished = true;
        }
        let action = if let Some(group) = assign_group {
            Some(crate::authoring::assign_group(&mut motion, selected, group).map(|_| selected))
        } else if let Some(key) = add_color_ramp {
            Some(crate::authoring::scene::assets::attach_color_ramp(
                &mut motion,
                selected,
                &key,
            ))
        } else if let Some(key) = add_oscillator {
            Some(crate::authoring::scene::assets::attach_oscillator(
                &mut motion,
                selected,
                &key,
            ))
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
            if state.group.is_none()
                && let Some(owner) = motion
                    .scene
                    .as_ref()
                    .and_then(|s| s.objects.iter().find(|o| o.owns(selected)))
            {
                state.network_object = Some(owner.id);
            }
            state.inspector_return = Some((id, selected));
            host.command(fold_platform::desktop::DesktopCommand::Select(
                fold_platform::desktop::Selection {
                    document: state.document,
                    objects: vec![id],
                },
            ));
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
                "Round", "Floor", "Ceil", "Truncate",
            ]
            .contains(&text.as_str())
            {
                &[
                    "Add", "Subtract", "Multiply", "Divide", "Min", "Max", "Absolute", "Negate",
                    "Round", "Floor", "Ceil", "Truncate",
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
