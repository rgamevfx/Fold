//! Compact vertical cards and main-pipe-preserving DAG arrangement.
use super::*;
use crate::sdk::{
    appearance::GraphText,
    imgui,
    typography::{self, TextRole},
};
use dear_node_editor as nodes;
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn text(mut text: GraphText) -> GraphText {
    // Compositing has its own compact presentation. Follow the shared spacing
    // scale, without inheriting Motion's deliberately large port/title sizes.
    text.titles = text.spacing * 15. / 18.;
    text.labels = text.titles;
    text
}

pub(super) fn size(ui: &imgui::Ui, node: &NodeView, sizes: GraphText) -> [f32; 2] {
    let _font = ui.push_font_with_size(None, sizes.spacing);
    let scale = ui.current_font_size() / 18.;
    let label = typography::text_size(ui, TextRole::Title, &node.label, text(sizes));
    [(100. * scale).max(label[0] + 20. * scale), 22. * scale]
}

/// Points are relative to the content rect; native node padding is four pixels.
pub(super) fn pin(
    node: &NodeView,
    key: &str,
    output: bool,
    primary: Option<&str>,
    size: [f32; 2],
    scale: f32,
) -> ([f32; 2], [f32; 2]) {
    let [w, h] = size;
    if output {
        return ([w * 0.5, h + 4. * scale], [0., 1.]);
    }
    if primary == Some(key) {
        return ([w * 0.5, -4. * scale], [0., -1.]);
    }
    let side: Vec<_> = node
        .inputs
        .iter()
        .filter(|p| Some(p.key.as_str()) != primary && p.key != "mask")
        .collect();
    if key == "mask" {
        return ([w + 4. * scale, h * 0.5], [1., 0.]);
    }
    let index = side.iter().position(|p| p.key == key).unwrap_or(0);
    (
        [
            -4. * scale,
            h * (index + 1) as f32 / (side.len() + 1) as f32,
        ],
        [-1., 0.],
    )
}

pub(super) fn layout(
    graph: &GraphView,
    spacing: [f32; 2],
    context: &impl GraphContext,
) -> Vec<(ObjectId, [f32; 2])> {
    let depths: BTreeMap<_, _> = super::layout(graph, [1., 1.])
        .into_iter()
        .map(|(id, p)| (id, p[0] as usize))
        .collect();
    let mut lanes = BTreeMap::new();
    let mut occupied = BTreeSet::new();
    let mut ordered: Vec<_> = graph.nodes.iter().collect();
    ordered.sort_by_key(|node| depths[&node.id]);
    for node in ordered {
        let depth = depths[&node.id];
        let primary = context.primary_input(node);
        let source = graph
            .wires
            .iter()
            .find(|w| w.target.node == node.id && Some(w.target.key.as_str()) == primary);
        let mut lane = source
            .and_then(|w| lanes.get(&w.source.node).copied())
            .unwrap_or(0);
        while occupied.contains(&(depth, lane)) {
            lane += 1;
        }
        occupied.insert((depth, lane));
        lanes.insert(node.id, lane);
    }
    graph
        .nodes
        .iter()
        .map(|n| {
            (
                n.id,
                [
                    lanes[&n.id] as f32 * spacing[0],
                    depths[&n.id] as f32 * spacing[1],
                ],
            )
        })
        .collect()
}

pub(super) fn draw(
    ui: &imgui::Ui,
    editor: &nodes::NodeEditorFrame<'_>,
    token: &nodes::NodeToken<'_>,
    node: &NodeView,
    graph: &GraphView,
    primary: Option<&str>,
    sizes: GraphText,
    show_ports: bool,
    disabled: bool,
    category_color: Option<[f32; 4]>,
    ids: &BTreeMap<(Socket, bool), usize>,
    pivots: &mut BTreeMap<usize, [f32; 2]>,
    directions: &mut BTreeMap<usize, [f32; 2]>,
) {
    let origin = ui.cursor_pos();
    let p = ui.cursor_screen_pos();
    let size = size(ui, node, sizes);
    let _geometry_font = ui.push_font_with_size(None, sizes.spacing);
    let scale = ui.current_font_size() / 18.;
    {
        let draw = ui.get_window_draw_list();
        if disabled {
            let mut strike = ui.style_color(imgui::StyleColor::TextDisabled);
            strike[3] = 0.75;
            draw.add_line(
                [p[0] + 4. * scale, p[1] + size[1]],
                [p[0] + size[0] - 4. * scale, p[1]],
                strike,
            )
            .thickness(1.5 * scale)
            .build();
        }
        draw.add_line(
            [p[0], p[1] + 2. * scale],
            [p[0], p[1] + size[1] - 2. * scale],
            if disabled {
                ui.style_color(imgui::StyleColor::TextDisabled)
            } else {
                category_color.unwrap_or(node.color)
            },
        )
        .thickness(3. * scale)
        .build();
        let _font = typography::push_role(ui, TextRole::Title, text(sizes));
        let label = ui.calc_text_size(&node.label);
        draw.add_text(
            [
                p[0] + (size[0] - label[0]) * 0.5,
                p[1] + (size[1] - label[1]) * 0.5,
            ],
            ui.style_color(if disabled {
                imgui::StyleColor::TextDisabled
            } else {
                imgui::StyleColor::Text
            }),
            &node.label,
        );
    }
    for (ports, output) in [(&node.inputs, false), (&node.outputs, true)] {
        for port in ports {
            let socket = Socket {
                node: node.id,
                key: port.key.clone(),
            };
            let raw = ids[&(socket.clone(), output)];
            let (offset, direction) = compact::pin(node, &port.key, output, primary, size, scale);
            let pivot = [p[0] + offset[0], p[1] + offset[1]];
            let connected = graph.wires.iter().any(|w| {
                if output {
                    w.source == socket
                } else {
                    w.target == socket
                }
            });
            // Set each native pin's tangent, including horizontal side inputs.
            let _direction = editor.push_style_var_vec2(
                if output {
                    nodes::StyleVar::SourceDirection
                } else {
                    nodes::StyleVar::TargetDirection
                },
                direction,
            );
            ui.set_cursor_pos(origin);
            token.pin(
                nodes::PinId::new(raw),
                if output {
                    nodes::PinKind::Output
                } else {
                    nodes::PinKind::Input
                },
                |pin| {
                    ui.dummy([0., 0.]);
                    pin.rect(
                        [pivot[0] - 6. * scale, pivot[1] - 6. * scale],
                        [pivot[0] + 6. * scale, pivot[1] + 6. * scale],
                    );
                    pin.pivot_rect(pivot, pivot);
                    ui.get_window_draw_list()
                        .add_circle(pivot, 3. * scale, port.color)
                        .filled(connected)
                        .thickness(1.3 * scale)
                        .build();
                },
            );
            if show_ports && !output {
                let label = match port.key.as_str() {
                    "foreground" => "A",
                    "background" => "B",
                    key => key,
                };
                let _font = typography::push_role(ui, TextRole::Title, text(sizes));
                let label_size = ui.calc_text_size(label);
                let label_pos = if direction[1] < 0. {
                    [pivot[0] + 8. * scale, pivot[1] - label_size[1]]
                } else if direction[0] < 0. {
                    [
                        pivot[0] - label_size[0] - 8. * scale,
                        pivot[1] - label_size[1] * 0.5,
                    ]
                } else {
                    [pivot[0] + 8. * scale, pivot[1] - label_size[1] * 0.5]
                };
                ui.get_window_draw_list().add_text(
                    label_pos,
                    ui.style_color(imgui::StyleColor::Text),
                    label,
                );
            }
            pivots.insert(raw, pivot);
            directions.insert(raw, direction);
        }
    }
    ui.set_cursor_pos(origin);
    ui.dummy(size);
}
