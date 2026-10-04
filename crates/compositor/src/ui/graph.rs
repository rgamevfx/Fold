//! Composite semantics adapted to the shared editor; no native UI machinery.
use super::state::State;
use crate::{Composite, Node, Parameters as P, PortType, Source};
use fold_foundation::{ObjectId, Time};
use fold_platform::desktop::{DesktopClient, DesktopCommand, Selection};
use fold_project::DocumentRef;
use fold_ui::sdk::{GRAPH_COLORS, graph_canvas::*};

pub(super) struct Context<'a> {
    pub state: &'a mut State,
    pub host: &'a mut dyn DesktopClient,
}
impl Context<'_> {
    fn templates(&self) -> Vec<(NodeTemplate, P)> {
        let graph = self.state.graph.as_ref().unwrap();
        let rect = [0, 0, graph.info.width, graph.info.height];
        let mut templates: Vec<_> = vec![
            P::Read {
                source: Source::Unassigned {
                    info: graph.info.clone(),
                },
                start: Time::ZERO,
                source_start: Time::ZERO,
                duration: graph.info.time(graph.info.frames).unwrap(),
            },
            P::Solid {
                rgba: [0.2, 0.3, 0.5, 1.],
            },
            P::TransformImage {
                settings: crate::Transform {
                    pivot: [graph.info.width as f64 / 2., graph.info.height as f64 / 2.],
                    ..Default::default()
                },
            },
            P::Crop { rect },
            P::GaussianBlur {
                size: [3.; 2],
                edges: Default::default(),
            },
            P::ColorGrade {
                settings: Default::default(),
            },
            P::Shape {
                shape: crate::Shape::Rectangle,
                bounds: rect.map(f64::from),
            },
            P::Shape {
                shape: crate::Shape::Ellipse,
                bounds: rect.map(f64::from),
            },
            P::Unary {
                operation: fold_render::operations::Unary::Exposure { stops: 0. },
            },
            P::Unary {
                operation: fold_render::operations::Unary::Invert,
            },
            P::Unary {
                operation: fold_render::operations::Unary::Clamp {
                    minimum: 0.,
                    maximum: 1.,
                },
            },
            P::Unary {
                operation: fold_render::operations::Unary::Premult,
            },
            P::Unary {
                operation: fold_render::operations::Unary::Unpremult,
            },
            P::ApplyMask,
            P::Composite {
                mode: Default::default(),
            },
            P::Shuffle { mappings: vec![] },
            P::RemoveChannels {
                channels: vec![],
                keep: false,
            },
        ]
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
                if let Ok(info) = self.host.video_info(d.id)
                    && let Ok(duration) = info.time(info.frames)
                {
                    templates.push((
                        NodeTemplate {
                            key: format!("document:{:?}", d.id),
                            label: format!("Read {} {:?}", d.type_id, d.id),
                            category: "Document output".into(),
                        },
                        P::Read {
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
        for node in &graph.nodes {
            if let P::Read {
                source: Source::Asset { asset, .. } | Source::Exr { asset, .. },
                ..
            } = &node.parameters
            {
                templates.push((
                    NodeTemplate {
                        key: format!("media:{:?}", node.id),
                        label: format!("Read media {asset:?}"),
                        category: "Media".into(),
                    },
                    node.parameters.clone(),
                ));
            }
        }
        templates
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
                    .parameters
                    .operator()
                    .output_key()
                    != wire.source.key
                {
                    return Err("Unknown output socket".into());
                }
                graph.connect(wire.source.node, wire.target.node, &wire.target.key)?;
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
                    .templates()
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
    fn graph(&self) -> GraphView {
        let graph = self.state.graph.as_ref().unwrap();
        GraphView {
            key: GraphKey {
                document: self.state.document.unwrap(),
                group: None,
            },
            generation: self.state.generation,
            selected: self.state.selected.clone(),
            has_parent: false,
            error: self.state.error.clone(),
            nodes: graph
                .nodes
                .iter()
                .map(|n| {
                    let op = n.parameters.operator();
                    NodeView {
                        id: n.id,
                        label: op.label().to_uppercase(),
                        summary: summary(&n.parameters),
                        position: n.position,
                        can_open: false,
                        color: match n.parameters {
                            P::Read { .. } | P::Solid { .. } => GRAPH_COLORS.source,
                            P::Merge | P::Composite { .. } => GRAPH_COLORS.merge,
                            _ => color(op.output),
                        },
                        inputs: op
                            .inputs
                            .iter()
                            .enumerate()
                            .map(|(i, kind)| PortView {
                                key: op.input_key(i).into(),
                                label: op.input_label(i).into(),
                                color: color(*kind),
                            })
                            .chain(op.supports_effect().then(|| PortView {
                                key: "mask".into(),
                                label: "mask".into(),
                                color: GRAPH_COLORS.mask,
                            }))
                            .collect(),
                        outputs: if matches!(n.parameters, P::Output) {
                            vec![]
                        } else {
                            vec![PortView {
                                key: op.output_key().into(),
                                label: op.output_key().into(),
                                color: color(op.output),
                            }]
                        },
                    }
                })
                .collect(),
            wires: graph
                .nodes
                .iter()
                .flat_map(|n| {
                    n.inputs
                        .iter()
                        .enumerate()
                        .filter_map(|(i, source)| {
                            source.map(|source| WireView {
                                source: Socket {
                                    node: source,
                                    key: graph
                                        .node(source)
                                        .unwrap()
                                        .parameters
                                        .operator()
                                        .output_key()
                                        .into(),
                                },
                                target: Socket {
                                    node: n.id,
                                    key: n.parameters.operator().input_key(i).into(),
                                },
                            })
                        })
                        .chain(n.effect.mask.map(|source| {
                            WireView {
                                source: Socket {
                                    node: source,
                                    key: graph
                                        .node(source)
                                        .unwrap()
                                        .parameters
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
        self.templates().into_iter().map(|(t, _)| t).collect()
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
            GraphEvent::Navigate(_) => return Err("This node has no editable group".into()),
        }
        Ok(())
    }
}
fn color(kind: PortType) -> [f32; 4] {
    match kind {
        PortType::Mask => GRAPH_COLORS.mask,
        _ => GRAPH_COLORS.image,
    }
}
fn summary(p: &P) -> String {
    match p {
        P::Unary { operation } => operation.label().into(),
        P::Shape { shape, .. } => shape.label().into(),
        P::GaussianBlur { size, .. } => format!("Gaussian {:.1} × {:.1} px", size[0], size[1]),
        P::TransformImage { .. } => "2D transform".into(),
        P::ColorGrade { .. } => "Color grade".into(),
        P::Composite { mode } => mode.label().into(),
        P::Shuffle { mappings } => format!("{} channel mappings", mappings.len()),
        P::RemoveChannels { keep, .. } => {
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
        P::Blur { radius } => format!("Radius {radius} px"),
        P::Grade { gain } => format!("RGB {:.2} / {:.2} / {:.2}", gain[0], gain[1], gain[2]),
        P::Transform { opacity, .. } => format!("Opacity {:.0}%", opacity * 100.),
        P::Merge => "Foreground over background".into(),
        P::Output => "Document video output".into(),
        P::Mask { .. } => "Rectangle • mask".into(),
        P::ApplyMask => "Premultiplied alpha".into(),
        P::Crop { .. } => "Transparent outside bounds".into(),
        P::Solid { .. } => "Linear RGBA".into(),
    }
}
