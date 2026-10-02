use super::state::Shared;
use crate::{Parameters as P, Source, package};
use fold_foundation::Time;
use fold_platform::desktop::{DesktopCommand, ViewLocation};
use fold_ui::sdk::{
    EditResponse as Response, ExtensionUi, NumericProperty, Panel, UiId,
    imgui::{Drag, Ui},
    toolbar::tooltip,
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
        let mut output_change = None;
        let Some(node) = state.graph.as_mut().and_then(|g| g.node_mut(id).ok()) else {
            return;
        };
        ui.text(node.parameters.operator().id);
        ui.separator();
        let mut response = Response::default();
        let mut navigate = None;
        match &mut node.parameters {
            P::Solid { rgba } => {
                let mut straight = rgba.map(f64::from);
                if straight[3] > 0. { for c in 0..3 { straight[c] /= straight[3]; } }
                fold_ui::sdk::color_controls::authored(ui, &mut straight, aces, &mut response);
                if response.changed { *rgba = [straight[0] * straight[3], straight[1] * straight[3], straight[2] * straight[3], straight[3]].map(|v| v as f32); }
            }
            P::Transform { translate, scale, opacity } => {
                for (i,label) in ["Position X", "Position Y"].iter().enumerate() {
                    NumericProperty { id: ["translation.x", "translation.y"][i], label, unit: "px", speed: 1.0, range: None, default: Some(0.) }.draw(ui, &mut translate[i], &mut response);
                }
                ui.separator();
                for (i,label) in ["Scale X", "Scale Y"].iter().enumerate() {
                    NumericProperty { id: ["scale.x", "scale.y"][i], label, unit: "", speed: 0.01, range: None, default: Some(1.) }.draw(ui, &mut scale[i], &mut response);
                }
                let changed = Drag::new("Opacity").speed(0.005).range(0.0,1.0).build(ui,opacity); response.item(ui,changed);
            }
            P::Crop { rect } | P::Mask { rect } => {
                for (i,label) in ["Left", "Top", "Right", "Bottom"].iter().enumerate() { let changed = Drag::new(label).speed(1.0).range(0,16384).build(ui,&mut rect[i]); response.item(ui,changed); }
                tooltip(ui, "Bounds in document pixels; right and bottom edges are exclusive. Outside is transparent.");
            }
            P::Blur { radius } => { let changed = Drag::new("Radius (pixels)").speed(0.1).range(0,64).build(ui,radius); response.item(ui,changed); tooltip(ui, "Box filter in linear light. Transparent borders expand the image support."); }
            P::Grade { gain } => { for (i,label) in ["Red gain", "Green gain", "Blue gain"].iter().enumerate() { let changed = Drag::new(label).speed(0.01).range(0.0,16.0).build(ui,&mut gain[i]); response.item(ui,changed); } tooltip(ui, "Scene-linear RGB gain."); }
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
        response.cancel_on_escape(ui, state.editing);
        if response.cancelled {
            state.cancel(host);
        } else {
            if response.changed {
                state.preview(host);
            }
            if response.finished && state.editing {
                state.commit(host);
            }
        }
        ui.separator();
        if state.editing {
            ui.text_colored([0.95, 0.75, 0.35, 1.0], "LIVE PREVIEW • uncommitted");
        }
        if !state.error.is_empty() {
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
