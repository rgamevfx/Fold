//! Typed connection traversal; opaque metadata is never interpreted as a wire.
use crate::{
    binding::Binding,
    expression::Expr,
    model::{Edge, Reference},
};
use fold_foundation::ObjectId;

pub fn edges(binding: &Binding) -> Result<Vec<&Edge>, String> {
    let mut result = vec![];
    let mut pending = vec![(binding, 0)];
    let mut count = 0;
    while let Some((binding, depth)) = pending.pop() {
        count += 1;
        if depth > 64 || count > 4096 {
            return Err("Binding traversal budget exceeded".into());
        }
        match binding {
            Binding::Link { reference } => {
                reference.validate()?;
                result.push(reference);
            }
            Binding::Expression { expression } => result.extend(expression.edges()?),
            Binding::List { values } => pending.extend(values.iter().rev().map(|v| (v, depth + 1))),
            _ => {}
        }
    }
    Ok(result)
}
pub fn retarget(binding: &mut Binding, id: ObjectId, target: Reference) -> Result<(), String> {
    if edges(binding)?.iter().filter(|edge| edge.id == id).count() != 1 {
        return Err("Connection identity is missing or ambiguous".into());
    }
    let mut pending = vec![binding];
    while let Some(binding) = pending.pop() {
        match binding {
            Binding::Link { reference } if reference.id == id => {
                reference.target = target;
                return Ok(());
            }
            Binding::Expression { expression } => {
                let mut expressions = vec![expression];
                while let Some(expression) = expressions.pop() {
                    match expression {
                        Expr::Reference { reference } if reference.id == id => {
                            reference.target = target;
                            return Ok(());
                        }
                        Expr::Unary { value, .. } => expressions.push(value),
                        Expr::Binary { left, right, .. } => {
                            expressions.push(left);
                            expressions.push(right);
                        }
                        Expr::Call { arguments, .. } => expressions.extend(arguments),
                        Expr::Vector { components, .. } => expressions.extend(components),
                        _ => {}
                    }
                }
            }
            Binding::List { values } => pending.extend(values),
            _ => {}
        }
    }
    Err("Connection is unavailable".into())
}
