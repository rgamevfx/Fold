use super::state::Shared;
use crate::{Parameters as P, Source, package};
use fold_foundation::Time;
use fold_platform::desktop::{DesktopCommand, ViewLocation};
use fold_ui::sdk::{
    EditResponse as Response, ExtensionUi, NumericProperty, Panel, UiId,
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
        let mut state = self.0.borrow_mut();
        state.sync(host);
        ui.text("NODE PROPERTIES");
        ui.separator();
        let Some(id) = state.selected.first().copied() else {
            ui.text_wrapped("Select a node on the graph to edit its properties. Drag wires between matching sockets to connect nodes.");
            ui.separator();
            ui.text_wrapped(&host.state().status);
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
        let Some(node) = state.graph.as_mut().and_then(|g| g.node_mut(id).ok()) else {
            return;
        };
        ui.text(node.parameters.operator().id);
        ui.text_disabled(format!("{id:?}"));
        ui.separator();
        let mut response = Response::default();
        let mut navigate = None;
        ui.text_disabled("Drag values • Ctrl-click to type • Escape cancels");
        match &mut node.parameters {
            P::Solid { rgba } => {
                ui.text("Scene-linear premultiplied color");
                for (i,label) in ["Red", "Green", "Blue", "Alpha"].iter().enumerate() {
                    let changed = Drag::new(label).speed(0.005).range(0.0,1.0).build(ui, &mut rgba[i]);
                    if changed { for c in 0..3 { rgba[c] = rgba[c].min(rgba[3]); } }
                    response.item(ui,changed);
                }
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
                ui.text_wrapped("Half-open rectangle in document pixels. Outside is transparent.");
            }
            P::Blur { radius } => { let changed = Drag::new("Radius (pixels)").speed(0.1).range(0,64).build(ui,radius); response.item(ui,changed); ui.text_wrapped("Box filter in linear light. Transparent borders expand the image support."); }
            P::Grade { gain } => { for (i,label) in ["Red gain", "Green gain", "Blue gain"].iter().enumerate() { let changed = Drag::new(label).speed(0.01).range(0.0,16.0).build(ui,&mut gain[i]); response.item(ui,changed); } ui.text_wrapped("Scene-linear gain, clamped to the supported SDR range."); }
            P::Read { source, start, source_start, duration } => {
                rational(ui,"Start",start,&mut response); rational(ui,"Source in",source_start,&mut response); rational(ui,"Duration",duration,&mut response);
                ui.separator();
                match source {
                    Source::Document { source, .. } => {
                        ui.text_wrapped(format!("Document {:?} / {}", source.document, source.output));
                        let time = host.state().navigation.last().map(|v| v.time).unwrap_or_else(|| Time::new(i64::from(host.state().frame)*i64::from(host.state().rate[1]),host.state().rate[0]).unwrap());
                        let local = if time >= *start && start.checked_add(*duration).is_ok_and(|end| time < end) { time.checked_sub(*start).and_then(|t| t.checked_add(*source_start)).ok() } else { None };
                        if let Some(local) = local {
                            ui.text(format!("Source time: {}/{} s", local.numerator(),local.denominator()));
                            if ui.button("Open Source") { navigate = Some(ViewLocation { document: source.document, time: local, label: "Source composite".into() }); }
                        } else { ui.text_disabled("Read is inactive at this time"); }
                    }
                    Source::Asset { asset, info } => {
                        ui.text(format!("{} x {} • {}/{} fps",info.width,info.height,info.rate[0],info.rate[1]));
                        if let Some(snapshot) = host.snapshot() && let Some(asset) = snapshot.state().assets.get(asset) { ui.text_wrapped(&asset.location); }
                    }
                }
            }
            P::Merge => ui.text_wrapped("Foreground over background, using premultiplied alpha in scene-linear light."),
            P::ApplyMask => ui.text_wrapped("Multiply image RGBA by the connected mask. Image and mask sockets are different types."),
            P::Output => ui.text_wrapped("The document's published video output. Connect the final image here."),
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
        ui.text_wrapped(&host.state().status);
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
