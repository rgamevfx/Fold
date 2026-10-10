//! Native Compositor parameters projected through the shared parameter service.
//! Sampling uses the same animated configuration as render compilation.
use crate::Composite;
use fold_foundation::Time;
use fold_graph::{
    binding::Binding,
    types::{Literal, Type},
};
use fold_media::Cancel;
use fold_platform::parameters::{Descriptor, Target};
use fold_project::Document;

pub(crate) fn catalog(document: &Document) -> Result<Vec<Descriptor>, String> {
    let native = Composite::read_document(document)?;
    let graph = native.prepare_document(document)?;
    let mut result = vec![];
    for node in &graph.nodes {
        for (name, parameter) in &node.operation.parameters {
            let ty = match parameter.binding() {
                Ok(Binding::Literal { value }) => Some(value.ty()),
                _ => None, // Domain configurations are not primitive bindings.
            };
            result.push(Descriptor {
                target: Target {
                    document: document.id,
                    placement: None,
                    object: Some(node.id),
                    occurrence: vec![],
                    parameter: parameter.id.into(),
                    component: None,
                },
                path: vec![node.name.clone(), name.clone()],
                value_type: ty,
                editable: !parameter.protected,
            });
        }
        for property in node.animated_properties() {
            for i in 0..property.values.len() {
                let fold_graph::model::Reference::Parameter {
                    parameter,
                    component,
                } = node.property_reference(&property.id, i)?
                else {
                    unreachable!()
                };
                result.push(Descriptor {
                    target: Target {
                        document: document.id,
                        placement: None,
                        object: Some(node.id),
                        occurrence: vec![],
                        parameter: parameter.into(),
                        component,
                    },
                    path: vec![
                        node.name.clone(),
                        property.id.clone(),
                        property.channels[i].to_lowercase(),
                    ],
                    value_type: Some(Type::Scalar),
                    editable: (if property.id == "mix" {
                        !node.effect.protected
                    } else {
                        !node.operation.parameters
                            [node.config().parameter_for_property(&property.id)]
                        .protected
                    }) && !node
                        .animation
                        .bindings()
                        .get(&crate::animation::path(&property.id, i))
                        .is_some_and(|p| p.protected),
                });
            }
        }
        for (name, parameter) in node.animation.bindings() {
            result.push(Descriptor {
                target: Target {
                    document: document.id,
                    placement: None,
                    object: Some(node.id),
                    occurrence: vec![],
                    parameter: parameter.id.into(),
                    component: None,
                },
                path: vec![node.name.clone(), "animation".into(), name.clone()],
                value_type: Some(Type::Scalar),
                editable: !parameter.protected,
            });
        }
    }
    Ok(result)
}

pub(crate) fn sample(
    document: &Document,
    target: &Target,
    time: Time,
    cancel: &Cancel,
) -> Result<Literal, String> {
    cancel.check()?;
    if !target.occurrence.is_empty() {
        return Err("Compositor occurrence sampling is not available".into());
    }
    let native = Composite::read_document(document)?;
    let graph = native.prepare_document(document)?;
    let node = graph.node(target.object.ok_or("Missing parameter owner")?)?;
    let parameter = target.parameter.native().ok_or("Missing parameter")?;
    if target.component.is_some() {
        let reference = fold_graph::model::Reference::Parameter {
            parameter,
            component: target.component,
        };
        if graph.reference_owner(reference)?.0 != node.id {
            return Err("Parameter belongs to another node".into());
        }
        return Ok(Literal::Scalar {
            value: graph.sample_reference(
                fold_graph::model::Reference::Parameter {
                    parameter,
                    component: target.component,
                },
                time,
            )?,
        });
    }
    if let Some(path) = node.channels().parameter_path(parameter) {
        let (property, component) = path.rsplit_once('.').ok_or("Missing component")?;
        let value = graph.sample_reference(
            node.property_reference(
                property,
                component.parse().map_err(|_| "Invalid component")?,
            )?,
            time,
        )?;
        return fold_platform::parameters::select_component(Literal::Scalar { value }, target);
    }
    let name = node.parameter_name(parameter)?;
    let values = crate::operation::parameter_values(&graph.evaluated_parameters(node.id, time)?);
    let value = values.get(name).ok_or("Missing evaluated parameter")?;
    let Binding::Literal { value } =
        serde_json::from_value(value.clone()).map_err(|_| "Parameter is not a primitive value")?
    else {
        return Err("Parameter is not a primitive value".into());
    };
    cancel.check()?;
    fold_platform::parameters::select_component(value, target)
}
