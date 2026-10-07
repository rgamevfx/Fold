use super::state::Shared;
use crate::{Parameters as P, package};
use fold_foundation::Time;
use fold_platform::desktop::DesktopCommand;
use fold_ui::sdk::{EditResponse as Response, ExtensionUi, Panel, UiId, imgui::Ui};
pub(super) struct Inspector(
    pub Shared,
    pub super::channels::ShuffleDraft,
    pub super::shader::Draft,
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
        let mut output_change = None;
        let names = |slot: usize| {
            state
                .graph
                .as_ref()
                .and_then(|g| {
                    g.node(id)
                        .ok()?
                        .inputs
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
                g.node(id).ok()?.effect.mask.and_then(|id| {
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
        let Some(node) = state.graph.as_mut().and_then(|g| g.node_mut(id).ok()) else {
            return;
        };
        ui.text(
            if matches!(
                node.parameters,
                P::Read {
                    source: crate::Source::Document { .. },
                    ..
                }
            ) {
                "Image Reference"
            } else {
                node.parameters.operator().label()
            },
        );
        ui.separator();
        let mut response = Response::default();
        let super::read::Actions {
            mut navigate,
            choose_source,
        } = super::read::draw(ui, node, host, aces, &mut response);
        for property in node
            .animated_properties()
            .into_iter()
            .filter(|property| property.id != "mix")
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
            );
        }
        let mut shader_migration = None;
        match &mut node.parameters {
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
                if ui.is_item_hovered() {
                    ui.tooltip_text("WGSL effect(uv) returns premultiplied RGBA. Image and control functions use their published names. GPU execution required.");
                }
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
            P::GaussianBlur { edges, .. } => {
                if let Some(_advanced) = ui.tree_node("Edges") {
                    let mut clamp = *edges == fold_render::operations::Edges::Clamp;
                    if ui.checkbox("Extend edge pixels", &mut clamp) {
                        *edges = if clamp {
                            fold_render::operations::Edges::Clamp
                        } else {
                            fold_render::operations::Edges::Transparent
                        };
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
            P::TransformImage { settings } => {
                if let Some(_advanced) = ui.tree_node("Sampling") {
                    if let Some(_combo) = ui.begin_combo("Filter", settings.filter.label()) {
                        for filter in fold_render::operations::Filter::ALL {
                            if ui.selectable(filter.label()) {
                                settings.filter = filter;
                                response.changed = true;
                                response.finished = true;
                            }
                        }
                    }
                    if ui.checkbox("Invert transform", &mut settings.invert) {
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
            P::Composite { mode } => {
                if let Some(_combo) = ui.begin_combo("Operation", mode.label()) {
                    for choice in fold_render::operations::MergeMode::ALL {
                        if ui.selectable(choice.label()) {
                            *mode = choice;
                            response.changed = true;
                            response.finished = true;
                        }
                    }
                }
            }
            P::ColorGrade { settings } => {
                if let Some(_advanced) = ui.tree_node("Alpha handling")
                    && ui.checkbox("Unpremultiply before grading", &mut settings.unpremultiply)
                {
                    response.changed = true;
                    response.finished = true;
                }
            }
            P::Shuffle { mappings } => super::channels::shuffle(
                ui,
                mappings,
                &input_a,
                &input_b,
                &mut self.1,
                &mut response,
            ),
            P::RemoveChannels { channels, keep } => {
                super::channels::remove(ui, channels, keep, &input_a, &mut response)
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
            P::Read { .. } => {}
            P::Solid { .. }
            | P::Transform { .. }
            | P::Crop { .. }
            | P::Mask { .. }
            | P::Blur { .. }
            | P::Grade { .. } => {}
            P::Merge => ui.text_wrapped(
                "Foreground over background, using premultiplied alpha in scene-linear light.",
            ),
            P::ApplyMask => ui.text_wrapped("Multiply image RGBA by the connected image’s alpha."),
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
        if node.parameters.operator().supports_effect() {
            super::channels::effect(
                ui,
                &mut node.effect,
                if matches!(node.parameters, P::Merge | P::Composite { .. }) {
                    &input_b
                } else {
                    &input_a
                },
                &mask_channels,
                &mut response,
            );
            if let Some(property) = node
                .animated_properties()
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
        if choose_source && let (Some(document), Some(snapshot)) = (document, host.snapshot()) {
            state.cancel(host);
            host.command(DesktopCommand::Extension(
                fold_platform::packages::CommandRequest {
                    id: "fold.app.read-source".into(),
                    base: snapshot.revision(),
                    arguments: serde_json::to_vec(
                        &serde_json::json!({"document":document,"node":id}),
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
) {
    let mut edit = Response::default();
    let mut promote = false;
    fold_ui::sdk::animated_property::AnimatedProperty {
        id: &property.id,
        label: &property.label,
        components: property.channels,
        unit: property.unit,
        range: property.range,
        color: (property.id == "color").then_some(aces),
    }
    .draw_with_menu(
        ui,
        &mut property.values,
        &mut node.animation,
        time,
        auto_key,
        &mut edit,
        &mut |ui| {
            if publishable && ui.menu_item("Promote to effect") {
                promote = true;
            }
        },
    );
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
    if edit.changed {
        let _ = node.set_property(&property.id, &property.values);
    }
    response.began |= edit.began;
    response.changed |= edit.changed;
    response.finished |= edit.finished;
    response.cancelled |= edit.cancelled;
}
