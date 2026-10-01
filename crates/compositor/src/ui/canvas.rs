use super::{
    navigation::{HitMap, link_curve},
    state::Shared,
};
use crate::{Composite, Node, Parameters as P, PortType, Source, package};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::{
    desktop::{DesktopCommand, Selection, ViewLocation},
    packages::CommandRequest,
};
use fold_project::DocumentRef;
use fold_ui::sdk::{
    ExtensionUi, GRAPH_COLORS, Panel,
    imgui::{self, Key, MouseButton},
    nodes::{self, NodeEditorUiExt},
};
use std::collections::BTreeMap;

pub(super) struct Canvas {
    state: Shared,
    editor: Option<nodes::EditorContext>,
    ids: BTreeMap<ObjectId, usize>,
    document: Option<DocumentId>,
    generation: u64,
    fit: u8,
    canvas_size: [f32; 2],
    hit_map: HitMap,
    search: String,
    popup_position: [f32; 2],
    cancel_move: bool,
    #[cfg(test)]
    pub(super) framed: bool,
    #[cfg(test)]
    pub(super) view: [[f32; 2]; 2],
    #[cfg(test)]
    pub(super) wire_probe: ([f32; 2], usize),
}
impl Canvas {
    pub fn new(state: Shared) -> Self {
        Self {
            state,
            editor: None,
            ids: BTreeMap::new(),
            document: None,
            generation: u64::MAX,
            fit: 2,
            canvas_size: [0.0; 2],
            hit_map: HitMap::default(),
            search: String::new(),
            popup_position: [0.0; 2],
            cancel_move: false,
            #[cfg(test)]
            framed: false,
            #[cfg(test)]
            view: [[0.0; 2]; 2],
            #[cfg(test)]
            wire_probe: ([0.0; 2], 0),
        }
    }
}
enum Action {
    Connect(ObjectId, ObjectId, &'static str),
    Disconnect(ObjectId, &'static str),
    Delete(Vec<ObjectId>),
    Add(Node),
}
fn color(kind: PortType) -> [f32; 4] {
    match kind {
        PortType::Mask => GRAPH_COLORS.mask,
        _ => GRAPH_COLORS.image,
    }
}
fn link_endpoints(
    graph: &Composite,
    pins: &BTreeMap<usize, (ObjectId, Option<usize>)>,
    a: nodes::PinId,
    b: nodes::PinId,
) -> Result<(ObjectId, ObjectId, &'static str), String> {
    let a = pins.get(&a.raw()).ok_or("Choose an input socket")?;
    let b = pins.get(&b.raw()).ok_or("Choose an output socket")?;
    let (source, target, slot) = match (a.1, b.1) {
        (None, Some(slot)) => (a.0, b.0, slot),
        (Some(slot), None) => (b.0, a.0, slot),
        _ => return Err("Connect an output to an input".into()),
    };
    Ok((
        source,
        target,
        graph.node(target)?.parameters.operator().input_key(slot),
    ))
}
impl Panel for Canvas {
    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn accepts_background_pan(&self, position: [f32; 2]) -> bool {
        self.hit_map.background(position)
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::COMPOSITE)
    }
    fn initialize(&mut self, context: &imgui::Context) {
        let editor = nodes::EditorContext::create_with_config(
            context,
            nodes::EditorConfig::new()
                .no_settings_file()
                // Trackpads send fractional wheel deltas; discrete zoom drops
                // them after snapping once to a preset level.
                .smooth_zoom(true, 1.3)
                .navigate_button(MouseButton::Middle),
        );
        editor.set_style_color(nodes::StyleColor::Background, GRAPH_COLORS.background);
        editor.set_style_color(nodes::StyleColor::Grid, GRAPH_COLORS.grid);
        editor.set_style_color(nodes::StyleColor::NodeBackground, GRAPH_COLORS.node);
        editor.set_style_color(nodes::StyleColor::SelectedNodeBorder, GRAPH_COLORS.selected);
        let mut style = editor.style();
        style.node_rounding = 6.0;
        style.selected_node_border_width = 2.5;
        style.highlight_connected_links = 1.0;
        editor.set_style(&style);
        self.editor = Some(editor);
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        self.hit_map.clear();
        let mut state = self.state.borrow_mut();
        state.sync(host);
        if ui.button("New composite") {
            state.create(host);
        }
        ui.same_line();
        if ui.button("Create from selection")
            && let Some(snapshot) = host.snapshot()
        {
            let selection = &host.state().selection;
            if let Some(document) = selection.document {
                host.command(DesktopCommand::Extension(CommandRequest {
                    id: "fold.app.create-composition".into(),
                    base: snapshot.revision(),
                    arguments: serde_json::to_vec(
                        &serde_json::json!({"document": document, "clips": selection.objects}),
                    )
                    .unwrap(),
                }));
                // Open the new dependency, retaining the parent playhead in the breadcrumb.
                if let Some(now) = host.snapshot()
                    && let Some(created) = now.state().documents.values().find(|d| {
                        d.type_id == crate::COMPOSITE
                            && !snapshot.state().documents.contains_key(&d.id)
                    })
                {
                    host.command(DesktopCommand::Navigate(ViewLocation {
                        document: created.id,
                        time: Time::ZERO,
                        label: "Composite".into(),
                    }));
                }
                state.sync(host);
            }
        }
        ui.same_line();
        if ui.button("View output") {
            state.cancel(host);
            state.view(host);
        }
        if let Some(snapshot) = host.snapshot()
            && let Some(_menu) = ui.begin_combo(
                "Document",
                state
                    .document
                    .map(|id| format!("Composite {id:?}"))
                    .unwrap_or_else(|| "Choose…".into()),
            )
        {
            for document in snapshot
                .state()
                .documents
                .values()
                .filter(|d| d.type_id == crate::COMPOSITE)
            {
                if ui.selectable(format!("Composite {:?}", document.id)) {
                    host.command(DesktopCommand::Navigate(ViewLocation {
                        document: document.id,
                        time: Time::ZERO,
                        label: "Composite".into(),
                    }));
                    state.sync(host);
                }
            }
        }
        let Some(mut graph) = state.graph.clone() else {
            ui.text_wrapped("Create a composite, or select timeline footage and choose Create from selection. Linked audio stays in the sequence.");
            ui.text_wrapped(&host.state().status);
            return;
        };
        let Some(editor_context) = &self.editor else {
            ui.text("Node editor is not initialized");
            return;
        };
        let document_changed = self.document != state.document;
        if document_changed {
            self.ids.clear();
            self.document = state.document;
            self.fit = 2;
        }
        for node in &graph.nodes {
            let next = (self.ids.len() + 1) * 16;
            self.ids.entry(node.id).or_insert(next);
        }
        layout_unpositioned(&mut graph);
        let mut open_search = ui.button("+ Add node  [Tab]");
        ui.same_line();
        if ui.button("Frame all  [F]") {
            self.fit = 1;
        }
        ui.same_line();
        if ui.button("Arrange") {
            if let Some(g) = &mut state.graph {
                let _ = g.auto_layout();
            }
            state.commit(host);
            graph = state.graph.clone().unwrap();
            layout_unpositioned(&mut graph);
        }
        ui.same_line();
        ui.text_disabled("Drag background: pan | Shift+drag: select | Scroll: zoom");
        if !state.error.is_empty() {
            ui.text_colored([1.0, 0.55, 0.35, 1.0], &state.error);
        }
        if state.editing && ui.is_key_pressed(Key::Escape) {
            state.cancel(host);
            graph = state.graph.clone().unwrap();
            layout_unpositioned(&mut graph);
            self.cancel_move = true;
        }
        let reset = document_changed || self.generation != state.generation || self.cancel_move;
        self.generation = state.generation;
        // Pass resolved dimensions. Native imgui-node-editor compares this
        // argument with the previous measured size: [0,0] falsely signals a
        // resize every frame and restores the old camera over navigation.
        let canvas_size = ui.content_region_avail().map(|v| v.max(1.0));
        // First-appearance scrollbars/docking may settle over several frames.
        // Native resize navigation overrides a fit made before that settles.
        if self.fit > 0 && self.canvas_size != canvas_size {
            self.fit = 2;
        }
        self.canvas_size = canvas_size;
        let canvas_origin = ui.cursor_screen_pos();
        if ui.is_window_hovered() && !ui.io().want_text_input() && !ui.is_any_item_active() {
            self.hit_map.bounds = Some([
                canvas_origin,
                [
                    canvas_origin[0] + canvas_size[0],
                    canvas_origin[1] + canvas_size[1],
                ],
            ]);
        }
        let editor = ui.node_editor(editor_context, "fold-compositor-canvas", canvas_size);
        let mut pins = BTreeMap::new();
        let mut pivots = BTreeMap::new();
        for node in &graph.nodes {
            let raw = self.ids[&node.id];
            let id = nodes::NodeId::new(raw);
            if reset {
                editor.set_node_position(id, node.position.unwrap_or([0.0; 2]));
            }
            let op = node.parameters.operator();
            let accent = match node.parameters {
                P::Read { .. } | P::Solid { .. } => GRAPH_COLORS.source,
                P::Merge => GRAPH_COLORS.merge,
                _ => color(op.output),
            };
            let _border = editor.push_style_color(nodes::StyleColor::NodeBorder, accent);
            editor.node(id, |token| {
                let origin = ui.cursor_pos();
                ui.text_colored(accent, op.id.to_uppercase());
                ui.dummy([190.0, 5.0]);
                if !matches!(node.parameters, P::Output) {
                    pins.insert(raw + 1, (node.id, None));
                    ui.set_cursor_pos([origin[0] + 132.0, origin[1] + 30.0]);
                    let pivot =
                        token.pin(nodes::PinId::new(raw + 1), nodes::PinKind::Output, |pin| {
                            socket(ui, pin, op.output_key(), op.output, true, true)
                        });
                    pivots.insert(raw + 1, pivot);
                }
                ui.set_cursor_pos([origin[0], origin[1] + 30.0]);
                for (slot, kind) in op.inputs.iter().enumerate() {
                    pins.insert(raw + 2 + slot, (node.id, Some(slot)));
                    let pivot = token.pin(
                        nodes::PinId::new(raw + 2 + slot),
                        nodes::PinKind::Input,
                        |pin| {
                            socket(
                                ui,
                                pin,
                                op.input_key(slot),
                                *kind,
                                false,
                                node.inputs[slot].is_some(),
                            )
                        },
                    );
                    pivots.insert(raw + 2 + slot, pivot);
                }
                let y = ui.cursor_pos()[1].max(origin[1] + 58.0);
                ui.set_cursor_pos([origin[0], y + 6.0]);
                ui.text_disabled(summary(&node.parameters));
                ui.dummy([190.0, 2.0]);
            });
            let p = editor.node_position(id);
            let size = editor.node_size(id);
            self.hit_map.nodes.push([
                editor.canvas_to_screen(p),
                editor.canvas_to_screen([p[0] + size[0], p[1] + size[1]]),
            ]);
        }
        if reset {
            editor.clear_selection();
            for id in &state.selected {
                if let Some(raw) = self.ids.get(id) {
                    editor.add_node_to_selection(nodes::NodeId::new(*raw));
                }
            }
        }
        let strength = editor_context.style().link_strength;
        for node in &graph.nodes {
            for (slot, input) in node.inputs.iter().enumerate() {
                if let Some(source) = input {
                    let pin = self.ids[&node.id] + 2 + slot;
                    self.hit_map.links.push(
                        link_curve(pivots[&(self.ids[source] + 1)], pivots[&pin], strength)
                            .map(|p| editor.canvas_to_screen(p)),
                    );
                    editor.link_colored(
                        nodes::LinkId::new(pin + 1_000_000),
                        nodes::PinId::new(self.ids[source] + 1),
                        nodes::PinId::new(pin),
                        color(node.parameters.operator().inputs[slot]),
                        2.5,
                    );
                }
            }
        }
        let mut actions = Vec::new();
        if let Some(create) = editor.begin_create([0.45, 0.85, 0.85, 1.0], 2.5)
            && let Some((a, b)) = create.query_new_link()
        {
            let result = link_endpoints(&graph, &pins, a, b).and_then(|(source, target, port)| {
                let mut candidate = graph.clone();
                candidate.connect(source, target, port)?;
                Ok((source, target, port))
            });
            match result {
                Ok((source, target, port)) => {
                    if create.accept_new_item() {
                        actions.push(Action::Connect(source, target, port));
                        state.error.clear();
                    }
                }
                Err(error) => {
                    create.reject_new_item_styled(GRAPH_COLORS.invalid, 3.0);
                    if !a.is_null() && !b.is_null() {
                        state.error = error;
                    }
                }
            }
        }
        if let Some(delete) = editor.begin_delete() {
            while let Some((_, _, target)) = delete.query_deleted_link() {
                if let Some((node, Some(slot))) = pins.get(&target.raw())
                    && delete.accept_deleted_item(false)
                {
                    actions.push(Action::Disconnect(
                        *node,
                        graph
                            .node(*node)
                            .unwrap()
                            .parameters
                            .operator()
                            .input_key(*slot),
                    ));
                }
            }
            let mut removed = Vec::new();
            while let Some(id) = delete.query_deleted_node() {
                if let Some(node) = graph.nodes.iter().find(|n| self.ids[&n.id] == id.raw()) {
                    if node.id == graph.output {
                        delete.reject_deleted_item();
                        state.error = "The document Output cannot be deleted".into();
                    } else if delete.accept_deleted_item(true) {
                        removed.push(node.id);
                    }
                }
            }
            if !removed.is_empty() {
                actions.push(Action::Delete(removed));
            }
        }
        #[cfg(test)]
        {
            self.view = [
                editor.canvas_to_screen([0.0, 0.0]),
                editor.canvas_to_screen([100.0, 100.0]),
            ];
            if let Some(c) = self.hit_map.links.first() {
                self.wire_probe = (
                    [0, 1].map(|i| (c[0][i] + 3.0 * c[1][i] + 3.0 * c[2][i] + c[3][i]) / 8.0),
                    editor.selected_links().len(),
                );
            }
            self.framed = graph.nodes.iter().all(|node| {
                let id = nodes::NodeId::new(self.ids[&node.id]);
                let p = editor.node_position(id);
                let s = editor.node_size(id);
                let min = editor.canvas_to_screen(p);
                let max = editor.canvas_to_screen([p[0] + s[0], p[1] + s[1]]);
                (0..2).all(|axis| {
                    min[axis] >= canvas_origin[axis] - 1.0
                        && max[axis] <= canvas_origin[axis] + canvas_size[axis] + 1.0
                })
            });
        }
        // Native input is applied in End, while Begin resets its changed flag.
        // Compare the resulting selection directly on the following frame.
        let selected: Vec<_> = editor
            .selected_nodes()
            .iter()
            .filter_map(|id| {
                graph
                    .nodes
                    .iter()
                    .find(|n| self.ids[&n.id] == id.raw())
                    .map(|n| n.id)
            })
            .collect();
        if state.selected != selected {
            state.selected = selected;
            host.command(DesktopCommand::Select(Selection {
                document: state.document,
                objects: state.selected.clone(),
            }));
        }
        let canvas_active = ui.is_window_focused() && !ui.io().want_text_input();
        if canvas_active && ui.is_key_pressed(Key::F) {
            self.fit = 1;
        }
        if canvas_active && ui.is_key_pressed(Key::Tab) {
            open_search = true;
        }
        if editor.show_background_context_menu() {
            open_search = true;
        }
        if open_search {
            self.popup_position = editor.screen_to_canvas(ui.io().mouse_pos());
        }
        if !reset && ui.is_mouse_down(MouseButton::Left) {
            let positions: Vec<_> = graph
                .nodes
                .iter()
                .map(|n| editor.node_position(nodes::NodeId::new(self.ids[&n.id])))
                .collect();
            let changed = graph.nodes.iter().zip(&positions).any(|(node, pos)| {
                let before = node.position.unwrap();
                (pos[0] - before[0]).abs() > 0.5 || (pos[1] - before[1]).abs() > 0.5
            });
            if changed {
                for (node, pos) in state
                    .graph
                    .as_mut()
                    .unwrap()
                    .nodes
                    .iter_mut()
                    .zip(positions)
                {
                    node.position = Some(pos);
                }
                state.editing = true;
            }
        }
        if self.fit > 0 {
            // The native editor needs one completed frame to measure new nodes.
            if self.fit == 1 {
                editor.navigate_to_content(0.15);
            }
            self.fit -= 1;
        }
        {
            let _suspend = editor.suspend();
            if open_search {
                self.search.clear();
                ui.open_popup("Add compositor node");
            }
            if let Some(_popup) = ui.begin_popup("Add compositor node") {
                ui.text("ADD NODE");
                ui.separator();
                if open_search {
                    ui.set_keyboard_focus_here();
                }
                ui.input_text("Search", &mut self.search).build();
                let query = self.search.to_lowercase();
                for parameters in defaults(&graph) {
                    let op = parameters.operator();
                    if op.id.to_lowercase().contains(&query) && ui.selectable(op.id) {
                        let mut node = Node::disconnected(parameters);
                        node.position = Some(self.popup_position);
                        actions.push(Action::Add(node));
                        ui.close_current_popup();
                    }
                }
                if "read".contains(&query)
                    && let Some(snapshot) = host.snapshot()
                {
                    ui.separator();
                    ui.text_disabled("READ DOCUMENT OUTPUT");
                    for d in snapshot
                        .state()
                        .documents
                        .values()
                        .filter(|d| Some(d.id) != state.document)
                    {
                        if let Ok(info) = host.video_info(d.id)
                            && ui.selectable(format!("Read {} {:?}", d.type_id, d.id))
                        {
                            let duration = info.time(info.frames).unwrap();
                            let mut node = Node::new(
                                P::Read {
                                    source: Source::Document {
                                        source: DocumentRef {
                                            document: d.id,
                                            output: "video".into(),
                                            extensions: Default::default(),
                                        },
                                        info,
                                    },
                                    start: Time::ZERO,
                                    source_start: Time::ZERO,
                                    duration,
                                },
                                vec![],
                            );
                            node.position = Some(self.popup_position);
                            actions.push(Action::Add(node));
                            ui.close_current_popup();
                        }
                    }
                    for n in &graph.nodes {
                        if let P::Read {
                            source: Source::Asset { asset, .. },
                            ..
                        } = &n.parameters
                            && ui.selectable(format!("Read media {asset:?}"))
                        {
                            let mut node = Node::new(n.parameters.clone(), vec![]);
                            node.position = Some(self.popup_position);
                            actions.push(Action::Add(node));
                            ui.close_current_popup();
                        }
                    }
                }
            }
        }
        editor.end();
        if !ui.is_mouse_down(MouseButton::Left) {
            self.cancel_move = false;
        }
        if !actions.is_empty() {
            // Keep the visible layout when the first structural edit persists
            // an older document that did not yet have authored positions.
            let mut candidate = graph.clone();
            let result = actions.into_iter().try_for_each(|action| match action {
                Action::Connect(s, t, p) => candidate.connect(s, t, p),
                Action::Disconnect(t, p) => candidate.disconnect(t, p),
                Action::Delete(ids) => candidate.remove(&ids),
                Action::Add(node) => {
                    state.selected = vec![node.id];
                    candidate.nodes.push(node);
                    candidate.validate()
                }
            });
            match result {
                Ok(()) => {
                    state.graph = Some(candidate);
                    state.commit(host);
                }
                Err(e) => state.error = e,
            }
        } else if state.editing
            && ui.is_mouse_released(MouseButton::Left)
            && !ui.is_any_item_active()
        {
            state.commit(host);
        }
    }
}
fn layout_unpositioned(graph: &mut Composite) {
    if graph.nodes.iter().any(|n| n.position.is_none()) {
        let mut layout = graph.clone();
        let _ = layout.auto_layout();
        for node in &mut graph.nodes {
            if node.position.is_none() {
                node.position = layout.node(node.id).ok().and_then(|n| n.position);
            }
        }
    }
}

fn socket(
    ui: &imgui::Ui,
    pin: &nodes::PinToken<'_>,
    label: &str,
    kind: PortType,
    output: bool,
    connected: bool,
) -> [f32; 2] {
    let _group = ui.begin_group();
    if output {
        ui.text_colored(color(kind), label);
        ui.same_line();
    }
    let p = ui.cursor_screen_pos();
    ui.get_window_draw_list()
        .add_circle([p[0] + 5.0, p[1] + 7.0], 4.5, color(kind))
        .filled(connected)
        .thickness(1.5)
        .build();
    ui.dummy([10.0, 14.0]);
    let pivot = [p[0] + 5.0, p[1] + 7.0];
    pin.pivot_rect(pivot, pivot);
    pin.pivot_alignment([0.5, 0.5]);
    if !output {
        ui.same_line();
        ui.text_colored(color(kind), label);
    }
    pivot
}

fn defaults(graph: &Composite) -> Vec<P> {
    let rect = [0, 0, graph.info.width, graph.info.height];
    vec![
        P::Solid {
            rgba: [0.2, 0.3, 0.5, 1.0],
        },
        P::Transform {
            translate: [0.0; 2],
            scale: [1.0; 2],
            opacity: 1.0,
        },
        P::Crop { rect },
        P::Blur { radius: 3 },
        P::Grade { gain: [1.0; 3] },
        P::Mask { rect },
        P::ApplyMask,
        P::Merge,
    ]
}
fn summary(p: &P) -> String {
    match p {
        P::Read { source, .. } => match source {
            Source::Asset { .. } => "Media source".into(),
            Source::Document { .. } => "Nested document".into(),
        },
        P::Blur { radius } => format!("Radius {radius} px"),
        P::Grade { gain } => format!("RGB {:.2} / {:.2} / {:.2}", gain[0], gain[1], gain[2]),
        P::Transform { opacity, .. } => format!("Opacity {:.0}%", opacity * 100.0),
        P::Merge => "Foreground over background".into(),
        P::Output => "Document video output".into(),
        P::Mask { .. } => "Rectangle • mask".into(),
        P::ApplyMask => "Premultiplied alpha".into(),
        P::Crop { .. } => "Transparent outside bounds".into(),
        P::Solid { .. } => "Linear RGBA".into(),
    }
}
