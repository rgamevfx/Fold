//! Bounded expression IR shared by time signals and element fields.
use crate::{
    model::{Edge, Reference},
    types::{Literal, Space, Type, Unit},
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ClockScope {
    Scene,
    Asset,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unary {
    Negate,
    Not,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Equal,
    Less,
    And,
    Or,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Function {
    Sin,
    Cos,
    Abs,
    Min,
    Max,
    Clamp,
    Lerp,
    Length,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Element {
    Index,
    Count,
    Position { unit: Unit, space: Space },
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expr {
    Literal {
        value: Literal,
    },
    Reference {
        reference: Edge,
    },
    Time {
        scope: ClockScope,
    },
    Element {
        property: Element,
    },
    Random {
        version: u32,
        stream: String,
    },
    Unary {
        op: Unary,
        value: Box<Expr>,
    },
    Binary {
        op: Binary,
        left: Box<Expr>,
        right: Box<Expr>,
    },
    Call {
        function: Function,
        arguments: Vec<Expr>,
    },
    Vector {
        components: Vec<Expr>,
        unit: Unit,
        space: Space,
    },
}
impl Expr {
    pub fn references(&self) -> Result<Vec<Reference>, String> {
        Ok(self.edges()?.into_iter().map(|edge| edge.target).collect())
    }
    pub fn edges(&self) -> Result<Vec<&Edge>, String> {
        let mut references = Vec::new();
        let mut stack = vec![(self, 0)];
        let mut count = 0;
        while let Some((expression, depth)) = stack.pop() {
            count += 1;
            if count > 4096 || depth > 64 {
                return Err("Expression exceeds 4096 terms or depth 64".into());
            }
            match expression {
                Self::Literal { value } => value.validate()?,
                Self::Reference { reference } => {
                    reference.validate()?;
                    references.push(reference);
                }
                Self::Random { version, stream }
                    if *version != 1 || stream.is_empty() || stream.len() > 256 =>
                {
                    return Err("Unsupported random version or invalid stream".into());
                }
                Self::Unary { value, .. } => stack.push((value, depth + 1)),
                Self::Binary { left, right, .. } => {
                    stack.push((left, depth + 1));
                    stack.push((right, depth + 1));
                }
                Self::Call { arguments, .. } => {
                    if arguments.len() > 3 {
                        return Err("Expression function accepts at most three arguments".into());
                    }
                    stack.extend(arguments.iter().map(|v| (v, depth + 1)));
                }
                Self::Vector { components, .. } => {
                    if !(2..=3).contains(&components.len()) {
                        return Err("Vector expression needs two or three components".into());
                    }
                    stack.extend(components.iter().map(|v| (v, depth + 1)));
                }
                _ => {}
            }
        }
        Ok(references)
    }
    pub fn infer(
        &self,
        resolve: &mut impl FnMut(Reference) -> Result<Type, String>,
    ) -> Result<Type, String> {
        self.references()?;
        self.infer_inner(resolve)
    }
    fn infer_inner(
        &self,
        resolve: &mut impl FnMut(Reference) -> Result<Type, String>,
    ) -> Result<Type, String> {
        let ty = match self {
            Self::Literal { value } => value.ty(),
            Self::Reference { reference } => resolve(reference.target)?.sampled().clone(),
            Self::Time { .. } => Type::Duration,
            Self::Element { property } => match property {
                Element::Index | Element::Count => Type::Integer,
                Element::Position { unit, space } => Type::Vector {
                    dimensions: 3,
                    unit: *unit,
                    space: *space,
                },
            },
            Self::Random { .. } => Type::Scalar,
            Self::Unary { op, value } => {
                let ty = value.infer_inner(resolve)?;
                match op {
                    Unary::Not if ty == Type::Bool => Type::Bool,
                    Unary::Negate if numeric(&ty) => ty,
                    _ => return Err("Unary expression type mismatch".into()),
                }
            }
            Self::Binary { op, left, right } => binary_type(
                *op,
                &left.infer_inner(resolve)?,
                &right.infer_inner(resolve)?,
            )?,
            Self::Call {
                function,
                arguments,
            } => {
                let types = arguments
                    .iter()
                    .map(|a| a.infer_inner(resolve))
                    .collect::<Result<Vec<_>, _>>()?;
                function_type(*function, &types)?
            }
            Self::Vector {
                components,
                unit,
                space,
            } => {
                let component_type = if *unit == Unit::Unitless {
                    Type::Scalar
                } else {
                    Type::Distance { unit: *unit }
                };
                for component in components {
                    if component.infer_inner(resolve)? != component_type {
                        return Err("Vector component units do not match".into());
                    }
                }
                Type::Vector {
                    dimensions: components.len() as u8,
                    unit: *unit,
                    space: *space,
                }
            }
        };
        if !ty.is_primitive() {
            return Err("Expressions require primitive values".into());
        }
        ty.validate()?;
        Ok(ty)
    }
}
fn numeric(ty: &Type) -> bool {
    matches!(
        ty,
        Type::Integer | Type::Scalar | Type::Distance { .. } | Type::Duration | Type::Vector { .. }
    )
}
pub fn binary_type(op: Binary, a: &Type, b: &Type) -> Result<Type, String> {
    match op {
        Binary::Add | Binary::Subtract if a == b && numeric(a) => Ok(a.clone()),
        Binary::Multiply
            if a == &Type::Integer && b == &Type::Duration
                || b == &Type::Integer && a == &Type::Duration =>
        {
            Ok(Type::Duration)
        }
        Binary::Multiply if a == &Type::Integer && b == a => Ok(Type::Integer),
        Binary::Multiply
            if a == &Type::Scalar
                && matches!(
                    b,
                    Type::Scalar | Type::Distance { .. } | Type::Vector { .. }
                ) =>
        {
            Ok(b.clone())
        }
        Binary::Multiply
            if b == &Type::Scalar && matches!(a, Type::Distance { .. } | Type::Vector { .. }) =>
        {
            Ok(a.clone())
        }
        Binary::Divide
            if a == b && matches!(a, Type::Scalar | Type::Distance { .. } | Type::Duration) =>
        {
            Ok(Type::Scalar)
        }
        Binary::Divide if a == &Type::Duration && b == &Type::Integer => Ok(Type::Duration),
        Binary::Divide
            if b == &Type::Scalar && matches!(a, Type::Distance { .. } | Type::Vector { .. }) =>
        {
            Ok(a.clone())
        }
        Binary::Equal if a == b && a.is_primitive() => Ok(Type::Bool),
        Binary::Less
            if a == b
                && matches!(
                    a,
                    Type::Scalar | Type::Integer | Type::Distance { .. } | Type::Duration
                ) =>
        {
            Ok(Type::Bool)
        }
        Binary::And | Binary::Or if a == &Type::Bool && b == a => Ok(Type::Bool),
        _ => Err(format!(
            "Expression operator {op:?} does not accept {a:?} and {b:?}"
        )),
    }
}
pub fn function_type(function: Function, args: &[Type]) -> Result<Type, String> {
    match (function, args) {
        (Function::Sin | Function::Cos, [Type::Scalar]) => Ok(Type::Scalar),
        (Function::Abs, [a]) if numeric(a) => Ok(a.clone()),
        (Function::Min | Function::Max, [a, b])
            if a == b
                && matches!(
                    a,
                    Type::Scalar | Type::Integer | Type::Distance { .. } | Type::Duration
                ) =>
        {
            Ok(a.clone())
        }
        (Function::Clamp, [a, b, c])
            if a == b
                && a == c
                && matches!(
                    a,
                    Type::Scalar | Type::Integer | Type::Distance { .. } | Type::Duration
                ) =>
        {
            Ok(a.clone())
        }
        (Function::Lerp, [a, b, Type::Scalar]) if a == b && a.is_interpolatable() => Ok(a.clone()),
        (
            Function::Length,
            [
                Type::Vector {
                    unit: Unit::Unitless,
                    ..
                },
            ],
        ) => Ok(Type::Scalar),
        (Function::Length, [Type::Vector { unit, .. }]) => Ok(Type::Distance { unit: *unit }),
        _ => Err(format!(
            "Expression function {function:?} does not accept {args:?}"
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn literal(value: Literal) -> Expr {
        Expr::Literal { value }
    }
    #[test]
    fn inferred_arithmetic_preserves_units_and_rejects_implicit_space_conversion() {
        let width = Expr::Binary {
            op: Binary::Multiply,
            left: Box::new(literal(Literal::Distance {
                value: 100.,
                unit: Unit::Canvas,
            })),
            right: Box::new(literal(Literal::Scalar { value: 2. })),
        };
        assert_eq!(
            width
                .infer(&mut |_| Err("unexpected reference".into()))
                .unwrap(),
            Type::Distance { unit: Unit::Canvas }
        );
        let invalid = Expr::Binary {
            op: Binary::Add,
            left: Box::new(width),
            right: Box::new(literal(Literal::Distance {
                value: 3.,
                unit: Unit::Meters,
            })),
        };
        assert!(invalid.infer(&mut |_| unreachable!()).is_err());
        let wave = Expr::Call {
            function: Function::Sin,
            arguments: vec![Expr::Time {
                scope: ClockScope::Asset,
            }],
        };
        assert!(
            wave.infer(&mut |_| unreachable!()).is_err(),
            "duration requires an explicit dimensionless ratio before sin"
        );
    }
    #[test]
    fn expressions_have_bounded_structure_and_versioned_random() {
        let mut expression = literal(Literal::Scalar { value: 1. });
        for _ in 0..66 {
            expression = Expr::Unary {
                op: Unary::Negate,
                value: Box::new(expression),
            };
        }
        assert!(expression.references().is_err());
        assert!(
            Expr::Random {
                version: 2,
                stream: "rotation".into()
            }
            .references()
            .is_err()
        );
    }
}
