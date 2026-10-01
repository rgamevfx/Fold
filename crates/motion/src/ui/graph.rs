//! Motion graph semantics adapted to the shared editor, including group scope.
use super::state::State;
use crate::{
    Motion, authoring as a,
    graph::{Input, Link, registry},
    nodes::interface::GroupSettings,
};
use fold_foundation::ObjectId;
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_ui::sdk::{GRAPH_COLORS, graph_canvas::*};

pub(super) struct Context<'a> {
    pub state: &'a mut State,
    pub host: &'a mut dyn DesktopClient,
}
fn apply(motion: &mut Motion, change: &GraphChange) -> Result<Option<ObjectId>, String> {
    match change {
        GraphChange::Connect(w) => a::connect(
            motion,
            w.target.node,
            &w.target.key,
            Link {
                node: w.source.node,
                socket: w.source.key.clone(),
            },
        )?,
        GraphChange::Disconnect(socket) => {
            a::node(motion, socket.node)?.inputs.remove(&socket.key);
        }
        GraphChange::Delete(ids) => {
            for &id in ids {
                a::remove(motion, id)?;
            }
        }
        GraphChange::Positions(positions) => {
            for &(id, position) in positions {
                a::node(motion, id)?.position = position;
            }
        }
        GraphChange::Add { template, position } => {
            let id = a::add_node(motion, template)?;
            a::node(motion, id)?.position = *position;
            return Ok(Some(id));
        }
    }
    Ok(None)
}
impl GraphContext for Context<'_> {
    fn graph(&self) -> GraphView {
        let motion = self.state.view_motion().unwrap();
        let mut error = self.state.error.clone();
        let nodes = motion
            .graph
            .nodes
            .iter()
            .map(|node| {
                let (inputs, outputs) = motion.signature(node).unwrap_or_else(|e| {
                    error = e;
                    (vec![], vec![])
                });
                NodeView {
                    id: node.id,
                    position: Some(node.position),
                    summary: String::new(),
                    color: GRAPH_COLORS.image,
                    label: registry::find(&node.kind)
                        .map(|d| d.name)
                        .unwrap_or(&node.kind)
                        .into(),
                    inputs: inputs
                        .iter()
                        .map(|s| PortView {
                            key: s.id.clone(),
                            label: format!("{} : {:?}", s.id, s.kind),
                            color: GRAPH_COLORS.image,
                        })
                        .collect(),
                    outputs: outputs
                        .iter()
                        .map(|(key, kind)| PortView {
                            key: key.clone(),
                            label: format!("{key} : {kind:?}"),
                            color: GRAPH_COLORS.image,
                        })
                        .collect(),
                    can_open: node.kind == "fold.motion.group_instance"
                        && node
                            .settings::<GroupSettings>()
                            .ok()
                            .and_then(|s| s.group)
                            .is_some(),
                }
            })
            .collect();
        GraphView {
            key: GraphKey {
                document: self.state.document.unwrap(),
                group: self.state.group,
            },
            generation: self.state.generation,
            nodes,
            error,
            selected: self.state.selected.clone(),
            has_parent: self.state.group.is_some(),
            wires: motion
                .graph
                .nodes
                .iter()
                .flat_map(|node| {
                    node.inputs.iter().filter_map(|(key, input)| {
                        if let Input::Link(link) = input {
                            Some(WireView {
                                source: Socket {
                                    node: link.node,
                                    key: link.socket.clone(),
                                },
                                target: Socket {
                                    node: node.id,
                                    key: key.clone(),
                                },
                            })
                        } else {
                            None
                        }
                    })
                })
                .collect(),
        }
    }
    fn catalog(&self) -> Vec<NodeTemplate> {
        registry::definitions()
            .iter()
            .map(|d| NodeTemplate {
                key: d.id.into(),
                label: d.name.into(),
                category: d.category.into(),
            })
            .collect()
    }
    fn validate(&self, change: &GraphChange) -> Result<(), String> {
        let mut motion = self.state.view_motion().ok_or("No motion graph")?;
        apply(&mut motion, change)?;
        motion.validate()
    }
    fn event(&mut self, event: GraphEvent) -> Result<(), String> {
        match event {
            GraphEvent::Select(ids) => {
                self.state.selected = ids.clone();
                self.host.command(DesktopCommand::Select(Selection {
                    document: self.state.document,
                    objects: ids,
                }));
            }
            GraphEvent::PreviewPositions(positions) => {
                let mut motion = self.state.view_motion().ok_or("No motion graph")?;
                apply(&mut motion, &GraphChange::Positions(positions))?;
                self.state.replace_view(motion);
                self.state.editing = true;
            }
            GraphEvent::Commit(changes) => {
                let mut motion = self.state.view_motion().ok_or("No motion graph")?;
                let mut selected = None;
                for change in &changes {
                    selected = apply(&mut motion, change)?.or(selected);
                }
                motion.validate()?;
                self.state.replace_view(motion);
                self.state.commit(self.host);
                if !self.state.error.is_empty() {
                    return Err(self.state.error.clone());
                }
                if let Some(id) = selected {
                    self.event(GraphEvent::Select(vec![id]))?;
                }
            }
            GraphEvent::Cancel => self.state.cancel(self.host),
            GraphEvent::Navigate(node) => {
                let group = if let Some(node) = node {
                    let motion = self.state.view_motion().ok_or("No motion graph")?;
                    let group = motion
                        .graph
                        .node(node)?
                        .settings::<GroupSettings>()?
                        .group
                        .ok_or("Group is not assigned")?;
                    if !motion.groups.contains_key(&group) {
                        return Err("Group is missing".into());
                    }
                    self.state.parents.push(self.state.group);
                    Some(group)
                } else {
                    self.state.parents.pop().flatten()
                };
                self.state.cancel(self.host);
                self.state.group = group;
                self.event(GraphEvent::Select(vec![]))?;
            }
        }
        Ok(())
    }
}
