//! Working-view routing is authored independently of the published Output.
use crate::{Composite, Node, OperationConfig};
use fold_foundation::ObjectId;
pub const DOCKED: &str = "fold.viewer.docked";
impl Composite {
    pub fn view_node(&mut self, source: ObjectId) -> Result<ObjectId, String> {
        if self.node(source)?.operation.operation == "Viewer" {
            return Ok(source);
        }
        let id = if let Some(node) = self
            .nodes
            .iter()
            .find(|n| n.operation.operation == "Viewer")
        {
            node.id
        } else {
            let mut viewer = Node::disconnected(OperationConfig::Viewer);
            viewer.position = Some([0., 0.]);
            viewer
                .extensions
                .insert(DOCKED.into(), serde_json::json!(true));
            let id = viewer.id;
            self.nodes.push(viewer);
            id
        };
        self.connect(source, id, "image")?;
        Ok(id)
    }
}
