use super::state::Shared;
use crate::{Parameters as P, Source, package};
use fold_foundation::Time;
use fold_platform::desktop::{DesktopCommand, ViewLocation};
use fold_ui::sdk::{
    EditResponse as Response, ExtensionUi, Panel, UiId,
    imgui::{Drag, Ui},
};
pub(super) struct Inspector(pub Shared);
impl Panel for Inspector {
    fn id(&self) -> &'static str {
        package::INSPECTOR
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::COMPOSITE)
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
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
        let Some(node) = state.graph.as_mut().and_then(|g| g.node_mut(id).ok()) else {
            return;
        };
        ui.text(node.parameters.operator().id);
        ui.separator();
        let mut response = Response::default();
        let mut navigate = None;
        let properties = node.parameters.animated_properties();
        for mut property in properties {
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
                &mut response,
            );
            if response.changed {
                let _ = node.parameters.set_property(property.id, &property.values);
            }
        }
        match &mut node.parameters {
            P::Read { source, start, source_start, duration } => {
                rational(ui,"Start",start,&mut response); rational(ui,"Source in",source_start,&mut response); rational(ui,"Duration",duration,&mut response);
                ui.separator();
                match source {
                    Source::Document { source, .. } => {
                        ui.text("Nested source");
                        let time = host.state().navigation.last().map(|v| v.time).unwrap_or_else(|| Time::new(i64::from(host.state().frame)*i64::from(host.state().rate[1]),host.state().rate[0]).unwrap());
                        let local = if time >= *start && start.checked_add(*duration).is_ok_and(|end| time < end) { time.checked_sub(*start).and_then(|t| t.checked_add(*source_start)).ok() } else { None };
                        if let Some(local) = local {
                            ui.text(format!("Source time: {}/{} s", local.numerator(),local.denominator()));
                            if ui.button("Open Source") { navigate = Some(ViewLocation { document: source.document, time: local, label: "Source composite".into() }); }
                        } else { ui.text_disabled("Read is inactive at this time"); }
                    }
                    Source::Asset { asset, info } => {
                        if aces {
                            let assignment = (|| {
                                if let Some(space) = fold_platform::color::input(&node.extensions)? { return Ok(Some(space)); }
                                let snapshot = host.snapshot().ok_or("Missing project")?;
                                let asset = snapshot.state().assets.get(asset).ok_or("Missing input asset")?;
                                fold_platform::color::input(&asset.extensions)
                            })();
                            let mut space = match assignment {
                                Ok(space) => space.unwrap_or_else(|| fold_platform::color::VIDEO_INPUT.into()),
                                Err(error) => { ui.text_wrapped(error); "Invalid input".into() }
                            };
                            if fold_ui::sdk::color_controls::input(ui, &choices, &mut space) {
                                node.extensions.insert(fold_platform::color::INPUT_KEY.into(), serde_json::Value::String(space));
                                response.changed = true; response.finished = true;
                            }
                        } else { ui.text_disabled("Legacy linear-sRGB"); }
                        ui.text(format!("{} x {} • {}/{} fps",info.width,info.height,info.rate[0],info.rate[1]));
                        if let Some(snapshot) = host.snapshot() && let Some(asset) = snapshot.state().assets.get(asset) { ui.text_wrapped(&asset.location); }
                    }
                }
            }
            P::Solid { .. } | P::Transform { .. } | P::Crop { .. } | P::Mask { .. } | P::Blur { .. } | P::Grade { .. } => {},
            P::Merge => ui.text_wrapped("Foreground over background, using premultiplied alpha in scene-linear light."),
            P::ApplyMask => ui.text_wrapped("Multiply image RGBA by the connected mask. Image and mask sockets are different types."),
            P::Output => {
                if aces {
                    if let (Some(snapshot), Some(document)) = (host.snapshot(), document) {
                        match fold_platform::color::output(&snapshot, document) {
                            Ok(mut transform) => if fold_ui::sdk::color_controls::output(ui, &choices, &mut transform) { output_change = Some(transform); },
                            Err(error) => ui.text_wrapped(error),
                        }
                    }
                } else { ui.text_disabled("Legacy linear-sRGB"); }
            },
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
fn rational(ui: &Ui, label: &str, value: &mut Time, response: &mut Response) {
    let mut numerator = value.numerator();
    let mut denominator = value.denominator();
    let changed = Drag::new(format!("{label} numerator"))
        .speed(0.1)
        .build(ui, &mut numerator);
    response.item(ui, changed);
    let changed_denominator = Drag::new(format!("{label} denominator"))
        .speed(0.1)
        .range(1, u32::MAX)
        .build(ui, &mut denominator);
    response.item(ui, changed_denominator);
    if (changed || changed_denominator)
        && let Ok(time) = Time::new(numerator, denominator)
    {
        *value = time;
    }
}
