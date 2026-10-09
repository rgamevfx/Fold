//! Motion semantics adapted to the shared graph canvas.
mod edit;
mod view;
use super::state::State;
use crate::{model::Reference, registry};
pub(super) use edit::apply;
use fold_foundation::ObjectId;
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::{GRAPH_COLORS, graph_canvas::*};
use serde::{Deserialize, Serialize};
pub(super) use view::view;

#[derive(Serialize, Deserialize)]
#[serde(tag = "kind", deny_unknown_fields)]
enum Target {
    Value { parameter: ObjectId },
    Edge { parameter: ObjectId, edge: ObjectId },
    Append { parameter: ObjectId },
    Output,
    Override { input: ObjectId },
    OverrideEdge { input: ObjectId, edge: ObjectId },
}
fn port(target: &Target, label: &str) -> PortView {
    PortView {
        key: serde_json::to_string(target).expect("native target"),
        label: label.into(),
        color: GRAPH_COLORS.image,
    }
}
fn source(reference: Reference) -> Socket {
    Socket {
        node: match reference {
            Reference::Input { input } => input,
            Reference::Node { node } => node,
            Reference::Instance { instance, .. } => instance,
        },
        key: serde_json::to_string(&reference).expect("native reference"),
    }
}
fn output(reference: Reference, label: &str) -> PortView {
    PortView {
        key: source(reference).key,
        label: label.into(),
        color: GRAPH_COLORS.source,
    }
}

pub(super) struct Context<'a> {
    pub state: &'a mut State,
    pub host: &'a mut dyn DesktopClient,
    pub definition: ObjectId,
}
impl GraphContext for Context<'_> {
    fn node_actions(&self, _node: ObjectId) -> Vec<(String, String)> {
        vec![("group-selection".into(), "Group selection".into())]
    }
    fn node_action(&mut self, node: ObjectId, action: &str) -> Result<(), String> {
        if action != "group-selection" {
            return Err("Unknown graph action".into());
        }
        let selected = &self.host.state().selection.objects;
        let members = if selected.contains(&node) {
            selected.clone()
        } else {
            vec![node]
        };
        let after = super::presentation::group(
            self.state.construction.as_ref().ok_or("No construction")?,
            self.definition,
            &members,
        )?;
        if self.state.commit(self.host, after) {
            Ok(())
        } else {
            Err(self.state.error.clone())
        }
    }
    fn graph(&self) -> GraphView {
        match view(
            self.state
                .construction
                .as_ref()
                .expect("active construction"),
            self.state.document.expect("active document"),
            self.definition,
            self.state.base.0,
            self.host.state().selection.objects.clone(),
        ) {
            Ok(graph) => graph,
            Err(error) => GraphView {
                key: GraphKey {
                    document: self.state.document.unwrap(),
                    group: Some(self.definition),
                },
                generation: self.state.base.0,
                nodes: vec![],
                wires: vec![],
                selected: vec![],
                has_parent: false,
                error,
            },
        }
    }
    fn catalog(&self) -> Vec<NodeTemplate> {
        registry::operations()
            .iter()
            .filter(|op| op.role == registry::Role::Operation && op.cpu_reference)
            .map(|op| NodeTemplate {
                key: serde_json::to_string(&(&op.key, op.version, &op.mode))
                    .expect("operation key"),
                label: if op.builtin == registry::Builtin::Screen {
                    "Placed Rectangle".into()
                } else {
                    op.title.clone()
                },
                category: if op.palette == registry::Palette::Artist {
                    "Content".into()
                } else {
                    "Animation and values".into()
                },
            })
            .collect()
    }
    fn validate(&self, change: &GraphChange) -> Result<(), String> {
        apply(
            self.state.construction.as_ref().ok_or("No construction")?,
            self.definition,
            std::slice::from_ref(change),
        )
        .map(|_| ())
    }
    fn event(&mut self, event: GraphEvent) -> Result<(), String> {
        match event {
            GraphEvent::Select(objects) => self.state.select(self.host, objects),
            GraphEvent::PreviewPositions(positions) => {
                let after = apply(
                    self.state.construction.as_ref().ok_or("No construction")?,
                    self.definition,
                    &[GraphChange::Positions(positions)],
                )?;
                self.state.construction = Some(after);
                self.state.editing = true;
            }
            GraphEvent::Commit(changes) => {
                let before = self.state.construction.as_ref().ok_or("No construction")?;
                let after = apply(before, self.definition, &changes)?;
                let selected = after
                    .definition(self.definition)?
                    .nodes
                    .iter()
                    .rev()
                    .find(|node| {
                        !before
                            .definition(self.definition)
                            .expect("existing asset")
                            .nodes
                            .iter()
                            .any(|old| old.id == node.id)
                    })
                    .map(|node| node.id);
                if !self.state.commit(self.host, after) {
                    return Err(self.state.error.clone());
                }
                if let Some(id) = selected {
                    self.state.select(self.host, vec![id]);
                }
            }
            GraphEvent::Cancel => self.state.cancel(self.host),
            GraphEvent::Navigate(target) => {
                let value = self.state.construction.as_ref().ok_or("No construction")?;
                let destination = if let Some(id) = target {
                    let destination = value
                        .definition(self.definition)?
                        .instances
                        .iter()
                        .find(|i| i.id == id)
                        .ok_or("Not an asset instance")?
                        .definition;
                    self.state.graph_history.push(self.definition);
                    destination
                } else {
                    self.state
                        .graph_history
                        .pop()
                        .unwrap_or(value.root.definition)
                };
                self.state.graph_definition = Some(destination);
                self.state.select(self.host, vec![]);
            }
        }
        Ok(())
    }
}
