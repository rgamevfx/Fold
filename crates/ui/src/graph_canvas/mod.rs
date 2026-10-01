//! One node editor for all feature graphs. Adapters describe stable objects and
//! translate events to domain transactions; native IDs, navigation, selection,
//! search, layout, hit testing and gesture lifetime belong to this module.
mod editor;
mod navigation;
#[cfg(test)]
mod tests;
pub use editor::GraphCanvas;
use fold_foundation::{DocumentId, ObjectId};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct GraphKey {
    pub document: DocumentId,
    pub group: Option<ObjectId>,
}
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Socket {
    pub node: ObjectId,
    pub key: String,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WireView {
    pub source: Socket,
    pub target: Socket,
}
#[derive(Clone)]
pub struct PortView {
    pub key: String,
    pub label: String,
    pub color: [f32; 4],
}
#[derive(Clone)]
pub struct NodeView {
    pub id: ObjectId,
    pub label: String,
    pub summary: String,
    pub color: [f32; 4],
    pub position: Option<[f32; 2]>,
    pub inputs: Vec<PortView>,
    pub outputs: Vec<PortView>,
    pub can_open: bool,
}
#[derive(Clone)]
pub struct GraphView {
    pub key: GraphKey,
    /// Changes when authoritative data replaces a gesture (undo, cancel, reload).
    pub generation: u64,
    pub nodes: Vec<NodeView>,
    pub wires: Vec<WireView>,
    pub selected: Vec<ObjectId>,
    pub has_parent: bool,
    pub error: String,
}
pub struct NodeTemplate {
    pub key: String,
    pub label: String,
    pub category: String,
}
#[derive(Clone, Debug)]
pub enum GraphChange {
    Add {
        template: String,
        position: [f32; 2],
    },
    Connect(WireView),
    Disconnect(Socket),
    Delete(Vec<ObjectId>),
    Positions(Vec<(ObjectId, [f32; 2])>),
}
#[derive(Debug)]
pub enum GraphEvent {
    Select(Vec<ObjectId>),
    /// UI-only positions. No undo entry until Commit; Cancel restores the model.
    PreviewPositions(Vec<(ObjectId, [f32; 2])>),
    /// Apply the whole batch atomically, including any outstanding position edit.
    Commit(Vec<GraphChange>),
    Cancel,
    /// Open a node's graph, or return to its parent. Never a project mutation.
    Navigate(Option<ObjectId>),
}
/// The feature seam: no ImGui or native editor IDs, no evaluation or persistence
/// in the editor. Validation must be side-effect free; failed batches must not
/// leave partially applied changes. Adapters retain ownership of project undo.
pub trait GraphContext {
    fn graph(&self) -> GraphView;
    fn catalog(&self) -> Vec<NodeTemplate>;
    fn validate(&self, change: &GraphChange) -> Result<(), String>;
    fn event(&mut self, event: GraphEvent) -> Result<(), String>;
}

/// Shared DAG layout, used both for unauthored positions and explicit Arrange.
/// Does not mutate the feature model until the user commits an edit.
fn layout(graph: &GraphView, spacing: [f32; 2]) -> Vec<(ObjectId, [f32; 2])> {
    use std::collections::BTreeMap;
    let mut depths = BTreeMap::new();
    for _ in 0..graph.nodes.len() {
        let mut progress = false;
        for node in &graph.nodes {
            if depths.contains_key(&node.id) {
                continue;
            }
            let sources: Vec<_> = graph
                .wires
                .iter()
                .filter(|w| w.target.node == node.id)
                .map(|w| w.source.node)
                .collect();
            if sources.iter().all(|id| depths.contains_key(id)) {
                let depth = sources.iter().map(|id| depths[id] + 1).max().unwrap_or(0);
                depths.insert(node.id, depth);
                progress = true;
            }
        }
        if !progress {
            break;
        }
    }
    let mut lanes = BTreeMap::new();
    graph
        .nodes
        .iter()
        .map(|node| {
            let depth = depths.get(&node.id).copied().unwrap_or(0);
            let lane = lanes.entry(depth).or_insert(0);
            let position = [depth as f32 * spacing[0], *lane as f32 * spacing[1]];
            *lane += 1;
            (node.id, position)
        })
        .collect()
}
