//! Graph gestures expressed as validated, feature-independent topology edits.
use super::*;

#[derive(Clone)]
pub(super) struct Insertion {
    pub label: String,
    pub input: Socket,
    pub output: Socket,
    pub change: GraphChange,
}

pub(super) fn insertions(
    context: &impl GraphContext,
    graph: &GraphView,
    node: &NodeView,
    wire: &WireView,
) -> Vec<Insertion> {
    // Only loose nodes can be dropped into a pipe. Moving an existing branch
    // must never silently replace its wiring (including off-screen links).
    if !context.can_insert(node.id)
        || wire.source.node == node.id
        || wire.target.node == node.id
        || graph
            .wires
            .iter()
            .any(|w| w.source.node == node.id || w.target.node == node.id)
    {
        return vec![];
    }
    let primary = context.primary_input(node);
    let mut choices = Vec::new();
    for input in &node.inputs {
        if primary.is_some_and(|key| key != input.key) {
            continue;
        }
        for output in &node.outputs {
            if context
                .primary_output(node)
                .is_some_and(|key| key != output.key)
            {
                continue;
            }
            let input_socket = Socket {
                node: node.id,
                key: input.key.clone(),
            };
            let output_socket = Socket {
                node: node.id,
                key: output.key.clone(),
            };
            let change = GraphChange::Rewire {
                remove: vec![wire.target.clone()],
                add: vec![
                    WireView {
                        source: wire.source.clone(),
                        target: input_socket.clone(),
                    },
                    WireView {
                        source: output_socket.clone(),
                        target: wire.target.clone(),
                    },
                ],
            };
            if context.validate(&change).is_ok() {
                choices.push(Insertion {
                    label: format!("{} → {}", input.label, output.label),
                    input: input_socket,
                    output: output_socket,
                    change,
                });
            }
        }
    }
    choices
}

pub(super) fn connection(
    graph: &GraphView,
    start: &(Socket, bool),
    end: &(Socket, bool),
) -> Result<GraphChange, String> {
    match (start.1, end.1) {
        (true, false) => Ok(GraphChange::Connect(WireView {
            source: start.0.clone(),
            target: end.0.clone(),
        })),
        (false, true) => Ok(GraphChange::Connect(WireView {
            source: end.0.clone(),
            target: start.0.clone(),
        })),
        (false, false) if start.0 != end.0 => {
            let existing = graph
                .wires
                .iter()
                .find(|w| w.target == start.0)
                .ok_or("Drag a connected input to move its wire")?;
            if graph.wires.iter().any(|w| w.target == end.0) {
                return Err("Disconnect the destination input first".into());
            }
            Ok(GraphChange::Rewire {
                remove: vec![start.0.clone()],
                add: vec![WireView {
                    source: existing.source.clone(),
                    target: end.0.clone(),
                }],
            })
        }
        _ => Err("Connect an output to an input".into()),
    }
}
