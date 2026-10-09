use super::{
    navigation::{HitMap, curve_intersects, directed_curve},
    *,
};
use crate::sdk::{
    GRAPH_COLORS,
    appearance::GraphText,
    imgui::{self, Key, MouseButton},
    toolbar::{ToolbarIcon, icon_button, icon_menu},
    typography::{self, TextRole},
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
    fn pin(&self, id: nodes::PinId) -> Option<&(Socket, bool)> {
        self.pins
            .iter()
            .find(|(_, raw)| **raw == id.raw())
            .map(|(key, _)| key)
    }
}
#[derive(Default)]
pub struct GraphCanvas {
    typography: Option<crate::sdk::typography::Typography>,
    editor: Option<nodes::EditorContext>,
    ids: Ids,
    key: Option<GraphKey>,
    generation: Option<u64>,
    selection: Vec<ObjectId>,
    fit: u8,
    size: [f32; 2],
    hit_map: HitMap,
    drop_transform: [[f32; 2]; 2],
    drop_bounds: Option<[[f32; 2]; 2]>,
    search: String,
    pub(super) popup_position: [f32; 2],
    context_node: Option<ObjectId>,
    context_input: Option<Socket>,
    last_click: Option<[f32; 2]>,
    cancel_create: bool,
    connecting: bool,
    insertion_choices: Vec<interactions::Insertion>,
    moving: bool,
    cancel_move: bool,
    error: String,
    #[cfg(test)]
    pub(super) framed: bool,
    #[cfg(test)]
    pub(super) raster_font: Option<imgui::FontId>,
    #[cfg(test)]
    pub(super) view: [[f32; 2]; 2],
    #[cfg(test)]
    pub(super) wire_probe: ([f32; 2], usize),
    #[cfg(test)]
    pub(super) pin_probes: Vec<(Socket, bool, [f32; 2])>,
    #[cfg(test)]
    pub(super) node_probes: Vec<(ObjectId, [f32; 2], [f32; 2])>,
    #[cfg(test)]
    pub(super) insertion_probe: bool,
    #[cfg(test)]
    pub(super) add_probe: [f32; 2],
    #[cfg(test)]
    pub(super) template_probe: [f32; 2],
    #[cfg(test)]
    pub(super) disconnect_probe: [f32; 2],
    #[cfg(test)]
    pub(super) choice_probes: Vec<[f32; 2]>,
}
impl GraphCanvas {
    pub fn initialize(
        &mut self,
        context: &imgui::Context,
        typography: Option<&crate::sdk::typography::Typography>,
    ) {
        self.typography = typography.cloned();
        let editor = nodes::EditorContext::create_with_config(
            context,
            nodes::EditorConfig::new()
                .no_settings_file()
                // Fractional trackpad deltas must not snap to discrete zoom levels.
                .smooth_zoom(true, 1.3)
                .navigate_button(MouseButton::Middle),
        );
        let mut style = editor.style();
        style.node_rounding = 6.;
        style.selected_node_border_width = 2.5;
        style.highlight_connected_links = 1.;
        editor.set_style(&style);
        self.editor = Some(editor);
        self.frame_all();
    }
    pub fn drop_bounds(&self) -> Option<[[f32; 2]; 2]> {
        self.drop_bounds
    }
    pub fn drop_transform(&self) -> [[f32; 2]; 2] {
        self.drop_transform
    }
    pub fn drop_position(&self, p: [f32; 2]) -> [f32; 2] {
        [0, 1].map(|i| self.drop_transform[0][i] + p[i] * self.drop_transform[1][i])
    }
    pub fn frame_all(&mut self) {
        self.fit = 2;
    }
    /// Clear stale event-time geometry when the panel has no graph to draw.
    pub fn clear_hit_map(&mut self) {
        self.hit_map.clear();
        self.drop_bounds = None;
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
        self.drop_bounds = None;
        let mut graph = context.graph();
        let changed = self.key != Some(graph.key);
        if changed {
            self.key = Some(graph.key);
            self.frame_all();
            self.moving = false;
            self.cancel_move = false;
            self.error.clear();
            self.last_click = None;
            self.insertion_choices.clear();
        }
        if self.generation != Some(graph.generation) {
            // An external revision wins over the in-flight gesture. Ignore the
            // remaining physical drag rather than committing stale positions.
            self.insertion_choices.clear();
            self.cancel_create |= self.connecting;
            self.cancel_move |= self.moving;
            self.moving = false;
        }
        if ui.is_key_pressed(Key::Escape)
            && (ui.is_window_focused() || !self.insertion_choices.is_empty())
        {
            self.error = context.event(GraphEvent::Cancel).err().unwrap_or_default();
            graph = context.graph();
            self.moving = false;
            self.cancel_move = true;
            self.cancel_create = true;
            self.insertion_choices.clear();
        }
        // Editor selections can include an owning scene object. Native nodes are
        // only a projection of that selection, never a reason to publish an edit.
        graph
            .selected
            .retain(|id| graph.nodes.iter().any(|node| node.id == *id));
        let text = self
            .typography
            .as_ref()
            .map(|t| t.appearance().graph)
            .unwrap_or_default();
        let vertical = context.presentation() == GraphPresentation::VerticalCompact;
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
        let mut arrange_nodes = false;
        if !compact {
            open_search = icon_button(ui, "add-node", ToolbarIcon::Add, "Add node (Tab)");
            #[cfg(test)]
            {
                self.add_probe =
                    [0, 1].map(|i| (ui.item_rect_min()[i] + ui.item_rect_max()[i]) * 0.5);
            }
            ui.same_line();
            if icon_button(
                ui,
                "frame-all",
                ToolbarIcon::FrameAll,
                "Frame all (F)\nDrag background: pan\nShift+drag: box select\nScroll: zoom\nDelete/Backspace: remove selection\nRight-click a wire: Disconnect",
            ) {
                self.fit = 1;
            }
            ui.same_line();
            arrange_nodes = icon_button(ui, "arrange", ToolbarIcon::Arrange, "Arrange nodes (L)");
        }
        if compact
            && let Some(_menu) = icon_menu(ui, "graph-actions", "Graph actions and navigation help")
        {
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
            arrange_nodes = ui.menu_item("Arrange nodes (L)");
            ui.separator();
            ui.text_disabled("Drag background: pan");
            ui.text_disabled("Shift+drag: box select");
            ui.text_disabled("Scroll: zoom");
            ui.text_disabled("Delete/Backspace: remove selection\nRight-click a wire: Disconnect");
        }
        if !graph.error.is_empty() {
            ui.text_colored(GRAPH_COLORS.invalid, &graph.error);
        }
        if !self.error.is_empty() && self.error != graph.error {
            ui.text_colored(GRAPH_COLORS.invalid, &self.error);
        }
        if graph.nodes.iter().any(|n| n.position.is_none()) {
            let layout: BTreeMap<_, _> = arrange(ui, &graph, text, context).into_iter().collect();
            for node in &mut graph.nodes {
                node.position.get_or_insert(layout[&node.id]);
            }
        }
        self.ids.register(&graph);
        let _layout_font = ui.push_font_with_size(None, text.spacing);
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
        let colors = crate::sdk::EditorColors::from_ui(ui);
        native.set_style_color(nodes::StyleColor::Background, colors.background);
        let mut grid = colors.grid;
        grid[3] *= 0.2;
        let mut surface = ui.style_color(imgui::StyleColor::PopupBg);
        surface[3] = 1.;
        native.set_style_color(nodes::StyleColor::Grid, grid);
        native.set_style_color(nodes::StyleColor::NodeBackground, surface);
        native.set_style_color(nodes::StyleColor::NodeBorder, colors.grid);
        native.set_style_color(nodes::StyleColor::SelectedNodeBorder, colors.selected);
        native.set_style_color(nodes::StyleColor::HoveredNodeBorder, colors.selected);
        native.set_style_color(nodes::StyleColor::HoveredLinkBorder, colors.selected);
        native.set_style_color(nodes::StyleColor::SelectedLinkBorder, colors.selected);
        let mut connected = colors.selected;
        connected[3] *= 0.35;
        native.set_style_color(nodes::StyleColor::HighlightLinkBorder, connected);
        let unit = ui.current_font_size();
        let mut style = native.style();
        style.node_padding = [if vertical {
            unit * 4. / 18.
        } else {
            unit * 0.65
        }; 4];
        style.node_rounding = if vertical {
            unit * 3. / 18.
        } else {
            unit * 0.4
        };
        style.source_direction = if vertical { [0., 1.] } else { [1., 0.] };
        style.target_direction = if vertical { [0., -1.] } else { [-1., 0.] };
        native.set_style(&style);
        let origin = ui.cursor_screen_pos();
        if ui.is_window_hovered() && !ui.io().want_text_input() && !ui.is_any_item_active() {
            self.hit_map.bounds = Some([origin, [origin[0] + size[0], origin[1] + size[1]]]);
        }
        // Begin temporarily transforms ImGui's pointer into canvas space.
        // Capture screen coordinates first, then convert exactly once.
        let pointer = ui.io().mouse_pos();
        self.drop_bounds = Some([origin, [origin[0] + size[0], origin[1] + size[1]]]);
        let editor = ui.node_editor(native, "fold-graph-canvas", size);
        let zero = editor.screen_to_canvas([0., 0.]);
        let basis = editor.screen_to_canvas([256., 256.]);
        self.drop_transform = [
            zero,
            [(basis[0] - zero[0]) / 256., (basis[1] - zero[1]) / 256.],
        ];
        let pointer_inside =
            (0..2).all(|i| pointer[i] >= origin[i] && pointer[i] < origin[i] + size[i]);
        if pointer_inside
            && ui.is_window_hovered()
            && (ui.is_mouse_clicked(MouseButton::Left)
                || ui.is_mouse_clicked(MouseButton::Right)
                || ui.is_mouse_clicked(MouseButton::Middle))
        {
            self.last_click = Some(editor.screen_to_canvas(pointer));
        }
        // Native GetCurrentZoom reports inverse scale. Use the actual canvas
        // transform so raster density and detail thresholds track magnification.
        let zero = editor.canvas_to_screen([0., 0.]);
        let hundred = editor.canvas_to_screen([100., 0.]);
        // A longer baseline avoids cancellation at font-density boundaries.
        let zoom = (hundred[0] - zero[0]) / 100.;
        // Density changes glyph coverage only, not layout size. The native
        // canvas still owns geometry transforms, clipping and node draw order.
        let _font = self
            .typography
            .as_ref()
            .map(|fonts| ui.push_font_with_size(Some(fonts.canvas_font(zoom)), 0.));
        #[cfg(test)]
        {
            self.raster_font = Some(ui.current_font());
        }
        let details = !text.hide_details
            || typography::text_size(ui, TextRole::Body, "Ag", text)[1] * zoom >= 8.;
        let header = colors.header;
        let header_height = header_height(ui, text);
        let mut pivots = BTreeMap::new();
        let mut directions = BTreeMap::new();
        for node in &graph.nodes {
            let id = nodes::NodeId::new(self.ids.nodes[&node.id]);
            if reset {
                editor.set_node_position(id, node.position.unwrap());
            }
            editor.node(id, |token| {
                if vertical {
                    compact::draw(
                        ui,
                        &editor,
                        token,
                        node,
                        &graph,
                        context.primary_input(node),
                        text,
                        self.connecting,
                        &self.ids.pins,
                        &mut pivots,
                        &mut directions,
                    );
                    return;
                }
                let origin = ui.cursor_pos();
                let p = ui.cursor_screen_pos();
                let [width, height] = node_size(ui, node, text);
                {
                    let draw = ui.get_window_draw_list();
                    draw.add_rect(
                        [p[0] - unit * 0.25, p[1] - unit * 0.25],
                        [
                            p[0] + width + unit * 0.25,
                            p[1] + header_height - unit * 0.6,
                        ],
                        header,
                    )
                    .filled(true)
                    .rounding(unit * 0.2)
                    .build();
                    draw.add_line(
                        [p[0], p[1] + header_height - unit * 0.55],
                        [p[0] + width, p[1] + header_height - unit * 0.55],
                        node.color,
                    )
                    .thickness(unit * 0.12)
                    .build();
                }
                label(ui, TextRole::Title, &node.label, colors.text, true, text);
                for (ports, output) in [(&node.inputs, false), (&node.outputs, true)] {
                    for (i, port) in ports.iter().enumerate() {
                        let socket = Socket {
                            node: node.id,
                            key: port.key.clone(),
                        };
                        let raw = self.ids.pins[&(socket.clone(), output)];
                        let x = if output {
                            width
                                - typography::text_size(ui, TextRole::Body, &port.label, text)[0]
                                - unit * 1.3
                        } else {
                            0.
                        };
                        ui.set_cursor_pos([
                            origin[0] + x,
                            origin[1] + header_height + port_row_height(ui, text) * i as f32,
                        ]);
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
                            |pin| draw_socket(ui, pin, port, output, connected, details, text),
                        );
                        pivots.insert(raw, pivot);
                        directions.insert(raw, if output { [1., 0.] } else { [-1., 0.] });
                    }
                }
                ui.set_cursor_pos([origin[0], origin[1] + height]);
                if !node.summary.is_empty() {
                    label(
                        ui,
                        TextRole::Secondary,
                        &node.summary,
                        colors.muted,
                        !text.hide_details || unit * zoom >= 11.,
                        text,
                    );
                }
                ui.dummy([width, unit * 0.15]);
            });
            let p = editor.node_position(id);
            let s = editor.node_size(id);
            let socket_radius = unit * 0.4;
            self.hit_map.nodes.push([
                editor.canvas_to_screen([p[0] - socket_radius, p[1] - socket_radius]),
                editor.canvas_to_screen([p[0] + s[0] + socket_radius, p[1] + s[1] + socket_radius]),
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
        let mut wire_curves = Vec::new();
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
            let curve = directed_curve(
                a,
                b,
                directions[&source],
                directions[&target],
                native.style().link_strength,
            )
            .map(|p| editor.canvas_to_screen(p));
            self.hit_map.links.push(curve);
            wire_curves.push((wire, curve));
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
        if let Some(create) = editor.begin_create(colors.selected, 2.5) {
            self.connecting = true;
            if self.cancel_create {
                create.reject_new_item();
            } else if let Some((a, b)) = create.query_new_link() {
                let result = self
                    .ids
                    .pin(a)
                    .zip(self.ids.pin(b))
                    .ok_or_else(|| "Choose an output and an input socket".to_string())
                    .and_then(|(a, b)| interactions::connection(&graph, a, b))
                    .and_then(|change| {
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
            } else if let Some(pin) = create.query_new_node() {
                // A connected input dragged onto empty canvas detaches on drop;
                // keep the old wire intact throughout the preview/cancel phase.
                if let Some((socket, false)) = self.ids.pin(pin)
                    && graph.wires.iter().any(|w| w.target == *socket)
                {
                    let change = GraphChange::Disconnect(socket.clone());
                    if context.validate(&change).is_ok() && create.accept_new_item() {
                        actions.push(change);
                    }
                } else {
                    create.reject_new_item();
                }
            }
        } else {
            self.connecting = false;
            if !ui.is_mouse_down(MouseButton::Left) {
                self.cancel_create = false;
            }
        }
        let active = ui.is_window_focused() && !ui.io().want_text_input();
        editor.set_shortcuts_enabled(active);
        if active && ui.is_key_pressed_with_repeat(Key::Backspace, false) {
            for link in editor.selected_links() {
                editor.delete_link(link);
            }
            for node in editor.selected_nodes() {
                editor.delete_node(node);
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
            self.pin_probes = self
                .ids
                .pins
                .iter()
                .filter_map(|((socket, output), id)| {
                    pivots
                        .get(id)
                        .map(|p| (socket.clone(), *output, editor.canvas_to_screen(*p)))
                })
                .collect();
            self.node_probes = graph
                .nodes
                .iter()
                .map(|node| {
                    let id = nodes::NodeId::new(self.ids.nodes[&node.id]);
                    let p = editor.node_position(id);
                    let s = editor.node_size(id);
                    (
                        node.id,
                        editor.canvas_to_screen([p[0] + s[0] * 0.5, p[1] + s[1] * 0.5]),
                        s,
                    )
                })
                .collect();
            self.insertion_probe = false;
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
        let reselected = ui.is_mouse_clicked(MouseButton::Left)
            && editor
                .hovered_node()
                .is_some_and(|node| selected.iter().any(|id| self.ids.nodes[id] == node.raw()));
        let changed = selected.len() != graph.selected.len()
            || selected.iter().any(|id| !graph.selected.contains(id));
        if changed || reselected {
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
        if active && ui.is_key_pressed(Key::F) {
            self.fit = 1;
        }
        arrange_nodes |= active
            && !ui.io().key_ctrl()
            && !ui.io().key_alt()
            && !ui.io().key_super()
            && ui.is_key_pressed_with_repeat(Key::L, false);
        let tab_search = active && ui.is_key_pressed(Key::Tab);
        open_search |= tab_search;
        let node_context = editor.show_node_context_menu().and_then(|id| {
            graph
                .nodes
                .iter()
                .find(|n| self.ids.nodes[&n.id] == id.raw())
                .map(|n| n.id)
        });
        let background_search = editor.show_background_context_menu();
        open_search |= background_search;
        if open_search {
            self.popup_position = if background_search || (tab_search && pointer_inside) {
                editor.screen_to_canvas(pointer)
            } else {
                self.last_click.unwrap_or_else(|| {
                    editor.screen_to_canvas([origin[0] + size[0] * 0.5, origin[1] + size[1] * 0.5])
                })
            };
        }
        let wire_context = editor
            .show_link_context_menu()
            .and_then(|id| {
                graph
                    .wires
                    .iter()
                    .find(|w| self.ids.links[&w.target] == id.raw())
                    .map(|w| w.target.clone())
            })
            .or_else(|| {
                editor
                    .show_pin_context_menu()
                    .and_then(|id| self.ids.pin(id))
                    .filter(|(_, output)| !output)
                    .map(|(socket, _)| socket.clone())
            });
        let mut current_positions = graph
            .nodes
            .iter()
            .map(|n| (n.id, n.position.unwrap()))
            .collect::<Vec<_>>();
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
                events.push(GraphEvent::PreviewPositions(positions.clone()));
            }
            current_positions = positions;
        }
        if self.fit > 0 {
            if self.fit == 1 {
                // Compact cards should not balloon to fill a nearly empty
                // panel. Center small networks at their current magnification.
                let bounds = graph.nodes.iter().fold(
                    [[f32::INFINITY; 2], [f32::NEG_INFINITY; 2]],
                    |mut bounds, node| {
                        let id = nodes::NodeId::new(self.ids.nodes[&node.id]);
                        let p = editor.node_position(id);
                        let s = editor.node_size(id);
                        for i in 0..2 {
                            bounds[0][i] = bounds[0][i].min(p[i]);
                            bounds[1][i] = bounds[1][i].max(p[i] + s[i]);
                        }
                        bounds
                    },
                );
                if vertical
                    && !graph.nodes.is_empty()
                    && (0..2).all(|i| (bounds[1][i] - bounds[0][i]) * zoom * 1.1 < size[i])
                {
                    let nodes = editor.selected_nodes();
                    let links = editor.selected_links();
                    editor.clear_selection();
                    for node in &graph.nodes {
                        editor.add_node_to_selection(nodes::NodeId::new(self.ids.nodes[&node.id]));
                    }
                    editor.navigate_to_selection(false, 0.15);
                    editor.clear_selection();
                    for node in nodes {
                        editor.add_node_to_selection(node);
                    }
                    for link in links {
                        editor.add_link_to_selection(link);
                    }
                } else {
                    editor.navigate_to_content(0.15);
                }
            }
            self.fit -= 1;
        }
        let mut open_insertion = false;
        if self.moving
            && !self.cancel_move
            && self.selection.len() == 1
            && self.insertion_choices.is_empty()
            && actions.is_empty()
            && let Some(node) = graph.nodes.iter().find(|n| n.id == self.selection[0])
        {
            let id = nodes::NodeId::new(self.ids.nodes[&node.id]);
            let p = editor.node_position(id);
            let s = editor.node_size(id);
            let body = [
                editor.canvas_to_screen([p[0] + s[0] * 0.15, p[1] + s[1] * 0.15]),
                editor.canvas_to_screen([p[0] + s[0] * 0.85, p[1] + s[1] * 0.85]),
            ];
            for (wire, curve) in &wire_curves {
                if !curve_intersects(*curve, body) {
                    continue;
                }
                let choices = interactions::insertions(context, &graph, node, wire);
                if choices.is_empty() {
                    continue;
                }
                #[cfg(test)]
                {
                    self.insertion_probe = true;
                }
                let source = self.ids.pins[&(wire.source.clone(), true)];
                let target = self.ids.pins[&(wire.target.clone(), false)];
                editor.link_colored(
                    nodes::LinkId::new(self.ids.links[&wire.target]),
                    nodes::PinId::new(source),
                    nodes::PinId::new(target),
                    colors.selected,
                    4.,
                );
                if choices.len() == 1 {
                    let choice = &choices[0];
                    for (a, b) in [
                        (source, self.ids.pins[&(choice.input.clone(), false)]),
                        (self.ids.pins[&(choice.output.clone(), true)], target),
                    ] {
                        let c = directed_curve(
                            pivots[&a],
                            pivots[&b],
                            directions[&a],
                            directions[&b],
                            native.style().link_strength,
                        );
                        ui.get_window_draw_list()
                            .add_bezier_curve(c[0], c[1], c[2], c[3], colors.selected)
                            .thickness(2.5)
                            .build();
                    }
                }
                if ui.is_mouse_released(MouseButton::Left) {
                    if choices.len() == 1 {
                        actions.push(choices[0].change.clone());
                    } else {
                        self.insertion_choices = choices;
                        open_insertion = true;
                    }
                }
                break;
            }
        }
        {
            let _suspend = editor.suspend();
            if open_insertion {
                ui.open_popup("Insert into connection");
            }
            if !self.insertion_choices.is_empty() {
                if let Some(_popup) = ui.begin_popup("Insert into connection") {
                    ui.text("Choose input → output");
                    let mut chosen = None;
                    #[cfg(test)]
                    {
                        self.choice_probes.clear();
                    }
                    for (index, choice) in self.insertion_choices.iter().enumerate() {
                        let _id = ui.push_id(index as i32);
                        if ui.menu_item(&choice.label) {
                            chosen = Some(index);
                        }
                        #[cfg(test)]
                        {
                            self.choice_probes
                                .push([0, 1].map(|i| {
                                    (ui.item_rect_min()[i] + ui.item_rect_max()[i]) * 0.5
                                }));
                        }
                    }
                    if let Some(index) = chosen {
                        actions.push(self.insertion_choices[index].change.clone());
                        self.insertion_choices.clear();
                        ui.close_current_popup();
                    }
                } else {
                    self.insertion_choices.clear();
                    self.moving = false;
                    events.push(GraphEvent::Cancel);
                }
            }
            if let Some(socket) = wire_context {
                self.context_input = Some(socket);
                ui.open_popup("Connection actions");
            }
            if let Some(_popup) = ui.begin_popup("Connection actions")
                && let Some(socket) = &self.context_input
            {
                let connected = graph.wires.iter().any(|w| &w.target == socket);
                let _disabled = ui.begin_disabled_with_cond(!connected);
                if ui.menu_item("Disconnect") {
                    actions.push(GraphChange::Disconnect(socket.clone()));
                }
                #[cfg(test)]
                {
                    self.disconnect_probe =
                        [0, 1].map(|i| (ui.item_rect_min()[i] + ui.item_rect_max()[i]) * 0.5);
                }
            }
            if let Some(pin) = editor.hovered_pin().and_then(|id| self.ids.pin(id))
                && let Some(node) = graph.nodes.iter().find(|n| n.id == pin.0.node)
                && let Some(port) = (if pin.1 { &node.outputs } else { &node.inputs })
                    .iter()
                    .find(|p| p.key == pin.0.key)
            {
                ui.tooltip_text(if !pin.1 && graph.wires.iter().any(|w| w.target == pin.0) {
                    format!(
                        "{}\nDrag to empty space to disconnect; Escape cancels",
                        port.label
                    )
                } else {
                    port.label.clone()
                });
            }
            if !self.moving
                && !self.connecting
                && editor.hovered_pin().is_none()
                && let Some(id) = editor.hovered_node()
                && let Some(node) = graph
                    .nodes
                    .iter()
                    .find(|n| self.ids.nodes[&n.id] == id.raw())
                && !node.summary.is_empty()
            {
                ui.tooltip_text(format!("{}\n{}", node.label, node.summary));
            }
            if let Some(node) = node_context {
                self.context_node = Some(node);
                ui.open_popup("Node actions");
            }
            if let Some(_popup) = ui.begin_popup("Node actions")
                && let Some(node) = self.context_node
                && let Some(view) = graph.nodes.iter().find(|n| n.id == node)
            {
                if view.can_open && ui.menu_item("Edit contents") {
                    events.push(GraphEvent::Navigate(Some(node)));
                }
                for (action, label) in context.node_actions(node) {
                    if ui.menu_item(label) {
                        if let Err(error) = context.node_action(node, &action) {
                            self.error = error;
                        }
                    }
                }
            }
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
                        #[cfg(test)]
                        {
                            self.template_probe = [0, 1]
                                .map(|i| (ui.item_rect_min()[i] + ui.item_rect_max()[i]) * 0.5);
                        }
                    }
                }
            }
        }
        drop(_font);
        editor.end();
        drop(_layout_font);
        if arrange_nodes {
            actions.push(GraphChange::Positions(arrange(ui, &graph, text, context)));
        }
        if !ui.is_mouse_down(MouseButton::Left) {
            self.cancel_move = false;
        }
        if !actions.is_empty() {
            // Preserve presentation-only layout for old documents at the first
            // structural edit. An explicit Arrange remains last and wins.
            actions.insert(0, GraphChange::Positions(current_positions));
            events.push(GraphEvent::Commit(actions));
            self.moving = false;
        } else if self.moving
            && self.insertion_choices.is_empty()
            && !ui.is_mouse_down(MouseButton::Left)
            && !ui.is_any_item_active()
        {
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
// Geometry uses an independent spacing unit; text only enlarges its own row or
// column when necessary to avoid overlap. Font size never scales the sockets.
fn port_row_height(ui: &imgui::Ui, text: GraphText) -> f32 {
    (ui.current_font_size() * 1.4).max(typography::text_size(ui, TextRole::Body, "Ag", text)[1])
}
fn header_height(ui: &imgui::Ui, text: GraphText) -> f32 {
    typography::text_size(ui, TextRole::Title, "Ag", text)[1] + ui.current_font_size() * 0.9
}
fn node_size(ui: &imgui::Ui, node: &NodeView, text: GraphText) -> [f32; 2] {
    let _font = ui.push_font_with_size(None, text.spacing);
    let unit = ui.current_font_size();
    let ports_width = |ports: &[PortView]| {
        ports
            .iter()
            .map(|p| typography::text_size(ui, TextRole::Body, &p.label, text)[0] + unit * 1.3)
            .fold(0., f32::max)
    };
    let width = (ports_width(&node.inputs) + ports_width(&node.outputs) + unit * 0.8)
        .max(unit * 10.)
        .max(typography::text_size(ui, TextRole::Title, &node.label, text)[0] + unit)
        .max(typography::text_size(ui, TextRole::Secondary, &node.summary, text)[0]);
    [
        width,
        header_height(ui, text)
            + unit * 0.25
            + port_row_height(ui, text) * node.inputs.len().max(node.outputs.len()).max(1) as f32,
    ]
}
fn arrange(
    ui: &imgui::Ui,
    graph: &GraphView,
    text: GraphText,
    context: &impl GraphContext,
) -> Vec<(ObjectId, [f32; 2])> {
    if context.presentation() == GraphPresentation::VerticalCompact {
        let _geometry_font = ui.push_font_with_size(None, text.spacing);
        let scale = ui.current_font_size() / 18.;
        let width = graph
            .nodes
            .iter()
            .map(|n| compact::size(ui, n, text)[0])
            .fold(110. * scale, f32::max);
        return compact::layout(graph, [width + 70. * scale, 78. * scale], context);
    }
    let spacing = graph
        .nodes
        .iter()
        .map(|n| node_size(ui, n, text))
        .fold([260_f32, 160_f32], |s, n| {
            [s[0].max(n[0] + 70.), s[1].max(n[1] + 70.)]
        });
    super::layout(graph, spacing)
}
fn draw_socket(
    ui: &imgui::Ui,
    pin: &nodes::PinToken<'_>,
    port: &PortView,
    output: bool,
    connected: bool,
    visible: bool,
    sizes: GraphText,
) -> [f32; 2] {
    let unit = ui.current_font_size();
    let start = ui.cursor_screen_pos();
    let text_size = typography::text_size(ui, TextRole::Body, &port.label, sizes);
    let row_height = port_row_height(ui, sizes);
    let row_width = text_size[0] + unit * 1.3;
    let pivot = [
        if output {
            start[0] + row_width + unit * 0.65
        } else {
            start[0] - unit * 0.65
        },
        start[1] + row_height * 0.5,
    ];
    ui.get_window_draw_list()
        .add_circle(pivot, unit * 0.3, port.color)
        .num_segments(std::num::NonZeroUsize::new(32).unwrap())
        .filled(connected)
        .thickness(1.5)
        .build();
    if visible {
        let _font = typography::push_role(ui, TextRole::Body, sizes);
        ui.get_window_draw_list().add_text(
            [
                start[0] + if output { 0. } else { unit * 1.3 },
                start[1] + (row_height - text_size[1]) * 0.5,
            ],
            ui.style_color(imgui::StyleColor::Text),
            &port.label,
        );
    }
    ui.dummy([row_width, row_height]);
    pin.rect(
        [start[0].min(pivot[0] - unit * 0.5), start[1]],
        [
            (start[0] + row_width).max(pivot[0] + unit * 0.5),
            start[1] + row_height,
        ],
    );
    pin.pivot_rect(pivot, pivot);
    // Explicit pivots keep wire endpoints independent of label bounds.
    pivot
}

fn label(
    ui: &imgui::Ui,
    role: TextRole,
    text: &str,
    color: [f32; 4],
    visible: bool,
    sizes: GraphText,
) {
    let _role = typography::push_role(ui, role, sizes);
    if visible {
        ui.get_window_draw_list()
            .add_text(ui.cursor_screen_pos(), color, text);
    }
    // Keep bounds and pins stationary when overview detail disappears.
    ui.dummy(ui.calc_text_size(text));
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn port_text_uses_available_space_before_growing_the_node() {
        let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        crate::sdk::typography::Typography::install(&mut context);
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([800., 600.]);
        context.io_mut().set_delta_time(1. / 60.);
        let port = |label: &str| PortView {
            key: label.into(),
            label: label.into(),
            color: GRAPH_COLORS.image,
        };
        let node = NodeView {
            id: ObjectId::new(),
            label: "Text".into(),
            summary: String::new(),
            color: GRAPH_COLORS.image,
            position: None,
            inputs: vec![port("In")],
            outputs: vec![port("Out")],
            can_open: false,
        };
        let ui = context.frame();
        ui.window("layout").build(|| {
            let mut text = GraphText::default();
            text.labels = 12.;
            let compact = node_size(ui, &node, text);
            for labels in [18., 22., 24.] {
                text.labels = labels;
                assert_eq!(
                    node_size(ui, &node, text),
                    compact,
                    "label size must not scale the card when text fits"
                );
            }
            text.spacing = 24.;
            let roomy = node_size(ui, &node, text);
            assert!(roomy[0] > compact[0] && roomy[1] > compact[1]);
            text.spacing = 18.;
            text.labels = 28.;
            assert!(
                node_size(ui, &node, text)[1] > compact[1],
                "oversized labels still reserve enough vertical room"
            );
        });
        drop(context.render_legacy());
    }

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
        for (a, b) in [(a, b), (b, a)] {
            let change =
                interactions::connection(&graph, ids.pin(a).unwrap(), ids.pin(b).unwrap()).unwrap();
            assert!(matches!(change, GraphChange::Connect(actual) if actual == *wire));
        }
        assert!(
            interactions::connection(&graph, ids.pin(a).unwrap(), ids.pin(a).unwrap()).is_err()
        );
    }
}
