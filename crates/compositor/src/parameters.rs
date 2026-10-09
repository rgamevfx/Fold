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
    if let Some(path) = node.tracks().parameter_path(parameter) {
        let value = node
            .tracks()
            .sample(path, time)?
            .ok_or("Missing animation parameter")?;
        return fold_platform::parameters::select_component(Literal::Scalar { value }, target);
    }
    let name = node.parameter_name(parameter)?;
    let values = crate::operation::parameter_values(&node.evaluated_parameters(time)?);
    let value = values.get(name).ok_or("Missing evaluated parameter")?;
    let Binding::Literal { value } =
        serde_json::from_value(value.clone()).map_err(|_| "Parameter is not a primitive value")?
    else {
        return Err("Parameter is not a primitive value".into());
    };
    cancel.check()?;
    fold_platform::parameters::select_component(value, target)
}
