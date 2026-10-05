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
        let visible = visible_nodes(
            &motion,
            self.state.group,
            self.state.network_object,
            self.state.document_network,
        );
        let nodes = motion
            .graph
            .nodes
            .iter()
            .filter(|node| visible.contains(&node.id))
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
                    label: node_label(&motion, node),
                    inputs: inputs
                        .iter()
                        .filter(|s| {
                            node.kind != "fold.motion.group_instance"
                                || self.state.parameter_sockets
                                || matches!(
                                    s.kind,
                                    crate::fields::Kind::Content | crate::fields::Kind::Points
                                )
                                || matches!(node.inputs.get(&s.id), Some(Input::Link(_)))
                        })
                        .map(|s| PortView {
                            key: s.id.clone(),
                            label: super::input_label(&motion, node, &s.id),
                            color: GRAPH_COLORS.image,
                        })
                        .collect(),
                    outputs: outputs
                        .iter()
                        .map(|(key, _kind)| PortView {
                            key: key.clone(),
                            label: super::inspector::property_label(key),
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
                group: self.state.group.or(self
                    .state
                    .network_object
                    .filter(|_| !self.state.document_network)),
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
                .filter(|node| visible.contains(&node.id))
                .flat_map(|node| {
                    node.inputs.iter().filter_map(|(key, input)| {
                        if let Input::Link(link) = input
                            && visible.contains(&link.node)
                        {
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
                    let added = apply(&mut motion, change)?;
                    if self.state.group.is_none()
                        && !self.state.document_network
                        && let (Some(id), Some(owner)) = (added, self.state.network_object)
                    {
                        super::network_scope::include(&mut motion, owner, [id]);
                    }
                    selected = added.or(selected);
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
                let selection = if group.is_none() {
                    self.state.network_object.into_iter().collect()
                } else {
                    vec![]
                };
                self.event(GraphEvent::Select(selection))?;
            }
        }
        Ok(())
    }
}

/// Scope is presentation only; the persistent graph and stable IDs remain intact.
pub(super) fn visible_nodes(
    m: &Motion,
    group: Option<ObjectId>,
    object: Option<ObjectId>,
    full: bool,
) -> std::collections::BTreeSet<ObjectId> {
    if group.is_some() || full || m.scene.is_none() {
        return m.graph.nodes.iter().map(|n| n.id).collect();
    }
    object
        .map(|id| super::network_scope::members(m, id))
        .unwrap_or_default()
}
fn node_label(m: &Motion, n: &crate::graph::Node) -> String {
    if n.kind == "fold.motion.group_instance" {
        if let Some(g) = n
            .settings::<GroupSettings>()
            .ok()
            .and_then(|s| s.group)
            .and_then(|id| m.groups.get(&id))
        {
            return g.name.clone();
        }
    }
    if n.kind == "fold.motion.group_input" {
        return n
            .settings
            .get("key")
            .and_then(|v| v.as_str())
            .map(super::inspector::property_label)
            .unwrap_or("Input".into());
    }
    if n.kind == "fold.motion.object_reference" {
        if let Some(name) = serde_json::from_value::<ObjectId>(n.settings["object"].clone())
            .ok()
            .and_then(|id| m.scene.as_ref()?.objects.iter().find(|o| o.id == id))
            .map(|o| &o.name)
        {
            return name.clone();
        }
    }
    registry::find(&n.kind)
        .map(|d| d.name)
        .unwrap_or(&n.kind)
        .into()
}

#[cfg(test)]
mod scope_tests {
    use super::*;
    #[test]
    fn temporary_rewire_keeps_disconnected_nodes_visible_after_reload() {
        let mut host = crate::ui::tests::Host::new();
        let mut state = State::default();
        state.create(&mut host);
        let mut branch = None;
        state.change_scene(&mut host, |m| {
            let id = a::scene::create_object(m, "fold.motion.rectangle")?;
            let object = m.scene.as_ref().unwrap().objects[0].clone();
            let extra = a::add_node(m, "fold.motion.fill")?;
            a::node(m, extra)?.connect("content", object.appearance.unwrap(), "content");
            a::node(m, object.transform.unwrap())?.connect("content", extra, "content");
            branch = Some(extra);
            Ok(Some(id))
        });
        let branch = branch.unwrap();
        let object = state
            .motion
            .as_ref()
            .unwrap()
            .scene
            .as_ref()
            .unwrap()
            .objects[0]
            .clone();
        state.network_object = Some(object.id);
        let mut context = Context {
            state: &mut state,
            host: &mut host,
        };
        let before: std::collections::BTreeSet<_> =
            context.graph().nodes.iter().map(|n| n.id).collect();
        context
            .event(GraphEvent::Commit(vec![GraphChange::Connect(WireView {
                source: Socket {
                    node: object.source.node,
                    key: "content".into(),
                },
                target: Socket {
                    node: object.transform.unwrap(),
                    key: "content".into(),
                },
            })]))
            .unwrap();
        assert!(
            context
                .state
                .motion
                .as_ref()
                .unwrap()
                .graph
                .node(object.appearance.unwrap())
                .is_ok(),
            "rewiring must not delete nodes"
        );
        let after: std::collections::BTreeSet<_> =
            context.graph().nodes.iter().map(|n| n.id).collect();
        assert_eq!(
            after, before,
            "rewiring must not hide the disconnected branch"
        );
        let mut reopened = State::default();
        reopened.sync(&mut host);
        reopened.network_object = Some(object.id);
        let restored = Context {
            state: &mut reopened,
            host: &mut host,
        }
        .graph();
        assert_eq!(
            restored
                .nodes
                .iter()
                .map(|n| n.id)
                .collect::<std::collections::BTreeSet<_>>(),
            before
        );
        Context {
            state: &mut reopened,
            host: &mut host,
        }
        .event(GraphEvent::Commit(vec![GraphChange::Delete(vec![branch])]))
        .unwrap();
        assert!(
            !Context {
                state: &mut reopened,
                host: &mut host
            }
            .graph()
            .nodes
            .iter()
            .any(|n| n.id == branch)
        );
        host.project.undo().unwrap();
        reopened.sync(&mut host);
        assert!(
            Context {
                state: &mut reopened,
                host: &mut host
            }
            .graph()
            .nodes
            .iter()
            .any(|n| n.id == branch)
        );
    }
    #[test]
    fn disconnected_new_node_remains_in_its_construction_after_reload() {
        let mut host = crate::ui::tests::Host::new();
        let mut state = State::default();
        state.create(&mut host);
        state.change_scene(&mut host, |m| {
            a::scene::create_object(m, "fold.motion.rectangle").map(Some)
        });
        let owner = state.selected[0];
        state.network_object = Some(owner);
        Context {
            state: &mut state,
            host: &mut host,
        }
        .event(GraphEvent::Commit(vec![GraphChange::Add {
            template: "fold.motion.wave".into(),
            position: [100., 100.],
        }]))
        .unwrap();
        let added = state.selected[0];
        let mut reopened = State::default();
        reopened.sync(&mut host);
        reopened.network_object = Some(owner);
        assert!(
            Context {
                state: &mut reopened,
                host: &mut host
            }
            .graph()
            .nodes
            .iter()
            .any(|n| n.id == added)
        );
        assert!(
            visible_nodes(
                reopened.motion.as_ref().unwrap(),
                None,
                Some(ObjectId::new()),
                false
            )
            .is_empty()
        );
    }
    #[test]
    fn selected_generator_shows_references_and_asset_but_not_source_internals() {
        let mut m = Motion::new_scene();
        let badge = a::scene::assets::badge(&mut m).unwrap();
        let path = a::scene::create_object(&mut m, "fold.motion.path").unwrap();
        let generator = a::scene::assets::along_path(&mut m, badge, path).unwrap();
        let nodes = visible_nodes(&m, None, Some(generator), false);
        assert_eq!(nodes.len(), 4);
        assert!(nodes.contains(&generator));
        assert!(!nodes.contains(&badge));
        assert!(!nodes.contains(&path));
        assert_eq!(
            visible_nodes(&m, None, Some(generator), true).len(),
            m.graph.nodes.len()
        );
    }
}
