use super::state::Shared;
use crate::{Parameters as P, package};
use fold_foundation::Time;
use fold_platform::desktop::DesktopCommand;
use fold_ui::sdk::{EditResponse as Response, ExtensionUi, Panel, UiId, imgui::Ui};
pub(super) struct Inspector(pub Shared, pub super::channels::ShuffleDraft);
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
        let document = state.document;
        let auto_key = state.auto_key;
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
        let Some(node) = state.graph.as_mut().and_then(|g| g.node_mut(id).ok()) else {
            return;
        };
        ui.text(node.parameters.operator().label());
        ui.separator();
        let mut response = Response::default();
        let super::read::Actions {
            navigate,
            choose_source,
        } = super::read::draw(ui, node, host, aces, &mut response);
        for property in node
            .animated_properties()
            .into_iter()
            .filter(|property| property.id != "mix")
        {
            property_control(ui, node, property, time, auto_key, aces, &mut response);
        }
        match &mut node.parameters {
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
                property_control(ui, node, property, time, auto_key, aces, &mut response);
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
) {
    let mut edit = Response::default();
    fold_ui::sdk::animated_property::AnimatedProperty {
        id: property.id,
        label: property.label,
        components: property.channels,
        unit: property.unit,
        range: property.range,
        color: (property.id == "color").then_some(aces),
    }
    .draw(
        ui,
        &mut property.values,
        &mut node.animation,
        time,
        auto_key,
        &mut edit,
    );
    if edit.changed {
        let _ = node.set_property(property.id, &property.values);
    }
    response.began |= edit.began;
    response.changed |= edit.changed;
    response.finished |= edit.finished;
    response.cancelled |= edit.cancelled;
}
