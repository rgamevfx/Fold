//! Native controls and source share one provider-owned construction and edit gate.
mod graph;
mod graph_templates;
mod instance;
mod keyframes;
mod literal;
mod procedural;
mod ramp;
mod references;
use literal::draw as literal;
mod presentation;
#[cfg(test)]
mod presentation_render;
mod protection;
mod scene;
mod state;
#[cfg(test)]
mod tests;
#[cfg(test)]
static UI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
#[cfg(test)]
use crate::types::Literal;
use crate::{binding::Binding, package};
use fold_platform::desktop::DesktopCommand;
use fold_ui::sdk::{self, EditResponse, ExtensionUi, Panel, PanelRegistry, UiId};
use state::Shared;

pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    let state = Shared::default();
    registry.register(crate::PACKAGE, Editor(state.clone(), Default::default()))?;
    registry.register_inspector(crate::PACKAGE, Inspector(state))?;
    let state = Shared::default();
    registry.register(
        crate::PACKAGE,
        scene::Editor(Editor(state.clone(), Default::default())),
    )?;
    registry.register_inspector(crate::PACKAGE, scene::Inspector::new(state))
}
struct Editor(Shared, sdk::graph_canvas::GraphCanvas);
struct Inspector(Shared);
impl Panel for Editor {
    fn initialize(
        &mut self,
        context: &sdk::imgui::Context,
        typography: Option<&sdk::typography::Typography>,
    ) {
        self.1.initialize(context, typography);
    }
    fn accepts_background_pan(&self, position: [f32; 2]) -> bool {
        self.1.accepts_background_pan(position)
    }
    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::CONSTRUCTION)
    }
    fn new_instance(&self) -> Option<sdk::EditorPanels> {
        let state = Shared::default();
        Some(sdk::EditorPanels {
            editor: Box::new(Editor(state.clone(), Default::default())),
            inspector: Box::new(Inspector(state)),
        })
    }
    fn cancel_interaction(&mut self, host: &mut dyn fold_platform::desktop::DesktopClient) {
        let editing = self.0.borrow().editing;
        if editing {
            self.0.borrow_mut().cancel(host);
        }
    }
    fn supports_network(&self) -> bool {
        true
    }
    fn is_layer_graph(&self) -> bool {
        true
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        self.draw_network(context);
    }
    fn draw_network(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = self.0.borrow_mut();
        state.sync(host);
        let Some(document) = state.document else {
            ui.text_wrapped("Select a scene object or an existing graph to edit.");
            return;
        };
        let draft_key = (document, state.object);
        if let Some(_tabs) = ui.tab_bar("motion-views") {
            if let Some(_tab) = ui.tab_item("Graph")
                && let Some(construction) = state.construction.as_ref()
            {
                let definitions: Vec<_> = construction
                    .definitions
                    .iter()
                    .map(|d| (d.id, d.name.clone()))
                    .collect();
                let definition = state
                    .graph_definition
                    .filter(|id| definitions.iter().any(|(key, _)| key == id))
                    .unwrap_or(construction.root.definition);
                let label = definitions
                    .iter()
                    .find(|(id, _)| *id == definition)
                    .map(|(_, name)| name.as_str())
                    .unwrap_or("Asset");
                if let Some(_menu) = ui.begin_combo("Asset", label) {
                    for (id, name) in &definitions {
                        if ui
                            .selectable_config(name)
                            .selected(*id == definition)
                            .build()
                        {
                            state.graph_definition = Some(*id);
                            state.graph_history.clear();
                            state.graph_occurrences.clear();
                            state.select(host, vec![]);
                        }
                    }
                }
                self.1.draw_inline(
                    ui,
                    &mut graph::Context {
                        state: &mut state,
                        host,
                        definition,
                    },
                );
            }
            if let Some(_tab) = ui.tab_item("Source") {
                let stale = state
                    .drafts
                    .get(&draft_key)
                    .is_some_and(|d| d.dirty && d.base != state.base);
                let enabled = state.drafts.get(&draft_key).is_some_and(|d| d.dirty) && !stale;
                let apply = {
                    let _disabled = ui.begin_disabled_with_cond(!enabled);
                    ui.button("Apply")
                };
                ui.same_line();
                if ui.button("Reload") {
                    state.reload_source(host);
                }
                if ui.is_item_hovered() {
                    ui.tooltip_text("Discard this source draft and load the current construction.");
                }
                if stale {
                    ui.same_line();
                    if ui.button("Rebase") {
                        state.rebase_source(host);
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text("Merge disjoint document edits into this draft for review. Does not apply it.");
                    }
                    ui.text_wrapped(
                        "Document changed. Rebase this draft or reload the current source.",
                    );
                }
                if apply {
                    let text = state
                        .drafts
                        .get(&draft_key)
                        .map(|d| d.text.clone())
                        .unwrap_or_default();
                    match crate::source::parse(&text) {
                        Ok(parsed) => {
                            if state.commit(host, parsed.construction) {
                                state.reload_source(host);
                            }
                        }
                        Err(error) => {
                            state.error = format!(
                                "{} (bytes {}–{})",
                                error.message, error.span.start, error.span.end
                            )
                        }
                    }
                }
                if let Some(draft) = state.drafts.get_mut(&draft_key) {
                    let size = [
                        ui.content_region_avail()[0].max(1.),
                        (ui.content_region_avail()[1] - 48.).max(80.),
                    ];
                    if ui
                        .input_text_multiline("##motion-source", &mut draft.text, size)
                        .build()
                    {
                        draft.dirty = true;
                    }
                }
            }
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}
impl Panel for Inspector {
    fn id(&self) -> &'static str {
        package::INSPECTOR
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::CONSTRUCTION)
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = self.0.borrow_mut();
        state.sync(host);
        state.literal_controls.invalid = false;
        let Some(document) = state.document else {
            ui.text_disabled("Select an object or graph.");
            return;
        };
        if protection::menu(ui, host, &mut state) {
            return;
        }
        let mut selected = host.state().selection.objects.clone();
        if let Some(object) = state.object
            && selected.as_slice() == [object]
            && let Some(graph) = &state.construction
        {
            if let Ok(definition) = graph.definition(graph.root.definition)
                && let Some(output) = definition.outputs.first()
                && let crate::model::Reference::Node { node } = output.value.target
            {
                selected = vec![node];
            }
        }
        let Some(mut construction) = state.construction.clone() else {
            return;
        };
        let locked = crate::protection::locked_ids(&construction).unwrap_or_default();
        let mut response = EditResponse::default();
        let mut animate = None;
        let _width = ui.push_item_width((ui.content_region_avail()[0] * 0.43).max(60.));
        if let Some(definition) = construction.definitions.iter_mut().find(|d| {
            if selected.is_empty() {
                d.id == state
                    .graph_definition
                    .unwrap_or(construction.root.definition)
            } else {
                selected.contains(&d.id) || d.inputs.iter().any(|i| selected.contains(&i.id))
            }
        }) {
            ui.text(&definition.name);
            for input in &mut definition.inputs {
                if !selected.is_empty()
                    && !selected.contains(&input.id)
                    && !selected.contains(&definition.id)
                {
                    continue;
                }
                let _id = UiId {
                    package: crate::PACKAGE,
                    panel: package::INSPECTOR,
                    document,
                    object: input.id,
                }
                .scope(ui);
                let _disabled = ui.begin_disabled_with_cond(locked.contains(&input.id));
                literal(
                    ui,
                    &input.name,
                    &mut input.default,
                    &mut response,
                    &mut state.literal_controls,
                );
                drop(_disabled);
                references::input_menu(
                    ui,
                    document,
                    definition.id,
                    input,
                    !locked.contains(&input.id),
                    &mut response,
                );
                if locked.contains(&input.id) {
                    ui.text_disabled("Locked");
                }
            }
        }
        let show_root = selected.contains(&construction.root.id)
            || (selected.is_empty()
                && state
                    .graph_definition
                    .unwrap_or(construction.root.definition)
                    == construction.root.definition
                && ui.collapsing_header("Main instance", sdk::imgui::TreeNodeFlags::empty()));
        if show_root && let Ok(definition) = construction.definition(construction.root.definition) {
            let inputs = definition.inputs.clone();
            instance::draw(
                ui,
                &mut construction.root,
                &inputs,
                &locked,
                state.object_time(host).ok(),
                &mut response,
                &mut state.literal_controls,
            );
        }
        let inspected_definition = state
            .graph_definition
            .unwrap_or(construction.root.definition);
        if state.key_time.is_some() && !state.editing {
            if let Ok(time) = state.object_time(host) {
                let parameters = construction
                    .definition(inspected_definition)
                    .map(|d| {
                        d.nodes
                            .iter()
                            .filter(|n| selected.contains(&n.id))
                            .flat_map(|n| n.parameters.values())
                            .filter(|p| {
                                matches!(
                                    p.binding(),
                                    Ok(Binding::Link { .. } | Binding::Expression { .. })
                                )
                            })
                            .map(|p| p.id)
                            .collect()
                    })
                    .unwrap_or_default();
                let key = references::SampleKey {
                    document,
                    object: state.object,
                    revision: state.base,
                    time,
                    occurrence: state.graph_occurrences.clone(),
                    parameters,
                };
                state.reference_samples.update(key, &construction);
            }
        } else {
            state.reference_samples.values.clear();
        }
        let sampled_parameters = state.reference_samples.values.clone();
        let reference_snapshot = construction.clone();
        let mut reference_selection = None;
        let instance_inputs: std::collections::BTreeMap<_, _> = construction
            .definitions
            .iter()
            .flat_map(|definition| &definition.instances)
            .filter(|instance| selected.contains(&instance.id))
            .filter_map(|instance| {
                construction
                    .definition(instance.definition)
                    .ok()
                    .map(|definition| (instance.id, definition.inputs.clone()))
            })
            .collect();
        if !selected.is_empty() {
            for definition in &mut construction.definitions {
                for instance in &mut definition.instances {
                    if let Some(inputs) = instance_inputs.get(&instance.id) {
                        instance::draw(
                            ui,
                            instance,
                            inputs,
                            &locked,
                            state
                                .key_time
                                .filter(|(id, _)| *id == definition.id)
                                .map(|(_, time)| time),
                            &mut response,
                            &mut state.literal_controls,
                        );
                    }
                }
                for output in &definition.outputs {
                    if selected.contains(&output.id) {
                        ui.text(&output.name);
                        ui.text_wrapped(
                            "Output · Reconnect in Graph; edit its declaration in Source.",
                        );
                    }
                }
                for node in &mut definition.nodes {
                    if !selected.contains(&node.id) {
                        continue;
                    }
                    ui.text(&node.name);
                    let Ok(operation) =
                        crate::registry::resolve(&node.operation, node.version, &node.mode)
                    else {
                        ui.text_disabled("Unresolved operation · Edit in Source");
                        continue;
                    };
                    procedural::schema(ui, node, &locked, &mut response);
                    for (name, parameter) in &mut node.parameters {
                        let _id = UiId {
                            package: crate::PACKAGE,
                            panel: package::INSPECTOR,
                            document,
                            object: parameter.id,
                        }
                        .scope(ui);
                        let _disabled = ui.begin_disabled_with_cond(locked.contains(&parameter.id));
                        let label = operation
                            .parameters
                            .get(name)
                            .map(|port| port.title.as_str())
                            .unwrap_or(name);
                        match parameter.binding() {
                            Ok(Binding::Literal { mut value }) => {
                                let previous = value.clone();
                                literal(
                                    ui,
                                    label,
                                    &mut value,
                                    &mut response,
                                    &mut state.literal_controls,
                                );
                                if value.ty().is_interpolatable() {
                                    if value != previous
                                        && host.state().auto_key
                                        && !state.literal_controls.invalid
                                        && let Some((_, time)) =
                                            state.key_time.filter(|(id, _)| *id == definition.id)
                                    {
                                        state.auto_keys.insert(parameter.id, time);
                                    }
                                    ui.same_line();
                                    if let Some(_menu) = sdk::toolbar::icon_menu(
                                        ui,
                                        "animation",
                                        "Animation options",
                                    ) && ui.menu_item("Animate")
                                    {
                                        let time = state
                                            .key_time
                                            .filter(|(id, _)| *id == definition.id)
                                            .map_or(fold_foundation::Time::ZERO, |(_, time)| time);
                                        animate = Some((parameter.id, time));
                                    }
                                }
                                if let Ok(value) = serde_json::to_value(Binding::Literal { value })
                                {
                                    parameter.value = value;
                                }
                            }
                            Ok(Binding::Ramp { mut ramp }) => {
                                ramp::draw(
                                    ui,
                                    label,
                                    &mut ramp,
                                    &mut response,
                                    &mut state.literal_controls,
                                );
                                parameter.value = serde_json::to_value(Binding::Ramp { ramp })
                                    .expect("native ramp");
                            }
                            Ok(Binding::Track { mut track }) => {
                                let time = state
                                    .key_time
                                    .filter(|(id, _)| *id == definition.id)
                                    .map(|(_, time)| time);
                                keyframes::draw(
                                    ui,
                                    label,
                                    &mut track,
                                    time,
                                    &mut response,
                                    &mut state.literal_controls,
                                );
                                parameter.value = serde_json::to_value(Binding::Track { track })
                                    .expect("native track");
                            }
                            Ok(_) => {
                                if let Some(value) = sampled_parameters.get(&parameter.id) {
                                    ui.text(format!(
                                        "{label} · Driven: {}",
                                        references::display(value)
                                    ));
                                } else {
                                    ui.text(format!("{label} · Driven"));
                                }
                            }
                            Err(_) => ui.text(format!("{label} · Unresolved")),
                        }
                        drop(_disabled);
                        let editable = !locked.contains(&parameter.id);
                        let sampled = sampled_parameters.get(&parameter.id);
                        match references::draw(
                            ui,
                            parameter,
                            &mut state.reference_drafts,
                            references::Context {
                                document,
                                graph: &reference_snapshot,
                                definition: reference_snapshot
                                    .definition(definition.id)
                                    .expect("existing definition"),
                                owner: &node.name,
                                sampled,
                                editable,
                            },
                        ) {
                            Ok((changed, selection)) => {
                                response.changed |= changed;
                                response.finished |= changed;
                                reference_selection = selection.or(reference_selection);
                            }
                            Err(error) => state.error = error,
                        }
                        if locked.contains(&parameter.id) {
                            ui.text_disabled("Locked");
                        }
                    }
                }
            }
        }
        if let Some(selection) = reference_selection {
            state.select(host, vec![selection]);
        }
        if !state.reference_samples.error.is_empty() {
            ui.text_wrapped(&state.reference_samples.error);
        }
        let definition = state
            .graph_definition
            .unwrap_or(construction.root.definition);
        if let Some(group) =
            presentation::draw(ui, &mut construction, definition, &selected, &mut response)
        {
            match presentation::ungroup(&construction, group) {
                Ok(after) => {
                    state.commit(host, after);
                }
                Err(error) => state.error = error,
            }
            return;
        }
        if let Some((parameter, time)) = animate {
            match crate::keyed::animate(&construction, parameter, time) {
                Ok(after) => {
                    state.commit(host, after);
                }
                Err(error) => state.error = error,
            }
            return;
        }
        response.finished &= !state.literal_controls.invalid;
        response.cancel_on_escape(ui, state.editing);
        if response.cancelled {
            state.cancel(host);
        } else {
            if response.changed {
                state.construction = Some(construction.clone());
                state.preview(host);
            }
            if response.finished && state.editing {
                state.finish_parameters(host, construction);
            }
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}
