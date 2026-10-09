//! Native construction language. Identity metadata is part of complete source.
pub use crate::source_parser::{Fragment, Parsed, parse, parse_fragment};
use crate::{
    binding::Binding,
    expression::Expr,
    expression_source,
    model::{Construction, Definition, Instance, Parameter, Reference},
    registry,
};
use fold_project::Metadata;
use serde::Serialize;
use std::fmt::Write;

fn json(value: &impl Serialize) -> Result<String, String> {
    serde_json::to_string(value).map_err(|e| e.to_string())
}
fn annotation(id: fold_foundation::ObjectId, extensions: &Metadata) -> Result<String, String> {
    let mut metadata = extensions.clone();
    if metadata
        .insert(
            "id".into(),
            serde_json::to_value(id).map_err(|e| e.to_string())?,
        )
        .is_some()
    {
        return Err("Metadata collides with native identity".into());
    }
    Ok(format!("@{}", json(&metadata)?))
}
fn parameter_annotation(parameter: &Parameter) -> Result<String, String> {
    let mut metadata = parameter.extensions.clone();
    if metadata
        .insert("protected".into(), serde_json::json!(parameter.protected))
        .is_some()
    {
        return Err("Parameter extension collides with protection flag".into());
    }
    annotation(parameter.id, &metadata)
}
/// Stable human-readable target names. Instance-output names use a JSON pair to
/// avoid ambiguity when authored names themselves contain separators.
pub(crate) fn reference_name(
    construction: &Construction,
    scope: &Definition,
    target: Reference,
) -> Result<String, String> {
    match target {
        Reference::Input { input } => scope
            .inputs
            .iter()
            .find(|i| i.id == input)
            .ok_or_else(|| "Input outside source scope".to_string())
            .and_then(|i| local_name(&i.name)),
        Reference::Node { node } => scope
            .nodes
            .iter()
            .find(|n| n.id == node)
            .ok_or_else(|| "Node outside source scope".to_string())
            .and_then(|n| local_name(&n.name)),
        Reference::Instance { instance, output } => {
            let instance = scope
                .instances
                .iter()
                .find(|i| i.id == instance)
                .ok_or("Instance outside source scope")?;
            let definition = construction.definition(instance.definition)?;
            let output = definition
                .outputs
                .iter()
                .find(|o| o.id == output)
                .ok_or("Output outside instance definition")?;
            // Prefix distinguishes an instance-output pair from any simple authored name.
            Ok(format!(
                "instance:{}",
                json(&[&instance.name, &output.name])?
            ))
        }
    }
}
fn local_name(name: &str) -> Result<String, String> {
    if name.starts_with("instance:") || name.starts_with("binding:") {
        Ok(format!("binding:{}", json(&name)?))
    } else {
        Ok(name.into())
    }
}
/// Resolve a projected name without confusing authored separators with instance paths.
pub fn resolve_reference_name(
    construction: &Construction,
    scope: &Definition,
    name: &str,
) -> Result<Reference, String> {
    if let Some(pair) = name.strip_prefix("instance:") {
        let [instance_name, output_name]: [String; 2] =
            serde_json::from_str(pair).map_err(|e| e.to_string())?;
        let instance = scope
            .instances
            .iter()
            .find(|i| i.name == instance_name)
            .ok_or("Unknown instance name")?;
        let definition = construction.definition(instance.definition)?;
        let output = definition
            .outputs
            .iter()
            .find(|o| o.name == output_name)
            .ok_or("Unknown instance output")?;
        return Ok(Reference::Instance {
            instance: instance.id,
            output: output.id,
        });
    }
    let decoded: String;
    let name = if let Some(escaped) = name.strip_prefix("binding:") {
        decoded = serde_json::from_str(escaped).map_err(|e| e.to_string())?;
        &decoded
    } else {
        name
    };
    if let Some(input) = scope.inputs.iter().find(|i| i.name == name) {
        return Ok(Reference::Input { input: input.id });
    }
    if let Some(node) = scope.nodes.iter().find(|n| n.name == name) {
        return Ok(Reference::Node { node: node.id });
    }
    Err(format!("Unknown binding {name}"))
}
fn expression(
    construction: &Construction,
    scope: Option<&Definition>,
    value: &Expr,
) -> Result<String, String> {
    expression_source::canonical(value, &mut |target| {
        let scope = scope.ok_or("Root override cannot reference a parent scope")?;
        reference_name(construction, scope, target)
    })
}
fn binding(
    construction: &Construction,
    scope: Option<&Definition>,
    value: &Binding,
    depth: usize,
) -> Result<String, String> {
    if depth > 64 {
        return Err("Source binding depth exceeds 64".into());
    }
    let text = match value {
        Binding::Literal { value } => format!(
            "value({})",
            expression(
                construction,
                scope,
                &Expr::Literal {
                    value: value.clone()
                }
            )?
        ),
        Binding::Link { reference } => format!(
            "link({})",
            expression(
                construction,
                scope,
                &Expr::Reference {
                    reference: reference.clone()
                }
            )?
        ),
        Binding::Expression { expression: value } => {
            format!("expr({})", expression(construction, scope, value)?)
        }
        Binding::Ramp { ramp } => {
            ramp.validate()?;
            format!("ramp({})", json(ramp)?)
        }
        Binding::Track { track } => {
            track.validate()?;
            format!("track({})", json(track)?)
        }
        Binding::List { values } => {
            if values.len() > 4096 {
                return Err("Source list exceeds 4096 entries".into());
            }
            let values = values
                .iter()
                .map(|value| binding(construction, scope, value, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            format!("list[{}]", values.join(", "))
        }
    };
    if text.len() > expression_source::MAX_SOURCE_BYTES {
        return Err("Source binding exceeds 64 KiB".into());
    }
    Ok(text)
}
fn instance(
    out: &mut String,
    construction: &Construction,
    scope: Option<&Definition>,
    instance: &Instance,
    indent: &str,
    keyword: &str,
) -> Result<(), String> {
    let definition = construction.definition(instance.definition)?;
    writeln!(
        out,
        "{indent}{keyword} {} {} = {} clock {} {{",
        json(&instance.name)?,
        annotation(instance.id, &instance.extensions)?,
        json(&definition.name)?,
        json(&instance.clock)?
    )
    .map_err(|e| e.to_string())?;
    for (input, parameter) in &instance.overrides {
        let input = definition
            .inputs
            .iter()
            .find(|i| i.id == *input)
            .ok_or("Override input outside definition")?;
        writeln!(
            out,
            "{indent}  {} {} = {};",
            json(&input.name)?,
            parameter_annotation(parameter)?,
            binding(construction, scope, &parameter.binding()?, 0)?
        )
        .map_err(|e| e.to_string())?;
    }
    writeln!(out, "{indent}}};").map_err(|e| e.to_string())
}
/// Complete deterministic textual projection, including otherwise opaque records.
/// This is not execution qualification; unresolved operations are printed explicitly.
pub fn canonical(construction: &Construction) -> Result<String, String> {
    construction.validate_structure()?;
    let mut out = format!(
        "motion 1;\nseed {};\ncanvas {};\n",
        construction.seed,
        json(&construction.canvas)?
    );
    for definition in &construction.definitions {
        asset(&mut out, construction, definition)?;
    }
    instance(&mut out, construction, None, &construction.root, "", "root")?;
    writeln!(
        out,
        "presentation {};\nextensions {};",
        json(&construction.presentation)?,
        json(&construction.extensions)?
    )
    .map_err(|e| e.to_string())?;
    if out.len() > expression_source::MAX_SOURCE_BYTES {
        return Err("Construction source exceeds 64 KiB".into());
    }
    Ok(out)
}

fn asset(
    out: &mut String,
    construction: &Construction,
    definition: &Definition,
) -> Result<(), String> {
    writeln!(
        out,
        "asset {} {} {{",
        json(&definition.name)?,
        annotation(definition.id, &definition.extensions)?
    )
    .map_err(|e| e.to_string())?;
    for input in &definition.inputs {
        let mut metadata = input.extensions.clone();
        metadata.insert("protected".into(), serde_json::json!(input.protected));
        writeln!(
            out,
            "  input {} {} : {} = {};",
            json(&input.name)?,
            annotation(input.id, &metadata)?,
            json(&input.ty)?,
            expression(
                construction,
                Some(definition),
                &Expr::Literal {
                    value: input.default.clone()
                }
            )?
        )
        .map_err(|e| e.to_string())?;
    }
    for node in &definition.nodes {
        if registry::resolve(&node.operation, node.version, &node.mode).is_err() {
            writeln!(out, "  unresolved {};", json(node)?).map_err(|e| e.to_string())?;
            continue;
        }
        writeln!(
            out,
            "  let {} {} = {}.{} @{} (",
            json(&node.name)?,
            annotation(node.id, &node.extensions)?,
            json(&node.operation)?,
            json(&node.mode)?,
            node.version
        )
        .map_err(|e| e.to_string())?;
        for (name, parameter) in &node.parameters {
            writeln!(
                out,
                "    {} {} = {};",
                json(name)?,
                parameter_annotation(parameter)?,
                binding(construction, Some(definition), &parameter.binding()?, 0)?
            )
            .map_err(|e| e.to_string())?;
        }
        writeln!(out, "  );").map_err(|e| e.to_string())?;
    }
    for asset_instance in &definition.instances {
        instance(
            out,
            construction,
            Some(definition),
            asset_instance,
            "  ",
            "instance",
        )?;
    }
    for output in &definition.outputs {
        writeln!(
            out,
            "  output {} {} : {} = {};",
            json(&output.name)?,
            annotation(output.id, &output.extensions)?,
            json(&output.ty)?,
            expression(
                construction,
                Some(definition),
                &Expr::Reference {
                    reference: output.value.clone()
                }
            )?
        )
        .map_err(|e| e.to_string())?;
    }
    writeln!(out, "}}").map_err(|e| e.to_string())?;
    if out.len() > expression_source::MAX_SOURCE_BYTES {
        return Err("Construction source exceeds 64 KiB".into());
    }
    Ok(())
}
/// Project one complete asset declaration without serializing unrelated assets.
pub fn canonical_fragment(
    construction: &Construction,
    target: fold_foundation::ObjectId,
) -> Result<String, String> {
    construction.validate_structure()?;
    let mut out = String::new();
    asset(&mut out, construction, construction.definition(target)?)?;
    Ok(out)
}
