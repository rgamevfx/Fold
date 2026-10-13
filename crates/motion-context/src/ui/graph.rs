//! The graph is a projection of native objects and their ordered group membership.
//! All topology events become the same canvas edits used by artist/agent authoring.
use super::state::State;
use fold_foundation::ObjectId;
use fold_motion_canvas::{
    editing::{self, Edit},
    hierarchy::{children, placement},
    model::{Canvas, Content},
    tools,
};
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_ui::sdk::{GRAPH_COLORS, graph_canvas::*};

pub(super) struct Context<'a> {
    pub state: &'a mut State,
    pub host: &'a mut dyn DesktopClient,
}
impl GraphContext for Context<'_> {
    fn can_insert(&self, _node: ObjectId) -> bool {
        false
    }
    fn primary_input<'a>(&self, node: &'a NodeView) -> Option<&'a str> {
        node.inputs
            .iter()
            .find(|p| p.key == "add")
            .map(|p| p.key.as_str())
    }
    fn graph(&self) -> GraphView {
        let canvas = self.state.canvas().expect("active canvas");
        let mut visible: std::collections::BTreeSet<_> = children(canvas, self.state.graph_group)
            .unwrap_or_default()
            .iter()
            .copied()
            .collect();
        if let Some(group) = self.state.graph_group {
            visible.insert(group);
        }
        let mut wires = vec![];
        let nodes = visible
            .iter()
            .filter_map(|id| canvas.objects.get(id).map(|object| (id, object)))
            .map(|(&id, object)| {
                let mut inputs = vec![];
                if let Content::Group { children, .. } = &object.content {
                    let members = if Some(id) == self.state.graph_group {
                        children.as_slice()
                    } else {
                        &[]
                    };
                    for (index, child) in members.iter().enumerate() {
                        let key = format!("child:{child}");
                        inputs.push(PortView {
                            key: key.clone(),
                            label: format!("{} · {}", index + 1, canvas.objects[child].name),
                            color: GRAPH_COLORS.source,
                        });
                        wires.push(WireView {
                            source: Socket {
                                node: *child,
                                key: "content".into(),
                            },
                            target: Socket { node: id, key },
                        });
                    }
                    inputs.push(PortView {
                        key: "add".into(),
                        label: "Add object".into(),
                        color: GRAPH_COLORS.source,
                    });
                }
                let kind = kind(&object.content);
                let detail = if let Content::Script { controls, .. } = &object.content {
                    format!("Script · {} controls", controls.len())
                } else if let Content::Group { children, .. } = &object.content {
                    format!("Group · {} objects", children.len())
                } else {
                    kind.to_owned()
                };
                NodeView {
                    id,
                    label: object.name.clone(),
                    summary: detail,
                    color: GRAPH_COLORS.source,
                    position: object.graph_position,
                    inputs,
                    outputs: vec![PortView {
                        key: "content".into(),
                        label: "Object".into(),
                        color: GRAPH_COLORS.source,
                    }],
                    can_open: matches!(object.content, Content::Group { .. })
                        && Some(id) != self.state.graph_group,
                }
            })
            .collect();
        GraphView {
            key: GraphKey {
                document: self.state.document.expect("active document"),
                group: self.state.graph_group,
            },
            generation: self.state.generation,
            nodes,
            wires,
            selected: self
                .state
                .selected
                .iter()
                .copied()
                .filter(|id| visible.contains(id))
                .collect(),
            has_parent: self.state.graph_group.is_some(),
            error: self.state.error.clone(),
        }
    }
    fn catalog(&self) -> Vec<NodeTemplate> {
        [
            ("rectangle", "Rectangle"),
            ("ellipse", "Ellipse"),
            ("path", "Path"),
            ("text", "Text"),
            ("group", "Group"),
            ("script", "Script"),
        ]
        .into_iter()
        .map(|(key, label)| NodeTemplate {
            key: key.into(),
            label: label.into(),
            category: "Objects".into(),
        })
        .collect()
    }
    fn validate(&self, change: &GraphChange) -> Result<(), String> {
        changes_in(
            self.state.canvas().ok_or("No Motion document")?,
            std::slice::from_ref(change),
            self.state.graph_group,
        )
        .map(|_| ())
    }
    fn event(&mut self, event: GraphEvent) -> Result<(), String> {
        if self.state.busy() {
            return Err("Wait for the current Motion edit".into());
        }
        match event {
            GraphEvent::Select(ids) => {
                if self.state.editing
                    && !self
                        .state
                        .edits
                        .iter()
                        .all(|e| matches!(e, Edit::GraphPosition { .. }))
                {
                    self.state.cancel(self.host);
                }
                self.state.selected = ids.clone();
                self.host.command(DesktopCommand::Select(Selection {
                    document: self.state.document,
                    objects: ids,
                }));
            }
            GraphEvent::PreviewPositions(positions) => {
                if self.state.editing
                    && !self
                        .state
                        .edits
                        .iter()
                        .all(|e| matches!(e, Edit::GraphPosition { .. }))
                {
                    self.state.cancel(self.host);
                }
                let (edits, _) = changes(
                    self.state.before.as_ref().ok_or("No Motion document")?,
                    &[GraphChange::Positions(positions)],
                )?;
                self.state.preview(self.host, edits)?;
            }
            GraphEvent::Commit(batch) => {
                let (edits, created) = changes_in(
                    self.state.canvas().ok_or("No Motion document")?,
                    &batch,
                    self.state.graph_group,
                )?;
                let mut combined = self.state.edits.clone();
                combined.extend(edits);
                self.state.preview(self.host, combined)?;
                let selected = if !created.is_empty() {
                    created
                } else {
                    self.state
                        .selected
                        .iter()
                        .filter(|id| {
                            self.state
                                .canvas()
                                .is_some_and(|c| c.objects.contains_key(id))
                        })
                        .copied()
                        .collect()
                };
                self.state.commit(self.host)?;
                if selected != self.state.selected {
                    self.state.selected = selected.clone();
                    self.host.command(DesktopCommand::Select(Selection {
                        document: self.state.document,
                        objects: selected,
                    }));
                }
            }
            GraphEvent::Cancel => self.state.cancel(self.host),
            GraphEvent::Navigate(target) => {
                let canvas = self.state.canvas().ok_or("No Motion document")?;
                let group = match target {
                    Some(id) => {
                        children(canvas, Some(id))?;
                        Some(id)
                    }
                    None => self
                        .state
                        .graph_group
                        .map(|id| placement(canvas, id).map(|p| p.0))
                        .transpose()?
                        .flatten(),
                };
                self.state.cancel(self.host);
                self.state.graph_group = group;
            }
        }
        Ok(())
    }
    fn node_actions(&self, node: ObjectId) -> Vec<(String, String)> {
        if self
            .state
            .canvas()
            .is_none_or(|c| editing::editable(c, node).is_err())
        {
            return vec![];
        }
        vec![
            ("forward".into(), "Bring forward".into()),
            ("backward".into(), "Send backward".into()),
        ]
    }
    fn node_action(&mut self, node: ObjectId, action: &str) -> Result<(), String> {
        let canvas = self.state.canvas().ok_or("No Motion document")?;
        let (parent, index) = placement(canvas, node)?;
        let length = children(canvas, parent)?.len();
        let index = match action {
            "forward" => (index + 1).min(length - 1),
            "backward" => index.saturating_sub(1),
            _ => return Err("Unknown Motion object action".into()),
        };
        let mut edits = self.state.edits.clone();
        edits.push(Edit::Move {
            id: node,
            parent,
            index,
        });
        self.state.preview(self.host, edits)?;
        self.state.commit(self.host)
    }
}

pub(super) fn kind(content: &Content) -> &'static str {
    match content {
        Content::Rectangle { .. } => "Rectangle",
        Content::Ellipse { .. } => "Ellipse",
        Content::Path { .. } => "Path",
        Content::Text { .. } => "Text",
        Content::Image { .. } => "Image",
        Content::Group { .. } => "Group",
        Content::Script { .. } => "Script",
    }
}
fn destination(canvas: &Canvas, target: &Socket) -> Result<usize, String> {
    let members = children(canvas, Some(target.node))?;
    if target.key == "add" {
        return Ok(members.len());
    }
    members
        .iter()
        .position(|id| target.key == format!("child:{id}"))
        .ok_or("Missing group input".into())
}
fn push(canvas: &mut Canvas, edits: &mut Vec<Edit>, edit: Edit) -> Result<(), String> {
    *canvas = editing::apply(canvas, std::slice::from_ref(&edit))?;
    edits.push(edit);
    Ok(())
}
fn disconnect(canvas: &mut Canvas, edits: &mut Vec<Edit>, socket: &Socket) -> Result<(), String> {
    if socket.key == "add" {
        return Err("The Add object port has no connection".into());
    }
    let index = destination(canvas, socket)?;
    let id = children(canvas, Some(socket.node))?[index];
    push(
        canvas,
        edits,
        Edit::Move {
            id,
            parent: None,
            index: canvas.roots.len(),
        },
    )
}
fn connect(
    canvas: &mut Canvas,
    edits: &mut Vec<Edit>,
    wire: &WireView,
    index: usize,
) -> Result<(), String> {
    if wire.source.key != "content" {
        return Err("Missing Motion object output".into());
    }
    let (parent, source_index) = placement(canvas, wire.source.node)?;
    let mut index = index.min(children(canvas, Some(wire.target.node))?.len());
    if parent == Some(wire.target.node) && source_index < index {
        index -= 1;
    }
    push(
        canvas,
        edits,
        Edit::Move {
            id: wire.source.node,
            parent: Some(wire.target.node),
            index,
        },
    )
}

/// Stage a graph batch locally. Only the caller publishes the resulting edits.
pub(super) fn changes(
    before: &Canvas,
    batch: &[GraphChange],
) -> Result<(Vec<Edit>, Vec<ObjectId>), String> {
    changes_in(before, batch, None)
}
fn changes_in(
    before: &Canvas,
    batch: &[GraphChange],
    parent: Option<ObjectId>,
) -> Result<(Vec<Edit>, Vec<ObjectId>), String> {
    let mut canvas = before.clone();
    let mut edits = vec![];
    let mut created = vec![];
    for change in batch {
        match change {
            GraphChange::Add { template, position } => {
                let mut object = tools::new_object(template)?;
                object.graph_position = Some(*position);
                let id = ObjectId::new();
                let index = children(&canvas, parent)?.len();
                push(
                    &mut canvas,
                    &mut edits,
                    Edit::Insert {
                        id,
                        object: Box::new(object),
                        parent,
                        index,
                    },
                )?;
                created.push(id);
            }
            GraphChange::Positions(positions) => {
                for (id, position) in positions {
                    let expected = canvas
                        .objects
                        .get(id)
                        .ok_or("Missing Motion object")?
                        .graph_position;
                    push(
                        &mut canvas,
                        &mut edits,
                        Edit::GraphPosition {
                            object: *id,
                            expected,
                            value: Some(*position),
                        },
                    )?;
                }
            }
            GraphChange::Delete(ids) => {
                if ids.iter().any(|id| !canvas.objects.contains_key(id)) {
                    return Err("Missing Motion object".into());
                }
                for id in ids {
                    if let Some(object) = canvas.objects.get(id).cloned() {
                        push(
                            &mut canvas,
                            &mut edits,
                            Edit::Delete {
                                id: *id,
                                expected: Box::new(object),
                            },
                        )?;
                    }
                }
            }
            GraphChange::Disconnect(socket) => disconnect(&mut canvas, &mut edits, socket)?,
            GraphChange::Connect(wire) => {
                let index = destination(&canvas, &wire.target)?;
                if wire.target.key != "add" {
                    disconnect(&mut canvas, &mut edits, &wire.target)?;
                }
                connect(&mut canvas, &mut edits, wire, index)?;
            }
            GraphChange::Rewire { remove, add } => {
                // Remember the target slots while their old connections still exist.
                let order = canvas
                    .objects
                    .iter()
                    .filter_map(|(&id, object)| {
                        if let Content::Group { children, .. } = &object.content {
                            Some((id, children.clone()))
                        } else {
                            None
                        }
                    })
                    .collect::<std::collections::BTreeMap<_, _>>();
                let mut slots = add
                    .iter()
                    .map(|wire| {
                        destination(&canvas, &wire.target).map(|index| (wire.clone(), index))
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                for socket in remove {
                    disconnect(&mut canvas, &mut edits, socket)?;
                }
                slots.sort_by_key(|(wire, index)| (wire.target.node, *index));
                let mut inserted: std::collections::BTreeMap<ObjectId, Vec<(usize, ObjectId)>> =
                    Default::default();
                for (wire, index) in slots {
                    if wire.target.key != "add" && destination(&canvas, &wire.target).is_ok() {
                        disconnect(&mut canvas, &mut edits, &wire.target)?;
                    }
                    let members = children(&canvas, Some(wire.target.node))?;
                    let previous = inserted.entry(wire.target.node).or_default();
                    let insertion = if wire.target.key == "add" {
                        members.len()
                    } else {
                        order[&wire.target.node][..index]
                            .iter()
                            .filter(|id| {
                                members.contains(id)
                                    && !previous.iter().any(|(_, added)| added == *id)
                            })
                            .count()
                            + previous
                                .iter()
                                .filter(|(slot, id)| *slot < index && members.contains(id))
                                .count()
                    };
                    connect(&mut canvas, &mut edits, &wire, insertion)?;
                    previous.push((index, wire.source.node));
                }
            }
        }
    }
    editing::apply(before, &edits)?;
    Ok((edits, created))
}
