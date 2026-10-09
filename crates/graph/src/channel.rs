//! Parameter paths and component selection shared by feature-owned parameters.
//! Paths are authoring projections; resolved references retain native identities.
use crate::types::{Literal, Type, Unit};

pub fn escape(segment: &str) -> String {
    segment
        .replace('~', "~0")
        .replace('/', "~1")
        .replace('.', "~2")
}
pub fn unescape(segment: &str) -> Result<String, String> {
    let mut result = String::new();
    let mut chars = segment.chars();
    while let Some(c) = chars.next() {
        if c == '~' {
            result.push(match chars.next() {
                Some('0') => '~',
                Some('1') => '/',
                Some('2') => '.',
                _ => return Err("Invalid parameter path escape".into()),
            });
        } else {
            result.push(c);
        }
    }
    Ok(result)
}
/// Normalize against the owning node/container, never the parameter itself.
pub fn resolve_path(owner: &[String], path: &str) -> Result<Vec<String>, String> {
    if path.len() > 4096 || path.is_empty() {
        return Err("Invalid parameter path length".into());
    }
    let mut result = if path.starts_with('/') {
        vec![]
    } else {
        owner.to_vec()
    };
    for part in path.split('/') {
        match part {
            "" | "." => {}
            ".." => {
                result.pop().ok_or("Parameter path escapes project root")?;
            }
            _ => result.push(unescape(part)?),
        }
        if result.len() > 256 {
            return Err("Parameter path depth exceeds 256".into());
        }
    }
    Ok(result)
}
pub fn relative_path(owner: &[String], target: &[String]) -> String {
    let common = owner.iter().zip(target).take_while(|(a, b)| a == b).count();
    let mut parts = vec!["..".to_string(); owner.len() - common];
    parts.extend(target[common..].iter().map(|v| escape(v)));
    if parts.is_empty() {
        ".".into()
    } else {
        parts.join("/")
    }
}
pub fn component_type(ty: &Type, component: Option<u8>) -> Result<Type, String> {
    let Some(index) = component else {
        return Ok(ty.clone());
    };
    match ty.sampled() {
        Type::Vector {
            dimensions, unit, ..
        } if index < *dimensions => Ok(if *unit == Unit::Unitless {
            Type::Scalar
        } else {
            Type::Distance { unit: *unit }
        }),
        Type::Color { .. } | Type::Rotation { .. } if index < 4 => Ok(Type::Scalar),
        _ => Err("Parameter component does not exist".into()),
    }
}
pub fn component_value(value: Literal, component: Option<u8>) -> Result<Literal, String> {
    let Some(index) = component else {
        return Ok(value);
    };
    let ty = component_type(&value.ty(), component)?;
    let number = match &value {
        Literal::Vector { value, .. } => value[usize::from(index)],
        Literal::Color { value, .. } | Literal::Rotation { value, .. } => value[usize::from(index)],
        _ => return Err("Parameter component does not exist".into()),
    };
    Ok(match ty {
        Type::Distance { unit } => Literal::Distance {
            value: number,
            unit,
        },
        _ => Literal::Scalar { value: number },
    })
}

/// Resolve a parameter in a graph definition. Provider adapters supply the outer
/// document/occurrence path when using the same path rules across documents.
pub fn resolve_definition(
    definition: &crate::model::Definition,
    owner: Option<&str>,
    path: &str,
) -> Result<crate::model::Reference, String> {
    let path = path.strip_prefix("channel:").unwrap_or(path);
    let mut base = vec![definition.name.clone()];
    if let Some(owner) = owner {
        base.push(owner.into());
    }
    let parts = resolve_path(&base, path)?;
    if parts.first() != Some(&definition.name) {
        return Err("Parameter path is outside this graph definition".into());
    }
    if (2..=3).contains(&parts.len())
        && let Some(input) = definition.inputs.iter().find(|p| p.name == parts[1])
    {
        if parts.len() == 2 {
            return Ok(crate::model::Reference::Input { input: input.id });
        }
        let component = Some(component_index(&parts[2])?);
        component_type(&input.ty, component)?;
        return Ok(crate::model::Reference::Parameter {
            parameter: input.id,
            component,
        });
    }
    if !(3..=4).contains(&parts.len()) {
        return Err("Expected graph/node/parameter[/component]".into());
    }
    let node = definition
        .nodes
        .iter()
        .find(|n| n.name == parts[1])
        .ok_or("Missing parameter owner")?;
    let parameter = node.parameters.get(&parts[2]).ok_or("Missing parameter")?;
    let component = parts.get(3).map(|name| component_index(name)).transpose()?;
    Ok(crate::model::Reference::Parameter {
        parameter: parameter.id,
        component,
    })
}
pub fn definition_path(
    definition: &crate::model::Definition,
    reference: crate::model::Reference,
) -> Result<String, String> {
    let crate::model::Reference::Parameter {
        parameter,
        component,
    } = reference
    else {
        return Err("Expected parameter reference".into());
    };
    if let Some(input) = definition.inputs.iter().find(|p| p.id == parameter) {
        let mut path = format!("/{}/{}", escape(&definition.name), escape(&input.name));
        if let Some(component) = component {
            let name = ["x", "y", "z", "w"]
                .get(usize::from(component))
                .ok_or("Invalid component")?;
            path.push('/');
            path.push_str(name);
        }
        return Ok(path);
    }
    let (node, name) = definition
        .nodes
        .iter()
        .find_map(|n| {
            n.parameters
                .iter()
                .find(|(_, p)| p.id == parameter)
                .map(|(name, _)| (n, name))
        })
        .ok_or("Missing parameter target")?;
    let mut path = format!(
        "/{}/{}/{}",
        escape(&definition.name),
        escape(&node.name),
        escape(name)
    );
    if let Some(component) = component {
        let name = ["x", "y", "z", "w"]
            .get(usize::from(component))
            .ok_or("Invalid component")?;
        path.push('/');
        path.push_str(name);
    }
    Ok(path)
}

fn component_index(name: &str) -> Result<u8, String> {
    match name {
        "x" | "r" => Ok(0),
        "y" | "g" => Ok(1),
        "z" | "b" => Ok(2),
        "w" | "a" => Ok(3),
        _ => Err("Unknown parameter component".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn paths_round_trip_reserved_names_and_reject_root_escape() {
        let owner = vec!["main".into(), "scene".into(), "copy".into()];
        let target = vec![
            "main".into(),
            "scene".into(),
            "a/b.~".into(),
            "width".into(),
        ];
        assert_eq!(
            resolve_path(&owner, &relative_path(&owner, &target)).unwrap(),
            target
        );
        assert!(resolve_path(&owner, "../../../../bad").is_err());
        assert!(resolve_path(&owner, "~9").is_err());
        assert_eq!(
            resolve_path(&owner, "/main/scene").unwrap(),
            vec!["main", "scene"]
        );
    }
}
