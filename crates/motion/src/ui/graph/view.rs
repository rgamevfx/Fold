use super::{Target, output, port, source};
use crate::{
    binding::Binding,
    model::{Construction, Reference},
    registry,
};
use fold_foundation::ObjectId;
use fold_ui::sdk::{GRAPH_COLORS, graph_canvas::*};

pub(in crate::ui) fn view(
    value: &Construction,
    document: fold_foundation::DocumentId,
    definition: ObjectId,
    generation: u64,
    selected: Vec<ObjectId>,
) -> Result<GraphView, String> {
    let definition = value.definition(definition)?;
    let locked = crate::protection::locked_ids(value).map_err(|e| e.reason)?;
    for id in crate::reconcile::identities(definition)?.keys() {
        if value
            .presentation
            .positions
            .get(id)
            .is_some_and(|position| !position.iter().all(|v| (*v as f32).is_finite()))
        {
            return Err(
                "A graph position exceeds the editor display range. Edit it in Source.".into(),
            );
        }
    }
    let mut nodes = vec![];
    let mut wires = vec![];
    let node_view = |id, name: &str, summary: &str, inputs, outputs, can_open| NodeView {
        id,
        label: name.into(),
        summary: {
            let mut summary = summary.to_string();
            if locked.contains(&id) {
                summary.push_str(" · Locked");
            }
            for group in &value.presentation.groups {
                if group.members.contains(&id) {
                    summary.push_str(&format!("\nGroup: {}", group.name));
                }
            }
            if let Some(note) = value.presentation.annotations.get(&id) {
                summary.push_str(&format!("\n{note}"));
            }
            summary
        },
        color: GRAPH_COLORS.image,
        position: value
            .presentation
            .positions
            .get(&id)
            .map(|p| [p[0] as f32, p[1] as f32]),
        inputs,
        outputs,
        can_open,
    };
    for input in &definition.inputs {
        nodes.push(node_view(
            input.id,
            &input.name,
            "Control",
            vec![],
            vec![output(Reference::Input { input: input.id }, "Value")],
            false,
        ));
    }
    for node in &definition.nodes {
        let operation = registry::resolve(&node.operation, node.version, &node.mode);
        let mut inputs = vec![];
        if let Ok(operation) = &operation {
            for (name, parameter) in &node.parameters {
                let binding = parameter.binding()?;
                let label = operation
                    .parameters
                    .get(name)
                    .map(|p| p.title.as_str())
                    .unwrap_or(name);
                match &binding {
                    Binding::Literal { .. } | Binding::Link { .. } => {
                        let target = port(
                            &Target::Value {
                                parameter: parameter.id,
                            },
                            label,
                        );
                        if let Binding::Link { reference } = &binding {
                            wires.push(WireView {
                                source: source(reference.target),
                                target: Socket {
                                    node: node.id,
                                    key: target.key.clone(),
                                },
                            });
                        }
                        inputs.push(target);
                    }
                    Binding::Expression { .. } | Binding::List { .. } => {
                        let edges = crate::connections::edges(&binding)?;
                        if edges.is_empty() && matches!(binding, Binding::Expression { .. }) {
                            inputs.push(port(
                                &Target::Value {
                                    parameter: parameter.id,
                                },
                                &format!("{label} · Expression"),
                            ));
                        }
                        for (index, edge) in edges.iter().enumerate() {
                            let target = port(
                                &Target::Edge {
                                    parameter: parameter.id,
                                    edge: edge.id,
                                },
                                &format!("{label} {}", index + 1),
                            );
                            wires.push(WireView {
                                source: source(edge.target),
                                target: Socket {
                                    node: node.id,
                                    key: target.key.clone(),
                                },
                            });
                            inputs.push(target);
                        }
                        if matches!(binding, Binding::List { .. }) {
                            inputs.push(port(
                                &Target::Append {
                                    parameter: parameter.id,
                                },
                                &format!("Add {label}"),
                            ));
                        }
                    }
                    Binding::Track { .. } | Binding::Ramp { .. } => {}
                }
            }
        }
        nodes.push(node_view(
            node.id,
            &node.name,
            operation
                .as_ref()
                .map_or("Unresolved operation", |op| &op.title),
            inputs,
            vec![output(Reference::Node { node: node.id }, "Value")],
            false,
        ));
    }
    for instance in &definition.instances {
        let asset = value.definition(instance.definition)?;
        let mut inputs = vec![];
        for input in &asset.inputs {
            let target = port(&Target::Override { input: input.id }, &input.name);
            if let Some(parameter) = instance.overrides.get(&input.id) {
                match parameter.binding()? {
                    Binding::Link { reference } => wires.push(WireView {
                        source: source(reference.target),
                        target: Socket {
                            node: instance.id,
                            key: target.key.clone(),
                        },
                    }),
                    binding @ (Binding::Expression { .. } | Binding::List { .. }) => {
                        for (index, edge) in crate::connections::edges(&binding)?.iter().enumerate()
                        {
                            let target = port(
                                &Target::OverrideEdge {
                                    input: input.id,
                                    edge: edge.id,
                                },
                                &format!("{} {}", input.name, index + 1),
                            );
                            wires.push(WireView {
                                source: source(edge.target),
                                target: Socket {
                                    node: instance.id,
                                    key: target.key.clone(),
                                },
                            });
                            inputs.push(target);
                        }
                        continue;
                    }
                    _ => {}
                }
            }
            inputs.push(target);
        }
        nodes.push(node_view(
            instance.id,
            &instance.name,
            &asset.name,
            inputs,
            asset
                .outputs
                .iter()
                .map(|o| {
                    output(
                        Reference::Instance {
                            instance: instance.id,
                            output: o.id,
                        },
                        &o.name,
                    )
                })
                .collect(),
            true,
        ));
    }
    for out in &definition.outputs {
        let target = port(&Target::Output, "Value");
        wires.push(WireView {
            source: source(out.value.target),
            target: Socket {
                node: out.id,
                key: target.key.clone(),
            },
        });
        nodes.push(node_view(
            out.id,
            &out.name,
            "Output",
            vec![target],
            vec![],
            false,
        ));
    }
    let sources: std::collections::BTreeSet<_> = nodes
        .iter()
        .flat_map(|node| {
            node.outputs.iter().map(|port| Socket {
                node: node.id,
                key: port.key.clone(),
            })
        })
        .collect();
    let missing = wires.iter().any(|wire| !sources.contains(&wire.source));
    wires.retain(|wire| sources.contains(&wire.source));
    Ok(GraphView {
        key: GraphKey {
            document,
            group: Some(definition.id),
        },
        generation,
        nodes,
        wires,
        selected,
        has_parent: definition.id != value.root.definition,
        error: if missing {
            "Some connections have missing sources. Repair them in Source.".into()
        } else {
            String::new()
        },
    })
}
