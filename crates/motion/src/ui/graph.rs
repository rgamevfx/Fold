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
fn source(reference: Reference, definition: ObjectId) -> Socket {
    Socket {
        node: match reference {
            Reference::Parameter { parameter, .. } => parameter,
            Reference::Input { .. } => definition,
            Reference::Node { node } => node,
            Reference::Instance { instance, .. } => instance,
        },
        key: serde_json::to_string(&reference).expect("native reference"),
    }
}
fn output(reference: Reference, label: &str) -> PortView {
    PortView {
        key: serde_json::to_string(&reference).expect("native reference"),
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
    fn node_actions(&self, node: ObjectId) -> Vec<(String, String)> {
        let mut actions = vec![("group-selection".into(), "Group selection".into())];
        if node == self.definition {
            actions.push(("add-parameter".into(), "Add Parameter".into()));
        }
        actions
    }
    fn node_action(&mut self, node: ObjectId, action: &str) -> Result<(), String> {
        if action == "add-parameter" && node == self.definition {
            let mut after = self.state.construction.clone().ok_or("No construction")?;
            let definition = after
                .definitions
                .iter_mut()
                .find(|d| d.id == self.definition)
                .ok_or("Missing graph")?;
            let mut name = "Parameter".to_string();
            let mut suffix = 2;
            while definition.inputs.iter().any(|i| i.name == name)
                || definition.nodes.iter().any(|n| n.name == name)
                || definition.instances.iter().any(|i| i.name == name)
            {
                name = format!("Parameter {suffix}");
                suffix += 1;
            }
            definition.inputs.push(crate::model::Input {
                id: ObjectId::new(),
                name,
                ty: crate::types::Type::Scalar,
                default: crate::types::Literal::Scalar { value: 0. },
                protected: false,
                extensions: Default::default(),
            });
            if self.state.commit(self.host, after) {
                return Ok(());
            }
            return Err(self.state.error.clone());
        }
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
        let has_image = self
            .state
            .construction
            .as_ref()
            .and_then(|c| c.definition(self.definition).ok())
            .is_some_and(|d| {
                d.nodes
                    .iter()
                    .any(|n| n.operation == "fold.motion.image_texture")
            });
        let mut entries: Vec<_> = registry::operations()
            .iter()
            .filter(|op| {
                op.role == registry::Role::Operation
                    && op.cpu_reference
                    && op.builtin != registry::Builtin::ImageTexture
                    && (has_image
                        || !matches!(
                            op.builtin,
                            registry::Builtin::SampleImageColor
                                | registry::Builtin::SampleImageScalar
                        ))
            })
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
            .collect();
        for (source, label) in super::images::sources(self.host, self.state.document) {
            let mode = if matches!(source, fold_platform::image_source::Source::Asset(_)) {
                "asset"
            } else {
                "output"
            };
            entries.push(NodeTemplate {
                key: serde_json::to_string(&("fold.motion.image_texture", 1, mode, source))
                    .unwrap(),
                label,
                category: "Image Texture".into(),
            });
        }
        entries
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
                    self.state.graph_occurrences.push(id);
                    destination
                } else {
                    self.state.graph_occurrences.pop();
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
