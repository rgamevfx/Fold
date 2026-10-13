//! Graph gestures expressed as validated, feature-independent topology edits.
use super::*;

#[derive(Clone)]
pub(super) struct Insertion {
    pub label: String,
    pub input: Socket,
    pub output: Socket,
    pub change: GraphChange,
    pub position: Option<[f32; 2]>,
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
                    position: None,
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

/// Deliberate screen-space zigzag, independent of canvas zoom. Small jitter and
/// slow direction changes do not count; one extraction per held-button gesture.
#[derive(Default, Debug)]
pub(super) struct Shake {
    anchor: Option<[f32; 2]>,
    direction: [f32; 2],
    last: f64,
    reversals: u8,
    fired: bool,
}
impl Shake {
    pub fn sample(&mut self, point: [f32; 2], now: f64) -> bool {
        if self.fired {
            return false;
        }
        let Some(anchor) = self.anchor else {
            self.anchor = Some(point);
            self.last = now;
            return false;
        };
        let delta = [point[0] - anchor[0], point[1] - anchor[1]];
        let distance = delta[0].hypot(delta[1]);
        if distance < 22. {
            return false;
        }
        let direction = [delta[0] / distance, delta[1] / distance];
        if now - self.last > 0.25 {
            self.reversals = 0;
        } else if direction[0] * self.direction[0] + direction[1] * self.direction[1] < -0.65 {
            self.reversals += 1;
        }
        self.anchor = Some(point);
        self.direction = direction;
        self.last = now;
        self.fired = self.reversals >= 3;
        self.fired
    }
}
