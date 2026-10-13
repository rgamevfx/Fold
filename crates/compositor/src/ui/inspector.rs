use super::state::Shared;
use crate::{OperationConfig as P, package};
use fold_foundation::Time;
use fold_platform::desktop::DesktopCommand;
use fold_ui::sdk::{EditResponse as Response, ExtensionUi, Panel, UiId, imgui::Ui};
pub(super) struct Inspector(
    pub Shared,
    pub super::channels::ShuffleDraft,
    pub super::shader::Draft,
    pub std::collections::BTreeSet<(fold_foundation::ObjectId, String)>,
    pub super::references::Drafts,
);
impl Panel for Inspector {
    fn id(&self) -> &'static str {
        package::INSPECTOR
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::COMPOSITE)
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        // Reserve space for the longest primary routing label at narrow widths.
        let _width = ui.push_item_width(
            (ui.content_region_avail()[0]
                - ui.calc_text_size("Destination layer")[0]
                - ui.clone_style().item_spacing()[0])
                .max(60.),
        );
        let aces = host
            .snapshot()
            .is_some_and(|s| fold_platform::color::project(&s).ok().flatten().is_some());
        let choices = host.state().color_choices.clone();
        let mut state = self.0.borrow_mut();
        state.sync(host);
        if state.shader_pending.is_some() {
            ui.text_wrapped("Validating shader draft… The saved source remains unchanged until validation succeeds.");
            if ui.button("Cancel shader draft")
                || ui.is_key_pressed(fold_ui::sdk::imgui::Key::Escape)
            {
                state.cancel(host);
            }
            return;
        }
        let Some(id) = state.selected.first().copied() else {
            ui.text_wrapped("Select a node to edit its properties.");
            return;
        };
        if state.selected.len() > 1 {
            ui.text(format!(
                "{} nodes selected • editing first",
                state.selected.len()
            ));
        }
        let _scope = state.document.map(|document| {
            UiId {
                package: crate::PACKAGE,
                panel: package::INSPECTOR,
                document,
                object: id,
            }
            .scope(ui)
        });
        self.1.select(state.document.map(|document| (document, id)));
        let document = state.document;
        let auto_key = host.state().auto_key;
        let time = host
            .state()
            .navigation
            .last()
            .map(|v| v.time)
            .unwrap_or(Time::ZERO);
        if state
            .graph
            .as_ref()
            .and_then(|g| g.node(id).ok())
            .is_some_and(|n| n.operation.operation == "Roto")
        {
            super::roto::draw_inspector(ui, &mut state, host, id, time, auto_key);
            return;
        }
        if state
            .graph
            .as_ref()
            .and_then(|g| g.node(id).ok())
            .is_some_and(|n| n.operation.operation == "Viewer")
        {
            ui.text("Viewer");
            return;
        }
        let mut output_change = None;
        let mut write_actions = (false, false);
        let names = |slot: usize| {
            state
                .graph
                .as_ref()
                .and_then(|g| {
                    g.node(id)
                        .ok()?
                        .input_ids()
                        .get(slot)
                        .copied()
                        .flatten()
                        .and_then(|id| {
                            g.channel_names_with(id, &|source| host.channels(source))
                                .ok()
                        })
                })
                .unwrap_or_default()
        };
        let input_a = names(0);
        let input_b = names(1);
        let mask_channels = state
            .graph
            .as_ref()
            .and_then(|g| {
                g.node(id).ok()?.effect.mask_node().and_then(|id| {
                    g.channel_names_with(id, &|source| host.channels(source))
                        .ok()
                })
            })
            .unwrap_or_default();
        let publishable = state
            .graph
            .as_ref()
            .is_some_and(|g| g.extensions.contains_key(crate::reusable::KEY));
        let mut promotion = None;
        let reference_graph = state.graph.clone();
        let reference_prepared = reference_graph.as_ref().and_then(|g| g.prepare().ok());
        let mut references = reference_prepared
            .as_ref()
            .zip(document)
            .map(|(graph, document)| super::references::Context {
                graph,
                document,
                drafts: &mut self.4,
            });
        if matches!(
            reference_prepared
                .as_ref()
                .and_then(|g| g.node(id).ok())
                .map(|n| n.config()),
            Some(P::LensDistortion { .. })
        ) {
            ui.checkbox("Reference grid", &mut state.lens_grid);
        }
        if let Some(node) = state.graph.as_ref().and_then(|g| g.node(id).ok()) {
            if node.operation.operation == "Color Shift" {
                let can_pick = !node
                    .operation
                    .parameters
                    .get("settings")
                    .is_some_and(|p| p.protected)
                    && !(0..3).any(|i| node.animation.driven(&format!("selected.{i}")));
                state.qualifier.borrow_mut().controls(ui, id, can_pick);
            }
        }
        let Some(node) = state.graph.as_mut().and_then(|g| g.node_mut(id).ok()) else {
            return;
        };
        let config = match node.prepare() {
            Ok(prepared) => prepared.config().clone(),
            Err(error) => {
                state.error = error;
                return;
            }
        };
        ui.text(
            if matches!(
                config,
                P::Read {
                    source: crate::Source::Document { .. },
                    ..
                }
            ) {
                "Image Reference"
            } else {
                config.operator().label()
            },
        );
        ui.separator();
        let mut response = Response::default();
        let super::read::Actions {
            mut navigate,
            choose_source,
            choose_proxy,
        } = super::read::draw(ui, node, host, aces, &mut response);
        if matches!(
            config,
            P::ColorShift { .. }
                | P::ChromaticAberration { .. }
                | P::LensDistortion { .. }
                | P::FilmEmulation { .. }
                | P::Grain { .. }
                | P::Retime { .. }
        ) {
            let mut filter_config = config.clone();
            super::filters::draw(ui, &mut filter_config, &mut response);
            if filter_config != config {
                if let Err(error) = node.set_configuration(filter_config) {
                    ui.text_wrapped(error);
                }
            }
        }
        let evaluated = reference_prepared
            .as_ref()
            .and_then(|g| g.evaluated_parameters(id, time).ok());
        super::filters::retime_helpers(ui, node, time, evaluated.as_ref(), &mut response);
        if matches!(config, P::Retime { .. }) {
            if let Some(P::Retime { settings }) = &evaluated {
                ui.text_disabled(format!(
                    "Source time: {:.3} s",
                    (settings.source_frame + settings.offset) * f64::from(settings.rate[1])
                        / f64::from(settings.rate[0])
                ));
            }
            if let Some(graph) = reference_prepared.as_ref() {
                let dt = Time::new(1, 1000).unwrap();
                let speed = time
                    .checked_sub(dt)
                    .ok()
                    .zip(time.checked_add(dt).ok())
                    .and_then(|(a, b)| {
                        Some((
                            graph.evaluated_parameters(id, a).ok()?,
                            graph.evaluated_parameters(id, b).ok()?,
                        ))
                    });
                if let Some((P::Retime { settings: a }, P::Retime { settings: b })) = speed {
                    let speed = (b.source_frame + b.offset - a.source_frame - a.offset) / 0.002
                        * f64::from(a.rate[1])
                        / f64::from(a.rate[0]);
                    ui.text_disabled(format!("Speed: {speed:.3}×"));
                }
            }
        }
        let properties = match node.prepare() {
            Ok(prepared) => prepared.animated_properties(),
            Err(error) => {
                state.error = error;
                return;
            }
        };
        let curated = matches!(
            config,
            P::ColorGrade { .. } | P::TransformImage { .. } | P::Blur { .. }
        );
        if curated && !matches!(config, P::TransformImage { .. }) {
            super::channels::selection(ui, &mut node.effect, &input_a, &mut response);
        }
        let mut groups: Vec<(&str, Vec<crate::animation::Property>)> = vec![];
        for property in properties.into_iter().filter(|p| p.id != "mix") {
            let group = crate::filters::group(&config, &property.id);
            if let Some((_, items)) = groups.iter_mut().find(|(name, _)| *name == group) {
                items.push(property);
            } else {
                groups.push((group, vec![property]));
            }
        }
        for (group, properties) in groups {
            let _section = if group.is_empty() {
                None
            } else {
                let Some(section) = ui
                    .tree_node_config(group)
                    .default_open(matches!(group, "Select" | "Adjust"))
                    .push()
                else {
                    continue;
                };
                Some(section)
            };
            for property in properties {
                if property.id == "target"
                    && matches!(&config,P::ColorShift {settings} if !settings.use_target)
                {
                    continue;
                }
                let split = curated
                    && ((matches!(config, P::ColorGrade { .. }) && property.channels.len() > 1)
                        || property.id == "scale"
                        || property.id == "size");
                property_control(
                    ui,
                    node,
                    property,
                    time,
                    auto_key,
                    aces,
                    &mut response,
                    publishable,
                    &mut promotion,
                    split.then_some(&mut self.3),
                    references.as_mut(),
                );
            }
        }
        let mut shader_migration = None;
        let mut config = match node.prepare() {
            Ok(prepared) => prepared.config().clone(),
            Err(error) => {
                state.error = error;
                return;
            }
        };
        match &mut config {
            P::ColorShift { .. }
            | P::ChromaticAberration { .. }
            | P::LensDistortion { .. }
            | P::FilmEmulation { .. }
            | P::Grain { .. }
            | P::Retime { .. } => {}
            P::Shader { settings } => {
                shader_migration = self.2.draw(ui, document.map(|d| (d, id)), settings);
                let size = [
                    ui.content_region_avail()[0].max(80.),
                    ui.frame_height() * 10.,
                ];
                let changed = ui
                    .input_text_multiline("##shader-source", &mut settings.source, size)
                    .build();
                response.item(ui, changed);
            }

            P::RelativeTransform { settings } => {
                use crate::transform::SizeReference;
                if let Some(_combo) =
                    ui.begin_combo("Size reference", format!("{:?}", settings.size_reference))
                {
                    for (basis, label) in [
                        (SizeReference::Placement, "Placement"),
                        (SizeReference::Width, "Width"),
                        (SizeReference::Height, "Height"),
                        (SizeReference::ShorterSide, "Shorter side"),
                    ] {
                        if ui.selectable(label) {
                            settings.size_reference = basis;
                            response.changed = true;
                            response.finished = true;
                        }
                    }
                }
                let _disabled = ui
                    .begin_disabled_with_cond(settings.size_reference != SizeReference::Placement);
                let label = format!("{:?}", settings.fit);
                if let Some(_combo) = ui.begin_combo("Fit", &label) {
                    for fit in [
                        fold_platform::format::Fit::Fit,
                        fold_platform::format::Fit::Fill,
                        fold_platform::format::Fit::Stretch,
                        fold_platform::format::Fit::Native,
                    ] {
                        if ui.selectable(format!("{fit:?}")) {
                            settings.fit = fit;
                            response.changed = true;
                            response.finished = true;
                        }
                    }
                }
                drop(_disabled);
                ui.text("Anchor");
                for y in 0..3 {
                    for x in 0..3 {
                        if x > 0 {
                            ui.same_line();
                        }
                        let _id = ui.push_id((y * 3 + x) as i32);
                        let selected = settings.anchor == [x as f64 / 2., y as f64 / 2.];
                        if ui.small_button(if selected { "o" } else { "." }) {
                            settings.anchor = [x as f64 / 2., y as f64 / 2.];
                            response.changed = true;
                            response.finished = true;
                        }
                    }
                }
                for (i, label) in ["Source width", "Source height"].iter().enumerate() {
                    let changed = fold_ui::sdk::imgui::Drag::new(label)
                        .speed(1.)
                        .range(1., 65536.)
                        .display_format(fold_ui::sdk::imgui::NumericFormat::new("%.1f").unwrap())
                        .build(ui, &mut settings.source_size[i]);
                    response.item(ui, changed);
                }
            }

            P::Unary { .. } | P::Shape { .. } => {}
            P::Blur { edges, filter, .. } => {
                super::controls::blur(ui, filter, edges, &mut response)
            }
            P::TransformImage { settings } => {
                super::controls::transform(ui, settings, &mut response)
            }
            P::Composite { mode, channels } => {
                let choices: Vec<_> = fold_render::operations::MergeMode::ALL
                    .iter()
                    .map(|choice| (*choice, choice.label()))
                    .collect();
                super::controls::combo(ui, "Operation", mode, &choices, &mut response);
                super::channels::selector(
                    ui,
                    "A channels",
                    &mut channels.a,
                    &input_a,
                    &mut response,
                );
                super::channels::selector(
                    ui,
                    "B channels",
                    &mut channels.b,
                    &input_b,
                    &mut response,
                );
                let available: Vec<_> = input_a.iter().chain(&input_b).cloned().collect();
                super::channels::selector(
                    ui,
                    "Output",
                    &mut node.effect.channels,
                    &available,
                    &mut response,
                );
                super::controls::row(ui, "Bounds");
                ui.text_disabled("Frame");
                super::controls::check(
                    ui,
                    "Also merge other channels",
                    &mut channels.other,
                    &mut response,
                );
            }
            P::ColorGrade { settings } => super::controls::grade(ui, settings, &mut response),
            P::Shuffle { mappings } => super::channels::shuffle(
                ui,
                mappings,
                &input_b,
                &input_a,
                &mut self.1,
                &mut response,
            ),
            P::CullChannels { channels, keep } => {
                super::channels::cull(ui, channels, keep, &input_a, &mut response)
            }
            P::Input { id } => {
                ui.text(format!("Input: {id}"));
            }
            P::Instance { instance, .. } => {
                ui.text_wrapped(fold_ui::sdk::effects::name(host, &instance.definition));
                fold_ui::sdk::effects::parameters(ui, host, instance, time, &mut response);
                if ui.small_button("Open effect graph") {
                    navigate = Some(fold_platform::desktop::ViewLocation {
                        document: instance.definition.document,
                        time: Time::ZERO,
                        label: "Effect".into(),
                    });
                }

                if ui.checkbox("Enabled", &mut instance.enabled) {
                    response.changed = true;
                    response.finished = true;
                }
            }
            P::Dissolve { curve } => {
                fold_ui::sdk::normalized_curve::draw(ui, curve, None, f64::NAN, &mut response);
            }
            P::Roto { .. } | P::Read { .. } | P::Viewer => {}
            P::Solid { .. } | P::Crop { .. } | P::Mask { .. } => {}

            P::Write { settings } => {
                write_actions = super::write::draw(ui, settings, &choices, &mut response);
            }
            P::Output => {
                if aces {
                    if let (Some(snapshot), Some(document)) = (host.snapshot(), document) {
                        match fold_platform::color::output(&snapshot, document) {
                            Ok(mut transform) => {
                                if fold_ui::sdk::color_controls::output(
                                    ui,
                                    &choices,
                                    &mut transform,
                                ) {
                                    output_change = Some(transform);
                                }
                            }
                            Err(error) => ui.text_wrapped(error),
                        }
                    }
                } else {
                    ui.text_disabled("Legacy linear-sRGB");
                }
            }
        }
        if let Err(error) = node.set_configuration(config.clone()) {
            ui.text_wrapped(error);
        }
        if config.operator().supports_effect() {
            ui.separator();
            if !curated && !matches!(config, P::Composite { .. }) {
                super::channels::selection(ui, &mut node.effect, &input_a, &mut response);
            }
            super::channels::effect(ui, &mut node.effect, &mask_channels, &mut response);
            if let Some(property) = node
                .prepare()
                .ok()
                .map(|p| p.animated_properties())
                .unwrap_or_default()
                .into_iter()
                .find(|property| property.id == "mix")
            {
                property_control(
                    ui,
                    node,
                    property,
                    time,
                    auto_key,
                    aces,
                    &mut response,
                    publishable,
                    &mut promotion,
                    None,
                    references.as_mut(),
                );
            }
        }
        if let Some(graph) = state.graph.as_mut() {
            match super::reusable::draw(ui, graph) {
                Ok(true) => {
                    response.changed = true;
                    response.finished = true;
                }
                Ok(false) => {}
                Err(error) => state.error = error,
            }
        }
        if let Some((control, binding)) = promotion {
            if let Some(graph) = state.graph.as_mut() {
                match graph.reusable().and_then(|mut d| {
                    if d.bindings
                        .iter()
                        .any(|b| b.node == binding.node && b.property == binding.property)
                    {
                        return Err("This property is already promoted".into());
                    }
                    d.interface.version = d
                        .interface
                        .version
                        .checked_add(1)
                        .ok_or("Interface version overflow")?;
                    d.interface.controls.push(control);
                    d.bindings.push(binding);
                    graph.extensions.insert(
                        crate::reusable::KEY.into(),
                        serde_json::to_value(d).map_err(|e| e.to_string())?,
                    );
                    Ok(())
                }) {
                    Ok(()) => {
                        response.changed = true;
                        response.finished = true;
                    }
                    Err(e) => state.error = e,
                }
            }
        }
        if let Some(migration) = shader_migration {
            let result = state.graph.as_mut().unwrap().migrate_shader(
                id,
                migration.settings,
                &migration.renames,
                &migration.remove,
            );
            if self.2.result(result) {
                response.changed = true;
                response.finished = true;
            }
        }
        if (choose_source || choose_proxy)
            && let (Some(document), Some(snapshot)) = (document, host.snapshot())
        {
            state.cancel(host);
            host.command(DesktopCommand::Extension(
                fold_platform::packages::CommandRequest {
                    id: "fold.app.read-source".into(),
                    base: snapshot.revision(),
                    arguments: serde_json::to_vec(
                        &serde_json::json!({"document":document,"node":id,"proxy":choose_proxy}),
                    )
                    .unwrap(),
                },
            ));
            return;
        }
        if let (Some(document), Some(transform)) = (document, output_change) {
            state.cancel(host);
            host.command(DesktopCommand::SetOutputColor {
                document,
                transform,
            });
            state.sync(host);
            return;
        }
        if let Some(location) = navigate {
            state.cancel(host);
            host.command(DesktopCommand::Navigate(location));
            state.sync(host);
            return;
        }
        if let Some(selected) = self.4.selected.take() {
            state.selected = vec![selected];
        }
        response.cancel_on_escape(ui, state.editing && state.property_editing);
        if response.cancelled {
            state.cancel(host);
        } else {
            if response.changed {
                state.property_editing = true;
                state.preview(host);
            }
            if state.property_editing
                && state.editing
                && (response.finished || !ui.is_any_item_active())
            {
                state.commit(host);
            }
        }
        if write_actions.0
            && let (Some(document), Some(snapshot)) = (document, host.snapshot())
        {
            host.command(DesktopCommand::Extension(
                fold_platform::packages::CommandRequest {
                    id: "fold.app.write-destination".into(),
                    base: snapshot.revision(),
                    arguments: serde_json::to_vec(
                        &serde_json::json!({"document":document,"node":id}),
                    )
                    .unwrap(),
                },
            ));
        }
        if write_actions.1
            && let (Some(document), P::Write { settings }) = (document, config)
        {
            host.command(DesktopCommand::ExportFrames {
                source: fold_project::DocumentRef {
                    document,
                    output: format!("write/{id}"),
                    extensions: Default::default(),
                },
                settings,
            });
        }
        if !state.error.is_empty() {
            ui.separator();
            ui.text_wrapped(&state.error);
        }
    }
}

fn property_control(
    ui: &Ui,
    node: &mut crate::Node,
    mut property: crate::animation::Property,
    time: Time,
    auto_key: bool,
    aces: bool,
    response: &mut Response,
    publishable: bool,
    promotion: &mut Option<(fold_platform::effects::Control, crate::reusable::Binding)>,
    split_state: Option<&mut std::collections::BTreeSet<(fold_foundation::ObjectId, String)>>,
    mut references: Option<&mut super::references::Context<'_, '_>>,
) {
    let split_key = (node.id, property.id.clone());
    let mut split = split_state
        .as_ref()
        .is_some_and(|state| state.contains(&split_key));
    let mut edit = Response::default();
    let mut promote = false;
    let Ok(mut curves) = node.animation.project() else {
        return;
    };
    let mut action = None;
    let driven = (0..property.values.len()).any(|i| {
        node.animation
            .driven(&crate::animation::path(&property.id, i))
    });
    let property_menu = crate::animation::Property {
        id: property.id.clone(),
        label: property.label.clone(),
        unit: property.unit,
        channels: property.channels,
        values: property.values.clone(),
        range: property.range,
    };
    let mut bound = vec![None; property.values.len()];
    if let Some(context) = references.as_ref() {
        for (i, sample) in bound.iter_mut().enumerate() {
            if node
                .animation
                .driven(&crate::animation::path(&property.id, i))
            {
                *sample = Some(property.values[i]);
                match context
                    .graph
                    .node(node.id)
                    .and_then(|n| n.property_reference(&property.id, i))
                    .and_then(|r| context.graph.sample_reference(r, time))
                {
                    Ok(value) => *sample = Some(value),
                    Err(error) => ui.text_wrapped(error),
                }
            }
        }
    }
    fold_ui::sdk::animated_property::AnimatedProperty {
        id: &property.id,
        label: &property.label,
        components: property.channels,
        unit: property.unit,
        range: property.range,
        color: (matches!(property.id.as_str(), "color" | "selected" | "target") && !driven)
            .then_some(aces),
    }
    .draw_compact(
        ui,
        &mut property.values,
        &mut curves,
        time,
        auto_key,
        &mut edit,
        &mut |ui, component| {
            if let Some(context) = references.as_mut() {
                context.menu(ui, node, &property_menu, component, &mut action);
            }
            if publishable && ui.menu_item("Promote to effect") {
                promote = true;
            }
        },
        split_state.as_ref().map(|_| &mut split),
        &bound,
    );
    let mut binding_changed = false;
    if let Some(context) = references {
        if context.finish(ui, node, &property, time, action) {
            binding_changed = true;
            edit.changed = true;
            edit.finished = true;
        }
    }
    if let Some(state) = split_state {
        if split {
            state.insert(split_key);
        } else {
            state.remove(&split_key);
        }
    }
    if promote {
        let id = format!(
            "control_{}",
            serde_json::to_value(fold_foundation::ObjectId::new())
                .unwrap()
                .as_str()
                .unwrap()
                .replace('-', "")
        );
        *promotion = Some((
            fold_platform::effects::Control {
                id: id.clone(),
                label: property.label.clone(),
                default: property.values.clone(),
                range: property.range,
                extensions: Default::default(),
            },
            crate::reusable::Binding {
                extensions: Default::default(),
                control: id,
                node: node.id,
                property: property.id.clone(),
            },
        ));
    }
    if edit.changed && !binding_changed {
        let mut next = node.clone();
        if let Err(error) = next
            .animation
            .replace(&curves)
            .and_then(|_| next.set_property(&property.id, &property.values))
        {
            ui.text_wrapped(error);
            return;
        }
        *node = next;
    }
    response.began |= edit.began;
    response.changed |= edit.changed;
    response.finished |= edit.finished;
    response.cancelled |= edit.cancelled;
}
