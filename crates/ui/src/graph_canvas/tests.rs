use super::*;
use crate::sdk::{CanvasPan, GRAPH_COLORS, imgui};

struct Context {
    graph: GraphView,
    original: GraphView,
    commits: usize,
    previews: usize,
    cancels: usize,
}
impl Context {
    fn new() -> Self {
        let mut nodes = Vec::new();
        let mut wires = Vec::new();
        for i in 0..8 {
            let id = ObjectId::new();
            if let Some(previous) = nodes.last().map(|n: &NodeView| n.id) {
                wires.push(WireView {
                    source: Socket {
                        node: previous,
                        key: "image".into(),
                    },
                    target: Socket {
                        node: id,
                        key: "image".into(),
                    },
                });
            }
            let port = || PortView {
                key: "image".into(),
                label: "image".into(),
                color: GRAPH_COLORS.image,
            };
            nodes.push(NodeView {
                id,
                label: format!("Node {i}"),
                summary: "Image operation".into(),
                color: GRAPH_COLORS.image,
                position: None,
                inputs: if i == 0 { vec![] } else { vec![port()] },
                outputs: if i == 7 { vec![] } else { vec![port()] },
                can_open: false,
            });
        }
        let graph = GraphView {
            key: GraphKey {
                document: DocumentId::new(),
                group: None,
            },
            generation: 0,
            selected: vec![nodes[0].id],
            nodes,
            wires,
            has_parent: false,
            error: String::new(),
        };
        Self {
            original: graph.clone(),
            graph,
            commits: 0,
            previews: 0,
            cancels: 0,
        }
    }
}
impl GraphContext for Context {
    fn graph(&self) -> GraphView {
        self.graph.clone()
    }
    fn catalog(&self) -> Vec<NodeTemplate> {
        vec![NodeTemplate {
            key: "test.node".into(),
            label: "Test node".into(),
            category: "Tests".into(),
        }]
    }
    fn validate(&self, _: &GraphChange) -> Result<(), String> {
        Ok(())
    }
    fn event(&mut self, event: GraphEvent) -> Result<(), String> {
        match event {
            GraphEvent::Select(ids) => self.graph.selected = ids,
            GraphEvent::PreviewPositions(positions) => {
                self.previews += 1;
                for (id, p) in positions {
                    self.graph
                        .nodes
                        .iter_mut()
                        .find(|n| n.id == id)
                        .unwrap()
                        .position = Some(p);
                }
            }
            GraphEvent::Commit(_) => {
                self.commits += 1;
                self.original = self.graph.clone();
                self.graph.generation += 1;
            }
            GraphEvent::Cancel => {
                self.cancels += 1;
                self.graph.nodes = self.original.nodes.clone();
                self.graph.generation += 1;
            }
            GraphEvent::Navigate(_) => {}
        }
        Ok(())
    }
}
fn frame(imgui: &mut imgui::Context, canvas: &mut GraphCanvas, graph: &mut Context) {
    let ui = imgui.frame();
    ui.window("graph")
        .position([0.; 2], imgui::Condition::Always)
        .size([850., 800.], imgui::Condition::Always)
        .build(|| canvas.draw(ui, graph));
    assert!(imgui.render_legacy().total_vtx_count() > 100);
}
fn frames(imgui: &mut imgui::Context, canvas: &mut GraphCanvas, graph: &mut Context, count: usize) {
    for _ in 0..count {
        frame(imgui, canvas, graph);
    }
}
fn scale(canvas: &GraphCanvas) -> f32 {
    canvas.view[1][0] - canvas.view[0][0]
}

#[test]
fn native_navigation_selection_and_gesture_lifetime_are_shared() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = GraphCanvas::default();
    canvas.initialize(&context);
    let mut graph = Context::new();
    frames(&mut context, &mut canvas, &mut graph, 20);
    assert!(canvas.framed, "all nodes must fit on first appearance");
    context.io_mut().add_mouse_pos_event([420., 650.]);
    frames(&mut context, &mut canvas, &mut graph, 3);
    for delta in [1_f32, -1., 0.2, -0.2] {
        let before = scale(&canvas);
        for _ in 0..if delta.abs() < 1. { 12 } else { 1 } {
            context.io_mut().add_mouse_wheel_event([0., delta]);
            frame(&mut context, &mut canvas, &mut graph);
        }
        frames(&mut context, &mut canvas, &mut graph, 20);
        assert!(
            (scale(&canvas) - before) * delta > 0.2,
            "continuous zoom {delta}: {before} -> {}",
            scale(&canvas)
        );
    }
    assert!(canvas.framed);
    let mut pan = CanvasPan::default();
    let before = canvas.view[0];
    let start = [420., 650.];
    context.io_mut().add_mouse_pos_event(start);
    frames(&mut context, &mut canvas, &mut graph, 3);
    assert!(canvas.accepts_background_pan(start));
    let button = pan.left_button(true, canvas.accepts_background_pan(start));
    assert_eq!(button, imgui::MouseButton::Middle);
    context.io_mut().add_mouse_button_event(button, true);
    frames(&mut context, &mut canvas, &mut graph, 3);
    for p in [[460., 675.], [490., 685.]] {
        context.io_mut().add_mouse_pos_event(p);
        frames(&mut context, &mut canvas, &mut graph, 3);
    }
    context
        .io_mut()
        .add_mouse_button_event(pan.left_button(false, false), false);
    frames(&mut context, &mut canvas, &mut graph, 3);
    assert!((canvas.view[0][0] - before[0] - 70.).abs() < 2.);
    assert!((canvas.view[0][1] - before[1] - 35.).abs() < 2.);
    let before = canvas.view[0];
    context.io_mut().add_mouse_pos_event(start);
    frames(&mut context, &mut canvas, &mut graph, 3);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Middle, true);
    frames(&mut context, &mut canvas, &mut graph, 3);
    context.io_mut().add_mouse_pos_event([380., 640.]);
    frames(&mut context, &mut canvas, &mut graph, 3);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Middle, false);
    frames(&mut context, &mut canvas, &mut graph, 3);
    assert!((canvas.view[0][0] - before[0] + 40.).abs() < 2.);
    assert!((canvas.view[0][1] - before[1] + 10.).abs() < 2.);
    let wire = canvas.wire_probe.0;
    assert!(!canvas.accepts_background_pan(wire));
    context.io_mut().add_mouse_pos_event(wire);
    frames(&mut context, &mut canvas, &mut graph, 3);
    let button = pan.left_button(true, canvas.accepts_background_pan(wire));
    assert_eq!(button, imgui::MouseButton::Left);
    context.io_mut().add_mouse_button_event(button, true);
    frames(&mut context, &mut canvas, &mut graph, 3);
    context
        .io_mut()
        .add_mouse_button_event(pan.left_button(false, false), false);
    frames(&mut context, &mut canvas, &mut graph, 3);
    assert_eq!(canvas.wire_probe.1, 1, "wire click selects, never pans");
    let before = canvas.view;
    pan.shift(true);
    context.io_mut().add_key_event(imgui::Key::ModShift, true);
    context.io_mut().add_mouse_pos_event([40., 300.]);
    frames(&mut context, &mut canvas, &mut graph, 3);
    let button = pan.left_button(true, canvas.accepts_background_pan([40., 300.]));
    assert_eq!(button, imgui::MouseButton::Left);
    context
        .io_mut()
        .add_key_event(imgui::Key::ModShift, pan.effective_shift());
    context.io_mut().add_mouse_button_event(button, true);
    frames(&mut context, &mut canvas, &mut graph, 3);
    context.io_mut().add_mouse_pos_event([815., 600.]);
    frames(&mut context, &mut canvas, &mut graph, 3);
    context
        .io_mut()
        .add_mouse_button_event(pan.left_button(false, true), false);
    frames(&mut context, &mut canvas, &mut graph, 3);
    pan.shift(false);
    context.io_mut().add_key_event(imgui::Key::ModShift, false);
    frame(&mut context, &mut canvas, &mut graph);
    assert!(
        graph.graph.selected.len() > 1,
        "box selection preserves every selected ID"
    );
    assert_eq!(canvas.view, before);
    let unit = scale(&canvas) / 100.;
    let start = [
        canvas.view[0][0] + 80. * unit,
        canvas.view[0][1] + 20. * unit,
    ];
    assert!(!canvas.accepts_background_pan(start));
    context.io_mut().add_mouse_pos_event(start);
    context.io_mut().add_mouse_button_event(
        pan.left_button(true, canvas.accepts_background_pan(start)),
        true,
    );
    frames(&mut context, &mut canvas, &mut graph, 3);
    context
        .io_mut()
        .add_mouse_pos_event([start[0] + 30., start[1] + 20.]);
    frames(&mut context, &mut canvas, &mut graph, 3);
    assert!(graph.previews > 0, "node drag emits position previews");
    assert_eq!(canvas.view, before);
    context.io_mut().add_key_event(imgui::Key::Escape, true);
    frame(&mut context, &mut canvas, &mut graph);
    context.io_mut().add_key_event(imgui::Key::Escape, false);
    context
        .io_mut()
        .add_mouse_button_event(pan.left_button(false, false), false);
    frames(&mut context, &mut canvas, &mut graph, 3);
    assert_eq!(graph.cancels, 1);
    assert_eq!(
        graph.commits, 0,
        "navigation, selection and cancelled movement never author data"
    );
    assert!(graph.graph.nodes.iter().all(|n| n.position.is_none()));
    // A subsequent completed drag commits once, never once per frame.
    context.io_mut().add_mouse_pos_event(start);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, true);
    frames(&mut context, &mut canvas, &mut graph, 3);
    // Cross the native drag threshold, then move and release on consecutive
    // frames; the final native position must be sampled on the release frame.
    context
        .io_mut()
        .add_mouse_pos_event([start[0] + 20., start[1] + 10.]);
    frame(&mut context, &mut canvas, &mut graph);
    context
        .io_mut()
        .add_mouse_pos_event([start[0] + 40., start[1] + 20.]);
    frame(&mut context, &mut canvas, &mut graph);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, false);
    frames(&mut context, &mut canvas, &mut graph, 3);
    assert_eq!(
        graph.commits,
        1,
        "previews: {}, positions: {:?}",
        graph.previews,
        graph
            .graph
            .nodes
            .iter()
            .map(|n| n.position)
            .collect::<Vec<_>>()
    );
    drop(canvas);
    drop(context);
}

#[test]
fn layout_uses_dependencies_not_node_storage_order() {
    let mut context = Context::new();
    let expected = layout(&context.graph, [260., 160.]);
    context.graph.nodes.reverse();
    let actual = layout(&context.graph, [260., 160.]);
    for position in expected {
        assert!(actual.contains(&position));
    }
    assert!(context.graph.nodes.iter().all(|n| n.position.is_none()));
}
