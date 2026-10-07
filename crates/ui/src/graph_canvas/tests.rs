use super::*;
use crate::sdk::{CanvasPan, GRAPH_COLORS, imgui};

struct Context {
    graph: GraphView,
    original: GraphView,
    commits: usize,
    previews: usize,
    cancels: usize,
    selections: usize,
    vertical: bool,
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
            selections: 0,
            vertical: false,
        }
    }
}
impl GraphContext for Context {
    fn presentation(&self) -> GraphPresentation {
        if self.vertical {
            GraphPresentation::VerticalCompact
        } else {
            GraphPresentation::Horizontal
        }
    }
    fn primary_input<'a>(&self, node: &'a NodeView) -> Option<&'a str> {
        if self.vertical {
            ["background", "image", "A"]
                .into_iter()
                .find(|key| node.inputs.iter().any(|p| p.key == *key))
        } else {
            (node.inputs.len() == 1).then(|| node.inputs[0].key.as_str())
        }
    }

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
            GraphEvent::Select(ids) => {
                self.selections += 1;
                self.graph.selected = ids;
            }
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
            GraphEvent::Commit(changes) => {
                for change in changes {
                    apply_test_change(&mut self.graph, change);
                }
                self.commits += 1;
                self.original = self.graph.clone();
                self.graph.generation += 1;
            }
            GraphEvent::Cancel => {
                self.cancels += 1;
                self.graph.nodes = self.original.nodes.clone();
                self.graph.wires = self.original.wires.clone();
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
fn arrange_button_and_shortcut_commit_layout_once_and_allow_text_entry() {
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
    canvas.initialize(&context, None);
    let mut graph = Context::new();
    for node in &mut graph.graph.nodes {
        node.position = Some([0., 0.]);
    }
    let mut input = String::new();
    let mut draw = |context: &mut imgui::Context, graph: &mut Context, typing: bool| {
        let mut button = [0.; 2];
        let ui = context.frame();
        ui.window("graph")
            .position([0.; 2], imgui::Condition::Always)
            .size([850., 800.], imgui::Condition::Always)
            .build(|| {
                if typing {
                    ui.set_keyboard_focus_here();
                    ui.input_text("Name", &mut input).build();
                }
                let p = ui.cursor_screen_pos();
                let height = ui.frame_height();
                let width = height + 4.;
                button = [
                    p[0] + 2. * (width + ui.clone_style().item_spacing()[0]) + width / 2.,
                    p[1] + height / 2.,
                ];
                canvas.draw(ui, graph);
            });
        assert!(context.render_legacy().total_vtx_count() > 100);
        button
    };
    for _ in 0..3 {
        draw(&mut context, &mut graph, false);
    }
    let button = draw(&mut context, &mut graph, false);
    context.io_mut().add_mouse_pos_event(button);
    draw(&mut context, &mut graph, false);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, true);
    draw(&mut context, &mut graph, false);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, false);
    draw(&mut context, &mut graph, false);
    assert_eq!(
        graph.commits, 1,
        "Arrange is directly accessible on the toolbar"
    );
    assert!(
        graph
            .graph
            .nodes
            .windows(2)
            .all(|pair| pair[0].position != pair[1].position)
    );

    context.io_mut().add_mouse_pos_event([420., 650.]);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, true);
    draw(&mut context, &mut graph, false);
    context
        .io_mut()
        .add_mouse_button_event(imgui::MouseButton::Left, false);
    draw(&mut context, &mut graph, false);
    context.io_mut().add_key_event(imgui::Key::L, true);
    for _ in 0..40 {
        draw(&mut context, &mut graph, false);
    }
    assert_eq!(
        graph.commits, 2,
        "holding L must not repeat the transaction"
    );
    context.io_mut().add_key_event(imgui::Key::L, false);
    draw(&mut context, &mut graph, false);

    context.io_mut().add_key_event(imgui::Key::ModCtrl, true);
    context.io_mut().add_key_event(imgui::Key::L, true);
    draw(&mut context, &mut graph, false);
    assert_eq!(graph.commits, 2, "Ctrl+L must not arrange nodes");
    context.io_mut().add_key_event(imgui::Key::L, false);
    context.io_mut().add_key_event(imgui::Key::ModCtrl, false);
    for _ in 0..3 {
        draw(&mut context, &mut graph, true);
    }
    assert!(context.io().want_text_input());
    context.io_mut().add_key_event(imgui::Key::L, true);
    context.io_mut().add_input_character('l');
    draw(&mut context, &mut graph, true);
    assert_eq!(graph.commits, 2, "typing L must not arrange nodes");
    assert_eq!(input, "l");
}

#[test]
fn native_navigation_selection_and_gesture_lifetime_are_shared() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let typography = crate::sdk::typography::Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = GraphCanvas::default();
    canvas.initialize(&context, Some(&typography));
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

#[test]
fn clicking_an_already_selected_node_reasserts_inspection_without_editing() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let typography = crate::sdk::typography::Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = GraphCanvas::default();
    canvas.initialize(&context, Some(&typography));
    let mut graph = Context::new();
    frames(&mut context, &mut canvas, &mut graph, 20);
    let unit = scale(&canvas) / 100.;
    let point = [
        canvas.view[0][0] + 80. * unit,
        canvas.view[0][1] + 20. * unit,
    ];
    context.io_mut().add_mouse_pos_event(point);
    frames(&mut context, &mut canvas, &mut graph, 3);
    for _ in 0..2 {
        let before = graph.selections;
        context
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, true);
        frames(&mut context, &mut canvas, &mut graph, 3);
        context
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, false);
        frames(&mut context, &mut canvas, &mut graph, 3);
        assert!(
            graph.selections > before,
            "explicit re-selection must notify the inspector router"
        );
        assert_eq!(graph.graph.selected, vec![graph.graph.nodes[0].id]);
    }
    assert_eq!(graph.commits, 0);
}

#[path = "render_tests.rs"]
mod rendering;

fn apply_test_change(graph: &mut GraphView, change: GraphChange) {
    match change {
        GraphChange::Positions(positions) => {
            for (id, p) in positions {
                graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == id)
                    .unwrap()
                    .position = Some(p);
            }
        }
        GraphChange::Disconnect(socket) => graph.wires.retain(|w| w.target != socket),
        GraphChange::Connect(wire) => {
            graph.wires.retain(|w| w.target != wire.target);
            graph.wires.push(wire);
        }
        GraphChange::Rewire { remove, add } => {
            for socket in remove {
                apply_test_change(graph, GraphChange::Disconnect(socket));
            }
            for wire in add {
                apply_test_change(graph, GraphChange::Connect(wire));
            }
        }
        GraphChange::Delete(ids) => {
            graph.nodes.retain(|n| !ids.contains(&n.id));
            graph
                .wires
                .retain(|w| !ids.contains(&w.source.node) && !ids.contains(&w.target.node));
        }
        GraphChange::Add { position, .. } => {
            let mut node = graph.nodes[1].clone();
            node.id = ObjectId::new();
            node.position = Some(position);
            graph.nodes.push(node);
        }
    }
}

fn gesture_fixture(vertical: bool) -> (imgui::Context, GraphCanvas, Context) {
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
    canvas.initialize(&context, None);
    let mut graph = Context::new();
    graph.vertical = vertical;
    graph.graph.nodes.truncate(3);
    graph.graph.wires.truncate(1);
    let positions = if vertical {
        [[0., 0.], [0., 350.], [250., 175.]]
    } else {
        [[0., 0.], [650., 0.], [325., 350.]]
    };
    for (node, p) in graph.graph.nodes.iter_mut().zip(positions) {
        node.position = Some(p);
    }
    graph.graph.selected = vec![graph.graph.nodes[2].id];
    graph.original = graph.graph.clone();
    frames(&mut context, &mut canvas, &mut graph, 30);
    (context, canvas, graph)
}

fn click(
    context: &mut imgui::Context,
    canvas: &mut GraphCanvas,
    graph: &mut Context,
    point: [f32; 2],
    button: imgui::MouseButton,
) {
    context.io_mut().add_mouse_pos_event(point);
    frames(context, canvas, graph, 2);
    context.io_mut().add_mouse_button_event(button, true);
    frames(context, canvas, graph, 2);
    context.io_mut().add_mouse_button_event(button, false);
    frames(context, canvas, graph, 3);
}

#[test]
fn wire_delete_backspace_and_context_disconnect_remove_only_the_wire() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for vertical in [false, true] {
        for key in [Some(imgui::Key::Delete), Some(imgui::Key::Backspace), None] {
            let (mut imgui, mut canvas, mut graph) = gesture_fixture(vertical);
            let wire = canvas.wire_probe.0;
            click(
                &mut imgui,
                &mut canvas,
                &mut graph,
                wire,
                if key.is_some() {
                    imgui::MouseButton::Left
                } else {
                    imgui::MouseButton::Right
                },
            );
            if let Some(key) = key {
                assert_eq!(canvas.wire_probe.1, 1, "wire is selected");
                imgui.io_mut().add_key_event(key, true);
                frames(&mut imgui, &mut canvas, &mut graph, 4);
                imgui.io_mut().add_key_event(key, false);
            } else {
                let point = canvas.disconnect_probe;
                assert_ne!(point, [0.; 2], "Disconnect menu is visible");
                click(
                    &mut imgui,
                    &mut canvas,
                    &mut graph,
                    point,
                    imgui::MouseButton::Left,
                );
            }
            assert!(graph.graph.wires.is_empty(), "{key:?}, vertical={vertical}");
            assert_eq!(graph.graph.nodes.len(), 3);
            assert_eq!(graph.commits, 1);
            drop(canvas);
        }
    }
}

#[test]
fn toolbar_add_uses_last_graph_click_and_tab_uses_pointer() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let (mut imgui, mut canvas, mut graph) = gesture_fixture(true);
    let point = [700., 650.];
    imgui.io_mut().add_mouse_pos_event(point);
    imgui.io_mut().add_mouse_wheel_event([0., 0.6]);
    frames(&mut imgui, &mut canvas, &mut graph, 25);
    // The desktop routes a physical left-background click through CanvasPan.
    let mut pan = CanvasPan::default();
    let button = pan.left_button(true, canvas.accepts_background_pan(point));
    assert_eq!(button, imgui::MouseButton::Middle);
    click(&mut imgui, &mut canvas, &mut graph, point, button);
    pan.left_button(false, true);
    let zoom = scale(&canvas) / 100.;
    let expected = [0, 1].map(|i| (point[i] - canvas.view[0][i]) / zoom);
    let add = canvas.add_probe;
    click(
        &mut imgui,
        &mut canvas,
        &mut graph,
        add,
        imgui::MouseButton::Left,
    );
    for i in 0..2 {
        assert!(
            (canvas.popup_position[i] - expected[i]).abs() < 1.,
            "actual={:?}, expected={expected:?}",
            canvas.popup_position
        );
    }
    // Moving through the popup must not change the stored canvas location.
    imgui.io_mut().add_mouse_pos_event([100., 150.]);
    frames(&mut imgui, &mut canvas, &mut graph, 3);
    for i in 0..2 {
        assert!(
            (canvas.popup_position[i] - expected[i]).abs() < 1.,
            "actual={:?}, expected={expected:?}",
            canvas.popup_position
        );
    }
    let template = canvas.template_probe;
    click(
        &mut imgui,
        &mut canvas,
        &mut graph,
        template,
        imgui::MouseButton::Left,
    );
    assert_eq!(graph.graph.nodes.len(), 4);
    for i in 0..2 {
        assert!((graph.graph.nodes[3].position.unwrap()[i] - expected[i]).abs() < 1.);
    }
    click(
        &mut imgui,
        &mut canvas,
        &mut graph,
        point,
        imgui::MouseButton::Left,
    );
    let pointer = [600., 500.];
    imgui.io_mut().add_mouse_pos_event(pointer);
    frame(&mut imgui, &mut canvas, &mut graph);
    imgui.io_mut().add_key_event(imgui::Key::Tab, true);
    frame(&mut imgui, &mut canvas, &mut graph);
    for i in 0..2 {
        assert!((canvas.popup_position[i] - (pointer[i] - canvas.view[0][i]) / zoom).abs() < 1.);
    }
}

#[test]
fn drag_loose_node_into_wire_previews_commits_once_and_escape_restores() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for vertical in [false, true] {
        for cancel in [false, true] {
            let (mut imgui, mut canvas, mut graph) = gesture_fixture(vertical);
            let node = graph.graph.nodes[2].id;
            let start = canvas.node_probes.iter().find(|p| p.0 == node).unwrap().1;
            let destination = canvas.wire_probe.0;
            imgui.io_mut().add_mouse_pos_event(start);
            frames(&mut imgui, &mut canvas, &mut graph, 2);
            imgui
                .io_mut()
                .add_mouse_button_event(imgui::MouseButton::Left, true);
            frames(&mut imgui, &mut canvas, &mut graph, 3);
            imgui
                .io_mut()
                .add_mouse_pos_event([start[0] + 20., start[1] + 20.]);
            frames(&mut imgui, &mut canvas, &mut graph, 3);
            imgui.io_mut().add_mouse_pos_event(destination);
            frames(&mut imgui, &mut canvas, &mut graph, 4);
            assert!(
                canvas.insertion_probe,
                "insertion preview, vertical={vertical}, start={start:?}, end={destination:?}, nodes={:?}, selected={:?}, previews={}, wires={:?}",
                canvas.node_probes, graph.graph.selected, graph.previews, graph.graph.wires
            );
            assert_eq!(graph.commits, 0);
            let before = graph.original.clone();
            if cancel {
                imgui.io_mut().add_key_event(imgui::Key::Escape, true);
                frame(&mut imgui, &mut canvas, &mut graph);
                imgui.io_mut().add_key_event(imgui::Key::Escape, false);
            }
            imgui
                .io_mut()
                .add_mouse_button_event(imgui::MouseButton::Left, false);
            frames(&mut imgui, &mut canvas, &mut graph, 4);
            if cancel {
                assert_eq!(graph.commits, 0);
                assert_eq!(graph.graph.wires, before.wires);
                assert_eq!(graph.graph.nodes[2].position, before.nodes[2].position);
            } else {
                assert_eq!(graph.commits, 1);
                assert_eq!(graph.graph.wires.len(), 2);
                assert!(graph.graph.wires.iter().any(|w| w.target.node == node));
                assert!(graph.graph.wires.iter().any(|w| w.source.node == node));
                assert_ne!(graph.graph.nodes[2].position, before.nodes[2].position);
            }
            drop(canvas);
        }
    }
}

#[test]
fn connected_input_drag_detaches_rewires_or_cancels_atomically() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for vertical in [false, true] {
        for action in ["detach", "rewire", "cancel"] {
            let (mut imgui, mut canvas, mut graph) = gesture_fixture(vertical);
            let target = graph.graph.nodes[1].id;
            let spare = graph.graph.nodes[2].id;
            let pin = |id| {
                canvas
                    .pin_probes
                    .iter()
                    .find(|p| p.0.node == id && !p.1)
                    .unwrap()
                    .2
            };
            let start = pin(target);
            let end = if action == "rewire" {
                pin(spare)
            } else {
                [760., 670.]
            };
            imgui.io_mut().add_mouse_pos_event(start);
            frames(&mut imgui, &mut canvas, &mut graph, 2);
            imgui
                .io_mut()
                .add_mouse_button_event(imgui::MouseButton::Left, true);
            frames(&mut imgui, &mut canvas, &mut graph, 3);
            imgui.io_mut().add_mouse_pos_event(end);
            frames(&mut imgui, &mut canvas, &mut graph, 4);
            assert_eq!(graph.commits, 0, "preview retains the original wire");
            if action == "cancel" {
                imgui.io_mut().add_key_event(imgui::Key::Escape, true);
                frame(&mut imgui, &mut canvas, &mut graph);
                imgui.io_mut().add_key_event(imgui::Key::Escape, false);
            }
            imgui
                .io_mut()
                .add_mouse_button_event(imgui::MouseButton::Left, false);
            frames(&mut imgui, &mut canvas, &mut graph, 5);
            match action {
                "detach" => {
                    assert!(graph.graph.wires.is_empty(), "vertical={vertical}");
                    assert_eq!(graph.commits, 1);
                }
                "rewire" => {
                    assert_eq!(graph.graph.wires.len(), 1);
                    assert_eq!(graph.graph.wires[0].target.node, spare);
                    assert_eq!(graph.commits, 1);
                }
                _ => {
                    assert_eq!(graph.graph.wires[0].target.node, target);
                    assert_eq!(graph.commits, 0);
                }
            }
            drop(canvas);
        }
    }
}

fn composite_fixture() -> Context {
    let mut context = Context::new();
    context.vertical = true;
    context.graph.nodes.truncate(7);
    let port = |key: &str| PortView {
        key: key.into(),
        label: match key {
            "foreground" => "A · Foreground",
            "background" => "B · Background",
            _ => key,
        }
        .into(),
        color: if key == "mask" {
            GRAPH_COLORS.mask
        } else {
            GRAPH_COLORS.image
        },
    };
    for (i, node) in context.graph.nodes.iter_mut().enumerate() {
        node.label = [
            "Foreground",
            "Grade",
            "Background",
            "Blur",
            "Merge",
            "Output",
            "Mask",
        ][i]
            .into();
        node.summary.clear();
        node.position = None;
        node.inputs = match i {
            1 | 3 => vec![port("image"), port("mask")],
            4 => vec![port("foreground"), port("background"), port("mask")],
            5 => vec![port("image")],
            _ => vec![],
        };
        node.outputs = if i == 5 { vec![] } else { vec![port("image")] };
        node.color = match i {
            0 | 2 => GRAPH_COLORS.source,
            4 => GRAPH_COLORS.merge,
            6 => GRAPH_COLORS.mask,
            _ => GRAPH_COLORS.image,
        };
    }
    context.graph.wires = [
        (0, 1, "image"),
        (2, 3, "image"),
        (1, 4, "foreground"),
        (3, 4, "background"),
        (4, 5, "image"),
        (6, 4, "mask"),
    ]
    .into_iter()
    .map(|(a, b, key)| WireView {
        source: Socket {
            node: context.graph.nodes[a].id,
            key: "image".into(),
        },
        target: Socket {
            node: context.graph.nodes[b].id,
            key: key.into(),
        },
    })
    .collect();
    context.graph.selected = vec![context.graph.nodes[4].id];
    context.original = context.graph.clone();
    context
}

#[test]
fn compact_layout_keeps_the_b_pipe_straight_and_cards_small() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let (mut imgui, mut canvas, _) = gesture_fixture(true);
    let mut graph = composite_fixture();
    let positions = compact::layout(&graph.graph, [180., 78.], &graph);
    let p = |index: usize| {
        positions
            .iter()
            .find(|(id, _)| *id == graph.graph.nodes[index].id)
            .unwrap()
            .1
    };
    assert_eq!(p(2)[0], p(3)[0]);
    assert_eq!(p(3)[0], p(4)[0]);
    assert_eq!(p(4)[0], p(5)[0]);
    assert!(p(1)[0] < p(4)[0], "A comes from a side branch");
    for wire in &graph.graph.wires {
        let source = positions
            .iter()
            .find(|(id, _)| *id == wire.source.node)
            .unwrap()
            .1;
        let target = positions
            .iter()
            .find(|(id, _)| *id == wire.target.node)
            .unwrap()
            .1;
        assert!(source[1] < target[1]);
    }
    frames(&mut imgui, &mut canvas, &mut graph, 30);
    let merge = graph.graph.nodes[4].id;
    let size = canvas.node_probes.iter().find(|p| p.0 == merge).unwrap().2;
    assert!(size[0] <= 115. && size[1] <= 32., "compact card: {size:?}");
    assert!(
        scale(&canvas) <= 101.,
        "small networks must not be enlarged by Frame all"
    );
    assert_eq!(graph.commits, 0, "viewing doesn't author a new layout");
    let pin = |key, output| {
        canvas
            .pin_probes
            .iter()
            .find(|p| p.0.node == merge && p.0.key == key && p.1 == output)
            .unwrap()
            .2
    };
    let a = pin("foreground", false);
    let b = pin("background", false);
    let mask = pin("mask", false);
    let output = pin("image", true);
    assert!(b[1] < a[1] && a[1] < output[1]);
    assert!(a[0] < b[0] && b[0] < mask[0]);
    assert_eq!(b[0], output[0]);
}

#[test]
fn insertion_prefers_declared_b_and_offers_choices_for_ambiguous_motion_ports() {
    let mut context = Context::new();
    let wire = context.graph.wires[0].clone();
    let mut node = context.graph.nodes[2].clone();
    context.graph.wires.truncate(1);
    let port = node.inputs[0].clone();
    node.inputs = ["foreground", "background"]
        .into_iter()
        .map(|key| PortView {
            key: key.into(),
            label: key.into(),
            ..port.clone()
        })
        .collect();
    context.vertical = true;
    let choices = interactions::insertions(&context, &context.graph, &node, &wire);
    assert_eq!(choices.len(), 1);
    assert_eq!(choices[0].input.key, "background");
    context.vertical = false;
    assert_eq!(
        interactions::insertions(&context, &context.graph, &node, &wire).len(),
        2
    );
    context.graph.wires.push(WireView {
        source: choices[0].output.clone(),
        target: wire.target.clone(),
    });
    assert!(
        interactions::insertions(&context, &context.graph, &node, &wire).is_empty(),
        "already wired nodes are not insertable"
    );
}

#[test]
fn ambiguous_insertion_waits_for_port_choice_and_escape_cancels_the_move() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    for cancel in [false, true] {
        let (mut imgui, mut canvas, mut graph) = gesture_fixture(false);
        let node = &mut graph.graph.nodes[2];
        let mut port = node.inputs[0].clone();
        port.key = "alternate".into();
        port.label = "Alternate".into();
        node.inputs.push(port);
        let id = node.id;
        graph.graph.generation += 1;
        graph.original = graph.graph.clone();
        frames(&mut imgui, &mut canvas, &mut graph, 3);
        let start = canvas.node_probes.iter().find(|p| p.0 == id).unwrap().1;
        let end = canvas.wire_probe.0;
        imgui.io_mut().add_mouse_pos_event(start);
        frames(&mut imgui, &mut canvas, &mut graph, 2);
        imgui
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, true);
        frames(&mut imgui, &mut canvas, &mut graph, 3);
        imgui
            .io_mut()
            .add_mouse_pos_event([start[0] + 20., start[1] + 20.]);
        frames(&mut imgui, &mut canvas, &mut graph, 3);
        imgui.io_mut().add_mouse_pos_event(end);
        frames(&mut imgui, &mut canvas, &mut graph, 4);
        assert!(canvas.insertion_probe);
        imgui
            .io_mut()
            .add_mouse_button_event(imgui::MouseButton::Left, false);
        frames(&mut imgui, &mut canvas, &mut graph, 3);
        assert_eq!(
            graph.commits, 0,
            "an ambiguous insertion waits for a choice"
        );
        assert_eq!(canvas.choice_probes.len(), 2);
        if cancel {
            let original = graph.original.clone();
            imgui.io_mut().add_key_event(imgui::Key::Escape, true);
            frames(&mut imgui, &mut canvas, &mut graph, 3);
            imgui.io_mut().add_key_event(imgui::Key::Escape, false);
            assert_eq!(graph.graph.wires, original.wires);
            assert_eq!(graph.graph.nodes[2].position, original.nodes[2].position);
            assert_eq!(graph.commits, 0);
        } else {
            let choice = canvas.choice_probes[1];
            click(
                &mut imgui,
                &mut canvas,
                &mut graph,
                choice,
                imgui::MouseButton::Left,
            );
            assert_eq!(graph.commits, 1);
            assert_eq!(
                graph
                    .graph
                    .wires
                    .iter()
                    .find(|w| w.target.node == id)
                    .unwrap()
                    .target
                    .key,
                "alternate"
            );
        }
        drop(canvas);
    }
}
