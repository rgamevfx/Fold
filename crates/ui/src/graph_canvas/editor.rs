use super::{
    navigation::{HitMap, link_curve},
    *,
};
use crate::sdk::{
    GRAPH_COLORS,
    imgui::{self, Key, MouseButton},
    toolbar::{ToolbarIcon, icon_button, icon_menu},
};
use dear_node_editor::{self as nodes, NodeEditorUiExt};
use std::collections::BTreeMap;

/// Native handles are allocated, not computed with fixed per-node offsets:
/// dynamic group sockets cannot collide with other nodes, pins or links.
#[derive(Default)]
struct Ids {
    next: usize,
    nodes: BTreeMap<ObjectId, usize>,
    pins: BTreeMap<(Socket, bool), usize>,
    links: BTreeMap<Socket, usize>,
}
impl Ids {
    fn register(&mut self, graph: &GraphView) {
        for node in &graph.nodes {
            self.nodes.entry(node.id).or_insert_with(|| {
                self.next += 1;
                self.next
            });
            for (ports, output) in [(&node.inputs, false), (&node.outputs, true)] {
                for port in ports {
                    let socket = Socket {
                        node: node.id,
                        key: port.key.clone(),
                    };
                    self.pins.entry((socket, output)).or_insert_with(|| {
                        self.next += 1;
                        self.next
                    });
                }
            }
        }
        for wire in &graph.wires {
            self.links.entry(wire.target.clone()).or_insert_with(|| {
                self.next += 1;
                self.next
            });
        }
    }
    fn connection(&self, a: nodes::PinId, b: nodes::PinId) -> Result<WireView, String> {
        let find = |id| {
            self.pins
                .iter()
                .find(|(_, raw)| **raw == id)
                .map(|(key, _)| key)
        };
        let a = find(a.raw()).ok_or("Choose an output and an input socket")?;
        let b = find(b.raw()).ok_or("Choose an output and an input socket")?;
        match (a.1, b.1) {
            (true, false) => Ok(WireView {
                source: a.0.clone(),
                target: b.0.clone(),
            }),
            (false, true) => Ok(WireView {
                source: b.0.clone(),
                target: a.0.clone(),
            }),
            _ => Err("Connect an output to an input".into()),
        }
    }
}
#[derive(Default)]
pub struct GraphCanvas {
    editor: Option<nodes::EditorContext>,
    ids: Ids,
    key: Option<GraphKey>,
    generation: Option<u64>,
    selection: Vec<ObjectId>,
    fit: u8,
    size: [f32; 2],
    hit_map: HitMap,
    search: String,
    popup_position: [f32; 2],
    moving: bool,
    cancel_move: bool,
    error: String,
    #[cfg(test)]
    pub(super) framed: bool,
    #[cfg(test)]
    pub(super) view: [[f32; 2]; 2],
    #[cfg(test)]
    pub(super) wire_probe: ([f32; 2], usize),
}
impl GraphCanvas {
    pub fn initialize(&mut self, context: &imgui::Context) {
        let editor = nodes::EditorContext::create_with_config(
            context,
            nodes::EditorConfig::new()
                .no_settings_file()
                // Fractional trackpad deltas must not snap to discrete zoom levels.
                .smooth_zoom(true, 1.3)
                .navigate_button(MouseButton::Middle),
        );
        editor.set_style_color(nodes::StyleColor::Background, GRAPH_COLORS.background);
        editor.set_style_color(nodes::StyleColor::Grid, GRAPH_COLORS.grid);
        editor.set_style_color(nodes::StyleColor::NodeBackground, GRAPH_COLORS.node);
        editor.set_style_color(nodes::StyleColor::SelectedNodeBorder, GRAPH_COLORS.selected);
        let mut style = editor.style();
        style.node_rounding = 6.;
        style.selected_node_border_width = 2.5;
        style.highlight_connected_links = 1.;
        editor.set_style(&style);
        self.editor = Some(editor);
        self.frame_all();
    }
    pub fn frame_all(&mut self) {
        self.fit = 2;
    }
    /// Clear stale event-time geometry when the panel has no graph to draw.
    pub fn clear_hit_map(&mut self) {
        self.hit_map.clear();
    }
    pub fn accepts_background_pan(&self, position: [f32; 2]) -> bool {
        self.hit_map.background(position)
    }
    pub fn draw(&mut self, ui: &imgui::Ui, context: &mut impl GraphContext) {
        self.draw_with(ui, context, false);
    }
    /// Join provider actions on one toolbar row; narrow panels use overflow.
    pub fn draw_inline(&mut self, ui: &imgui::Ui, context: &mut impl GraphContext) {
        self.draw_with(ui, context, true);
    }
    fn draw_with(&mut self, ui: &imgui::Ui, context: &mut impl GraphContext, inline: bool) {
        self.hit_map.clear();
        let mut graph = context.graph();
        let changed = self.key != Some(graph.key);
        if changed {
            self.key = Some(graph.key);
            self.frame_all();
            self.moving = false;
            self.cancel_move = false;
            self.error.clear();
        }
        if self.generation != Some(graph.generation) {
            // An external revision wins over the in-flight gesture. Ignore the
            // remaining physical drag rather than committing stale positions.
            self.cancel_move |= self.moving;
            self.moving = false;
        }
        if ui.is_key_pressed(Key::Escape) && ui.is_window_focused() {
            self.error = context.event(GraphEvent::Cancel).err().unwrap_or_default();
            graph = context.graph();
            self.moving = false;
            self.cancel_move = true;
        }
        let mut actions = Vec::new();
        if inline {
            ui.same_line();
        }
        let can_open = graph.selected.len() == 1
            && graph
                .nodes
                .iter()
                .any(|n| n.id == graph.selected[0] && n.can_open);
        let required = 3. * ui.frame_height_with_spacing()
            + if graph.has_parent {
                ui.frame_height_with_spacing()
            } else {
                0.
            }
            + if can_open {
                ui.calc_text_size("Open group")[0] + 16.
            } else {
                0.
            };
        let compact = inline && ui.content_region_avail()[0] < required;
        if graph.has_parent && !compact {
            if icon_button(ui, "parent", ToolbarIcon::Back, "Return to parent graph") {
                self.error = context
                    .event(GraphEvent::Navigate(None))
                    .err()
                    .unwrap_or_default();
                return;
            }
            ui.same_line();
        }
        if can_open && !compact {
            if ui.button("Open group") {
                self.error = context
                    .event(GraphEvent::Navigate(Some(graph.selected[0])))
                    .err()
                    .unwrap_or_default();
                return;
            }
            ui.same_line();
        }
        let mut open_search = false;
        if !compact {
            open_search = icon_button(ui, "add-node", ToolbarIcon::Add, "Add node (Tab)");
            ui.same_line();
            if icon_button(ui, "frame-all", ToolbarIcon::FrameAll, "Frame all (F)") {
                self.fit = 1;
            }
            ui.same_line();
        }
        if let Some(_menu) = icon_menu(ui, "graph-actions", "Graph actions and navigation help") {
            if compact {
                if graph.has_parent && ui.menu_item("Return to parent graph") {
                    self.error = context
                        .event(GraphEvent::Navigate(None))
                        .err()
                        .unwrap_or_default();
                    return;
                }
                if can_open && ui.menu_item("Open group") {
                    self.error = context
                        .event(GraphEvent::Navigate(Some(graph.selected[0])))
                        .err()
                        .unwrap_or_default();
                    return;
                }
                if ui.menu_item("Add node (Tab)") {
                    open_search = true;
                }
                if ui.menu_item("Frame all (F)") {
                    self.fit = 1;
                }
                ui.separator();
            }
            if ui.menu_item("Arrange nodes") {
                actions.push(GraphChange::Positions(arrange(ui, &graph)));
            }
            ui.separator();
            ui.text_disabled("Drag background: pan");
            ui.text_disabled("Shift+drag: box select");
            ui.text_disabled("Scroll: zoom");
            ui.text_disabled("Delete: remove selection");
        }
        if !graph.error.is_empty() {
            ui.text_colored(GRAPH_COLORS.invalid, &graph.error);
        }
        if !self.error.is_empty() && self.error != graph.error {
            ui.text_colored(GRAPH_COLORS.invalid, &self.error);
        }
        if graph.nodes.iter().any(|n| n.position.is_none()) {
            let layout: BTreeMap<_, _> = arrange(ui, &graph).into_iter().collect();
            for node in &mut graph.nodes {
                node.position.get_or_insert(layout[&node.id]);
            }
        }
        self.ids.register(&graph);
        let Some(native) = &self.editor else {
            ui.text("Node editor is not initialized");
            return;
        };
        let reset = changed || self.generation != Some(graph.generation) || self.cancel_move;
        self.generation = Some(graph.generation);
        // Explicit dimensions avoid the native resize path restoring the old
        // camera every frame. Initial docking/scrollbars need time to settle.
        let size = ui.content_region_avail().map(|v| v.max(1.));
        if self.fit > 0 && self.size != size {
            self.fit = 2;
        }
        self.size = size;
        let origin = ui.cursor_screen_pos();
        if ui.is_window_hovered() && !ui.io().want_text_input() && !ui.is_any_item_active() {
            self.hit_map.bounds = Some([origin, [origin[0] + size[0], origin[1] + size[1]]]);
        }
        let editor = ui.node_editor(native, "fold-graph-canvas", size);
        let mut pivots = BTreeMap::new();
        for node in &graph.nodes {
            let id = nodes::NodeId::new(self.ids.nodes[&node.id]);
            if reset {
                editor.set_node_position(id, node.position.unwrap());
            }
            let _border = editor.push_style_color(nodes::StyleColor::NodeBorder, node.color);
            editor.node(id, |token| {
                let origin = ui.cursor_pos();
                let [width, height] = node_size(ui, node);
                ui.text_colored(node.color, &node.label);
                ui.dummy([width, 5.]);
                for (ports, output) in [(&node.inputs, false), (&node.outputs, true)] {
                    for (i, port) in ports.iter().enumerate() {
                        let socket = Socket {
                            node: node.id,
                            key: port.key.clone(),
                        };
                        let raw = self.ids.pins[&(socket.clone(), output)];
                        let x = if output {
                            width - ui.calc_text_size(&port.label)[0] - 18.
                        } else {
                            0.
                        };
                        ui.set_cursor_pos([origin[0] + x, origin[1] + 30. + i as f32 * 22.]);
                        let connected = graph.wires.iter().any(|w| {
                            if output {
                                w.source == socket
                            } else {
                                w.target == socket
                            }
                        });
                        let pivot = token.pin(
                            nodes::PinId::new(raw),
                            if output {
                                nodes::PinKind::Output
                            } else {
                                nodes::PinKind::Input
                            },
                            |pin| draw_socket(ui, pin, &port.label, output, connected, port.color),
                        );
                        pivots.insert(raw, pivot);
                    }
                }
                ui.set_cursor_pos([origin[0], origin[1] + height]);
                if !node.summary.is_empty() {
                    ui.text_disabled(&node.summary);
                }
                ui.dummy([width, 2.]);
            });
            let p = editor.node_position(id);
            let s = editor.node_size(id);
            self.hit_map.nodes.push([
                editor.canvas_to_screen(p),
                editor.canvas_to_screen([p[0] + s[0], p[1] + s[1]]),
            ]);
        }
        if reset || self.selection != graph.selected {
            editor.clear_selection();
            for id in &graph.selected {
                if let Some(raw) = self.ids.nodes.get(id) {
                    editor.add_node_to_selection(nodes::NodeId::new(*raw));
                }
            }
        }
        for wire in &graph.wires {
            let (Some(&source), Some(&target)) = (
                self.ids.pins.get(&(wire.source.clone(), true)),
                self.ids.pins.get(&(wire.target.clone(), false)),
            ) else {
                continue;
            };
            let (Some(&a), Some(&b)) = (pivots.get(&source), pivots.get(&target)) else {
                continue;
            };
            self.hit_map.links.push(
                link_curve(a, b, native.style().link_strength).map(|p| editor.canvas_to_screen(p)),
            );
            let color = graph
                .nodes
                .iter()
                .find(|n| n.id == wire.target.node)
                .and_then(|n| n.inputs.iter().find(|p| p.key == wire.target.key))
                .map(|p| p.color)
                .unwrap_or(GRAPH_COLORS.image);
            editor.link_colored(
                nodes::LinkId::new(self.ids.links[&wire.target]),
                nodes::PinId::new(source),
                nodes::PinId::new(target),
                color,
                2.5,
            );
        }
        if let Some(create) = editor.begin_create(GRAPH_COLORS.selected, 2.5)
            && let Some((a, b)) = create.query_new_link()
        {
            let result = self.ids.connection(a, b).and_then(|wire| {
                let change = GraphChange::Connect(wire);
                context.validate(&change)?;
                Ok(change)
            });
            match result {
                Ok(change) => {
                    if create.accept_new_item() {
                        actions.push(change);
                        self.error.clear();
                    }
                }
                Err(error) => {
                    create.reject_new_item_styled(GRAPH_COLORS.invalid, 3.);
                    if !a.is_null() && !b.is_null() {
                        self.error = error;
                    }
                }
            }
        }
        if let Some(delete) = editor.begin_delete() {
            while let Some((id, _, _)) = delete.query_deleted_link() {
                if let Some(wire) = graph
                    .wires
                    .iter()
                    .find(|w| self.ids.links[&w.target] == id.raw())
                    && delete.accept_deleted_item(false)
                {
                    actions.push(GraphChange::Disconnect(wire.target.clone()));
                }
            }
            let mut removed = Vec::new();
            while let Some(id) = delete.query_deleted_node() {
                if let Some(node) = graph
                    .nodes
                    .iter()
                    .find(|n| self.ids.nodes[&n.id] == id.raw())
                {
                    match context.validate(&GraphChange::Delete(vec![node.id])) {
                        Ok(()) => {
                            if delete.accept_deleted_item(true) {
                                removed.push(node.id);
                            }
                        }
                        Err(error) => {
                            delete.reject_deleted_item();
                            self.error = error;
                        }
                    }
                }
            }
            if !removed.is_empty() {
                actions.push(GraphChange::Delete(removed));
            }
        }
        #[cfg(test)]
        {
            self.view = [
                editor.canvas_to_screen([0.; 2]),
                editor.canvas_to_screen([100.; 2]),
            ];
            if let Some(c) = self.hit_map.links.first() {
                self.wire_probe = (
                    [0, 1].map(|i| (c[0][i] + 3. * c[1][i] + 3. * c[2][i] + c[3][i]) / 8.),
                    editor.selected_links().len(),
                );
            }
            self.framed = self.hit_map.nodes.iter().all(|r| {
                (0..2).all(|i| r[0][i] >= origin[i] - 1. && r[1][i] <= origin[i] + size[i] + 1.)
            });
        }
        // Native selection is updated at End, not at Begin's changed flag.
        let selected: Vec<_> = editor
            .selected_nodes()
            .iter()
            .filter_map(|id| {
                graph
                    .nodes
                    .iter()
                    .find(|n| self.ids.nodes[&n.id] == id.raw())
                    .map(|n| n.id)
            })
            .collect();
        let mut events = Vec::new();
        if selected != graph.selected {
            events.push(GraphEvent::Select(selected.clone()));
        }
        self.selection = selected;
        if let Some(id) = editor.double_clicked_node()
            && let Some(node) = graph
                .nodes
                .iter()
                .find(|n| n.can_open && self.ids.nodes[&n.id] == id.raw())
        {
            events.push(GraphEvent::Navigate(Some(node.id)));
        }
        let active = ui.is_window_focused() && !ui.io().want_text_input();
        if active && ui.is_key_pressed(Key::F) {
            self.fit = 1;
        }
        open_search |= active && ui.is_key_pressed(Key::Tab);
        open_search |= editor.show_background_context_menu();
        if open_search {
            self.popup_position = editor.screen_to_canvas(ui.io().mouse_pos());
        }
        // End applies native motion, so also sample on release: a fast drag
        // may have no intervening held-button frame after the node first moves.
        if !reset
            && (self.moving
                || ui.is_mouse_down(MouseButton::Left)
                || ui.is_mouse_released(MouseButton::Left))
        {
            let positions: Vec<_> = graph
                .nodes
                .iter()
                .map(|n| {
                    (
                        n.id,
                        editor.node_position(nodes::NodeId::new(self.ids.nodes[&n.id])),
                    )
                })
                .collect();
            if graph
                .nodes
                .iter()
                .zip(&positions)
                .any(|(n, (_, p))| (0..2).any(|i| (n.position.unwrap()[i] - p[i]).abs() > 0.5))
            {
                self.moving = true;
                events.push(GraphEvent::PreviewPositions(positions));
            }
        }
        if self.fit > 0 {
            if self.fit == 1 {
                editor.navigate_to_content(0.15);
            }
            self.fit -= 1;
        }
        {
            let _suspend = editor.suspend();
            if open_search {
                self.search.clear();
                ui.open_popup("Add graph node");
            }
            if let Some(_popup) = ui.begin_popup("Add graph node") {
                ui.text("ADD NODE");
                ui.separator();
                if open_search {
                    ui.set_keyboard_focus_here();
                }
                ui.input_text("Search", &mut self.search).build();
                let query = self.search.to_lowercase();
                for template in context.catalog() {
                    let label = format!("{} / {}", template.category, template.label);
                    if format!("{label} {}", template.key)
                        .to_lowercase()
                        .contains(&query)
                    {
                        let _id = ui.push_id(&template.key);
                        if ui.selectable(label) {
                            actions.push(GraphChange::Add {
                                template: template.key,
                                position: self.popup_position,
                            });
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
            // Preserve presentation-only layout for old documents at the first
            // structural edit. An explicit Arrange remains last and wins.
            actions.insert(
                0,
                GraphChange::Positions(
                    graph
                        .nodes
                        .iter()
                        .map(|n| (n.id, n.position.unwrap()))
                        .collect(),
                ),
            );
            events.push(GraphEvent::Commit(actions));
            self.moving = false;
        } else if self.moving && !ui.is_mouse_down(MouseButton::Left) && !ui.is_any_item_active() {
            events.push(GraphEvent::Commit(Vec::new()));
            self.moving = false;
        }
        for event in events {
            let edit = matches!(
                event,
                GraphEvent::Commit(_) | GraphEvent::PreviewPositions(_)
            );
            let commit = matches!(event, GraphEvent::Commit(_));
            match context.event(event) {
                Ok(()) => {
                    if commit {
                        self.error.clear();
                    }
                }
                Err(error) => {
                    if edit {
                        let _ = context.event(GraphEvent::Cancel);
                    }
                    self.error = error;
                    self.moving = false;
                    break;
                }
            }
        }
    }
}
fn node_size(ui: &imgui::Ui, node: &NodeView) -> [f32; 2] {
    let ports_width = |ports: &[PortView]| {
        ports
            .iter()
            .map(|p| ui.calc_text_size(&p.label)[0] + 20.)
            .fold(0., f32::max)
    };
    let width = (ports_width(&node.inputs) + ports_width(&node.outputs) + 30.)
        .max(190.)
        .max(ui.calc_text_size(&node.label)[0])
        .max(ui.calc_text_size(&node.summary)[0]);
    [
        width,
        36. + node.inputs.len().max(node.outputs.len()).max(1) as f32 * 22.,
    ]
}
fn arrange(ui: &imgui::Ui, graph: &GraphView) -> Vec<(ObjectId, [f32; 2])> {
    let spacing = graph
        .nodes
        .iter()
        .map(|n| node_size(ui, n))
        .fold([260_f32, 160_f32], |s, n| {
            [s[0].max(n[0] + 70.), s[1].max(n[1] + 70.)]
        });
    super::layout(graph, spacing)
}
fn draw_socket(
    ui: &imgui::Ui,
    pin: &nodes::PinToken<'_>,
    label: &str,
    output: bool,
    connected: bool,
    color: [f32; 4],
) -> [f32; 2] {
    let _group = ui.begin_group();
    if output {
        ui.text_colored(color, label);
        ui.same_line();
    }
    let p = ui.cursor_screen_pos();
    let pivot = [p[0] + 5., p[1] + 7.];
    ui.get_window_draw_list()
        .add_circle(pivot, 4.5, color)
        .filled(connected)
        .thickness(1.5)
        .build();
    ui.dummy([10., 14.]);
    pin.pivot_rect(pivot, pivot);
    pin.pivot_alignment([0.5, 0.5]);
    if !output {
        ui.same_line();
        ui.text_colored(color, label);
    }
    pivot
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn dynamic_sockets_have_stable_noncolliding_native_ids() {
        let node = ObjectId::new();
        let port = |i| PortView {
            key: format!("port-{i}"),
            label: String::new(),
            color: GRAPH_COLORS.image,
        };
        let mut graph = GraphView {
            key: GraphKey {
                document: DocumentId::new(),
                group: None,
            },
            generation: 0,
            selected: vec![],
            has_parent: false,
            error: String::new(),
            nodes: vec![NodeView {
                id: node,
                label: String::new(),
                summary: String::new(),
                color: GRAPH_COLORS.image,
                position: None,
                inputs: (0..1100).map(port).collect(),
                outputs: (0..1100).map(port).collect(),
                can_open: false,
            }],
            wires: vec![WireView {
                source: Socket {
                    node,
                    key: "port-0".into(),
                },
                target: Socket {
                    node,
                    key: "port-1".into(),
                },
            }],
        };
        let mut ids = Ids::default();
        ids.register(&graph);
        let pins = ids.pins.clone();
        graph.nodes[0].inputs.reverse();
        graph.nodes[0].outputs.push(port(1100));
        ids.register(&graph);
        for (key, value) in pins {
            assert_eq!(ids.pins[&key], value);
        }
        let all: Vec<_> = ids
            .nodes
            .values()
            .chain(ids.pins.values())
            .chain(ids.links.values())
            .copied()
            .collect();
        assert_eq!(
            all.len(),
            all.iter().collect::<std::collections::BTreeSet<_>>().len()
        );
        assert!(!all.contains(&0));
        let wire = &graph.wires[0];
        let a = nodes::PinId::new(ids.pins[&(wire.source.clone(), true)]);
        let b = nodes::PinId::new(ids.pins[&(wire.target.clone(), false)]);
        assert_eq!(ids.connection(a, b).unwrap(), *wire);
        assert_eq!(ids.connection(b, a).unwrap(), *wire);
        assert!(ids.connection(a, a).is_err());
    }
}
