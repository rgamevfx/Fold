//! Composite semantics adapted to the shared editor; no native UI machinery.
use super::state::State;
use crate::{Composite, Node, OperationConfig as P, Source};
use fold_foundation::{ObjectId, Time};
use fold_graph::types::Type;
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_project::DocumentRef;
use fold_ui::sdk::{GRAPH_COLORS, graph_canvas::*};

pub(super) struct Context<'a> {
    pub state: &'a mut State,
    pub host: &'a mut dyn DesktopClient,
}
impl Context<'_> {
    fn templates(&self) -> Result<Vec<(NodeTemplate, P)>, String> {
        let graph = self.state.graph.as_ref().unwrap();
        let mut templates: Vec<_> = crate::OPERATORS
            .iter()
            .filter(|op| !matches!(op.id, "Input" | "Effect" | "Dissolve" | "Output" | "Mask"))
            .map(|op| op.defaults(&graph.info).expect("registered default"))
            .collect::<Vec<_>>()
            .into_iter()
            .map(|p| {
                (
                    NodeTemplate {
                        key: p.operator().id.into(),
                        label: p.operator().label().into(),
                        category: "Image".into(),
                    },
                    p,
                )
            })
            .collect();
        if let Some(snapshot) = self.host.snapshot() {
            for d in snapshot
                .state()
                .documents
                .values()
                .filter(|d| Some(d.id) != self.state.document)
            {
                if let Ok(interface) = self.host.effect_interface(d.id) {
                    let reference = DocumentRef {
                        document: d.id,
                        output: "video".into(),
                        extensions: Default::default(),
                    };
                    templates.push((
                        NodeTemplate {
                            key: format!("effect:{:?}", d.id),
                            label: fold_ui::sdk::effects::name(self.host, &reference),
                            category: "Effects".into(),
                        },
                        P::Instance {
                            instance: fold_ui::sdk::effects::new_instance(reference, &interface),
                            images: interface.images,
                        },
                    ));
                    continue;
                }
                if let Ok(info) = self.host.video_info(d.id)
                    && let Ok(duration) = info.time(info.frames)
                {
                    templates.push((
                        NodeTemplate {
                            key: format!("document:{:?}", d.id),
                            label: format!(
                                "{}: {}",
                                self.host
                                    .document_kinds()
                                    .iter()
                                    .find(|k| k.type_id == d.type_id)
                                    .map(|k| k.title)
                                    .unwrap_or("Document"),
                                snapshot
                                    .state()
                                    .organization
                                    .items
                                    .iter()
                                    .find(|item| item.id == fold_project::ItemId::Document(d.id))
                                    .map(|item| item.name.as_str())
                                    .unwrap_or("Untitled")
                            ),
                            category: "Image references".into(),
                        },
                        P::Read {
                            options: Default::default(),
                            source: Source::Document {
                                source: DocumentRef {
                                    document: d.id,
                                    output: "video".into(),
                                    extensions: Default::default(),
                                },
                                info,
                            },
                            start: Time::ZERO,
                            source_start: Time::ZERO,
                            duration,
                        },
                    ));
                }
            }
        }
        let prepared = graph.prepare()?;
        for node in prepared.nodes() {
            if let P::Read {
                source: Source::Asset { asset, .. } | Source::Exr { asset, .. },
                ..
            } = node.config()
            {
                templates.push((
                    NodeTemplate {
                        key: format!("media:{:?}", node.id),
                        label: format!("Read media {asset:?}"),
                        category: "Media".into(),
                    },
                    node.config().clone(),
                ));
            }
        }
        Ok(templates)
    }
    fn apply(
        &self,
        graph: &mut Composite,
        change: &GraphChange,
    ) -> Result<Option<ObjectId>, String> {
        match change {
            GraphChange::Connect(wire) => {
                if graph
                    .node(wire.source.node)?
                    .prepare()?
                    .config()
                    .operator()
                    .output_key()
                    != wire.source.key
                {
                    return Err("Unknown output socket".into());
                }
                graph.connect(wire.source.node, wire.target.node, &wire.target.key)?;
            }
            GraphChange::Rewire { remove, add } => {
                for socket in remove {
                    self.apply(graph, &GraphChange::Disconnect(socket.clone()))?;
                }
                for wire in add {
                    self.apply(graph, &GraphChange::Connect(wire.clone()))?;
                }
            }
            GraphChange::Disconnect(socket) => graph.disconnect(socket.node, &socket.key)?,
            GraphChange::Delete(ids) => graph.remove(ids)?,
            GraphChange::Positions(positions) => {
                for &(id, position) in positions {
                    graph.node_mut(id)?.position = Some(position);
                }
            }
            GraphChange::Add { template, position } => {
                let parameters = self
                    .templates()?
                    .into_iter()
                    .find(|(t, _)| &t.key == template)
                    .ok_or("Node template is no longer available")?
                    .1;
                let mut node = Node::disconnected(parameters);
                node.position = Some(*position);
                let id = node.id;
                graph.nodes.push(node);
                return Ok(Some(id));
            }
        }
        Ok(None)
    }
}
impl GraphContext for Context<'_> {
    fn presentation(&self) -> GraphPresentation {
        GraphPresentation::VerticalCompact
    }
    fn primary_input<'a>(&self, node: &'a NodeView) -> Option<&'a str> {
        ["source", "outgoing", "background", "image", "A"]
            .into_iter()
            .find(|key| node.inputs.iter().any(|port| port.key == *key))
    }

    fn graph(&self) -> GraphView {
        let graph = self.state.graph.as_ref().unwrap();
        let prepared = graph.prepare();
        let error = prepared
            .as_ref()
            .err()
            .cloned()
            .unwrap_or_else(|| self.state.error.clone());
        let nodes = prepared.as_ref().map(|p| p.nodes()).unwrap_or(&[]);
        GraphView {
            key: GraphKey {
                document: self.state.document.unwrap(),
                group: None,
            },
            generation: self.state.generation,
            selected: self.state.selected.clone(),
            has_parent: false,
            error,
            nodes: nodes
                .iter()
                .map(|n| {
                    let config = n.config();
                    let op = config.operator();
                    NodeView {
                        id: n.id,
                        label: if let P::Read {
                            source: Source::Document { source, .. },
                            ..
                        } = n.config()
                        {
                            self.host
                                .snapshot()
                                .and_then(|s| {
                                    s.state()
                                        .documents
                                        .get(&source.document)
                                        .map(|d| d.type_id.clone())
                                })
                                .and_then(|kind| {
                                    self.host
                                        .document_kinds()
                                        .iter()
                                        .find(|k| k.type_id == kind)
                                        .map(|k| k.title.to_string())
                                })
                                .unwrap_or("Document".into())
                        } else {
                            if let P::Instance { instance, .. } = n.config() {
                                fold_ui::sdk::effects::name(self.host, &instance.definition)
                            } else {
                                op.label().to_string()
                            }
                        },
                        summary: summary(n.config()),
                        position: n.position,
                        can_open: matches!(
                            n.config(),
                            P::Read {
                                source: Source::Document { .. },
                                ..
                            }
                        ),
                        color: match n.config() {
                            P::Read { .. } | P::Solid { .. } => GRAPH_COLORS.source,
                            P::Composite { .. } => GRAPH_COLORS.merge,
                            _ => color(op.output.clone()),
                        },
                        inputs: (0..n.config().input_count())
                            .map(|i| PortView {
                                key: n.config().input_key(i).into(),
                                label: match (op.id, i) {
                                    ("Merge", 0) => "A · Foreground",
                                    ("Merge", 1) => "B · Background",
                                    _ => config.input_key(i),
                                }
                                .into(),
                                color: color(n.config().input_type(i)),
                            })
                            .chain(op.supports_effect().then(|| PortView {
                                key: "mask".into(),
                                label: "mask".into(),
                                color: GRAPH_COLORS.mask,
                            }))
                            .collect(),
                        outputs: if matches!(n.config(), P::Output) {
                            vec![]
                        } else {
                            vec![PortView {
                                key: op.output_key().into(),
                                label: op.output_key().into(),
                                color: color(op.output.clone()),
                            }]
                        },
                    }
                })
                .collect(),
            wires: nodes
                .iter()
                .flat_map(|n| {
                    n.input_ids()
                        .into_iter()
                        .enumerate()
                        .filter_map(|(i, source)| {
                            source.map(|source| WireView {
                                source: Socket {
                                    node: source,
                                    key: prepared
                                        .as_ref()
                                        .unwrap()
                                        .node(source)
                                        .unwrap()
                                        .config()
                                        .operator()
                                        .output_key()
                                        .into(),
                                },
                                target: Socket {
                                    node: n.id,
                                    key: n.config().input_key(i).into(),
                                },
                            })
                        })
                        .chain(n.effect.mask_node().map(|source| {
                            WireView {
                                source: Socket {
                                    node: source,
                                    key: prepared
                                        .as_ref()
                                        .unwrap()
                                        .node(source)
                                        .unwrap()
                                        .config()
                                        .operator()
                                        .output_key()
                                        .into(),
                                },
                                target: Socket {
                                    node: n.id,
                                    key: "mask".into(),
                                },
                            }
                        }))
                })
                .collect(),
        }
    }
    fn catalog(&self) -> Vec<NodeTemplate> {
        self.templates()
            .unwrap_or_default()
            .into_iter()
            .map(|(t, _)| t)
            .collect()
    }
    fn validate(&self, change: &GraphChange) -> Result<(), String> {
        let mut graph = self.state.graph.clone().ok_or("No composite graph")?;
        self.apply(&mut graph, change)?;
        graph.validate()
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
                let mut graph = self.state.graph.clone().ok_or("No composite graph")?;
                self.apply(&mut graph, &GraphChange::Positions(positions))?;
                self.state.graph = Some(graph);
                self.state.editing = true;
            }
            GraphEvent::Commit(changes) => {
                let mut graph = self.state.graph.clone().ok_or("No composite graph")?;
                let mut selected = None;
                for change in &changes {
                    selected = self.apply(&mut graph, change)?.or(selected);
                }
                graph.validate()?;
                self.state.graph = Some(graph);
                self.state.commit(self.host);
                if !self.state.error.is_empty() {
                    return Err(self.state.error.clone());
                }
                if let Some(id) = selected {
                    self.event(GraphEvent::Select(vec![id]))?;
                }
            }
            GraphEvent::Cancel => self.state.cancel(self.host),
            GraphEvent::Navigate(Some(id)) => {
                let node = self.state.graph.as_ref().ok_or("No composite")?.node(id)?;
                let time = self
                    .host
                    .state()
                    .navigation
                    .last()
                    .map(|v| v.time)
                    .unwrap_or(Time::ZERO);
                let location = super::read::source_location(node, time)
                    .ok_or("Source is unavailable or inactive at this time")?;
                self.state.cancel(self.host);
                self.host.command(DesktopCommand::Navigate(location));
            }
            GraphEvent::Navigate(None) => {
                return Err("Use the document breadcrumb to return to the parent".into());
            }
        }
        Ok(())
    }
}
fn color(kind: Type) -> [f32; 4] {
    match kind {
        Type::Mask => GRAPH_COLORS.mask,
        _ => GRAPH_COLORS.image,
    }
}
fn summary(p: &P) -> String {
    match p {
        P::Shader { .. } => "GPU image shader".into(),
        P::RelativeTransform { .. } => "Relative image placement".into(),
        P::Input { id } => format!("Input: {id}"),
        P::Instance { .. } => "Reusable effect".into(),
        P::Dissolve { .. } => "Duration-driven mix".into(),
        P::Unary { operation } => operation.label().into(),
        P::Shape { shape, .. } => shape.label().into(),
        P::Blur { size, filter, .. } => {
            format!("{} {:.1} × {:.1} px", filter.label(), size[0], size[1])
        }
        P::TransformImage { .. } => "2D transform".into(),
        P::ColorGrade { .. } => "Color grade".into(),
        P::Composite { mode, .. } => mode.label().into(),
        P::Shuffle { mappings } => format!("{} channel mappings", mappings.len()),
        P::CullChannels { keep, .. } => {
            if *keep {
                "Keep selected channels".into()
            } else {
                "Remove selected channels".into()
            }
        }
        P::Read { source, .. } => match source {
            Source::Unassigned { .. } => "Choose a source file".into(),
            Source::Asset { .. } => "Media source".into(),
            Source::Exr { image, .. } => format!("OpenEXR • {} channels", image.channels.len()),
            Source::Document { .. } => "Nested document".into(),
        },
        P::Write { settings } => settings.destination.rsplit('/').next().unwrap_or("").into(),
        P::Output => "Document video output".into(),
        P::Mask { .. } => "Rectangle • mask".into(),
        P::Crop { .. } => "Transparent outside bounds".into(),
        P::Solid { .. } => "Linear RGBA".into(),
    }
}
