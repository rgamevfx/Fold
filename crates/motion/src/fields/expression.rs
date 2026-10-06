//! Persistable typed expression IR. Compilation retains native field sampling and
//! explicit named dependencies; expressions never execute arbitrary source code.
use super::{Datum, Field, Kind};
use fold_foundation::Time;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
#[derive(Clone, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum Expression {
    Literal {
        value: Datum,
    },
    Input {
        name: String,
    },
    Time,
    Index,
    Count,
    Position,
    Unary {
        operation: Unary,
        value: Box<Expression>,
    },
    Binary {
        operation: Binary,
        a: Box<Expression>,
        b: Box<Expression>,
    },
    Vector {
        x: Box<Expression>,
        y: Box<Expression>,
    },
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unary {
    Sin,
    Cos,
    Floor,
    Abs,
    Negate,
}
#[derive(Clone, Copy, Debug, Serialize, Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Binary {
    Add,
    Subtract,
    Multiply,
    Divide,
    Modulo,
    Min,
    Max,
}
impl Expression {
    pub fn compile(&self, inputs: &BTreeMap<String, Field>, time: Time) -> Result<Field, String> {
        self.compile_inner(inputs, time, 0, &mut 0)
    }
    fn compile_inner(
        &self,
        inputs: &BTreeMap<String, Field>,
        time: Time,
        depth: usize,
        count: &mut usize,
    ) -> Result<Field, String> {
        *count += 1;
        if depth > 32 || *count > 512 {
            return Err("expression exceeds depth 32 or 512 terms".into());
        }
        let mut child = |e: &Expression| e.compile_inner(inputs, time, depth + 1, count);
        Ok(match self {
            Self::Literal { value } => {
                value.validate()?;
                Field::constant(value.clone())
            }
            Self::Input { name } => inputs
                .get(name)
                .ok_or_else(|| format!("unknown expression input {name}"))?
                .clone(),
            Self::Time => Field::new(Kind::Scalar, false, move |s, _| {
                let t = time
                    .checked_sub(s.time_offset.unwrap_or(Time::ZERO))
                    .map_err(|e| e.to_string())?;
                Ok(Datum::Scalar(t.numerator() as f64 / t.denominator() as f64))
            }),
            Self::Index => Field::new(Kind::Scalar, true, |s, _| Ok(Datum::Scalar(s.index as f64))),
            Self::Count => Field::new(Kind::Scalar, true, |s, _| Ok(Datum::Scalar(s.count as f64))),
            Self::Position => Field::new(Kind::Vector, true, |s, _| Ok(Datum::Vector(s.position))),
            Self::Unary { operation, value } => {
                let value = child(value)?;
                let kind = value.kind;
                if !matches!(kind, Kind::Scalar | Kind::Vector | Kind::Color) {
                    return Err("unary expression requires numeric value".into());
                }
                let operation = *operation;
                value.map(kind, move |v| {
                    v.numeric(&v, |x, _| match operation {
                        Unary::Sin => x.sin(),
                        Unary::Cos => x.cos(),
                        Unary::Floor => x.floor(),
                        Unary::Abs => x.abs(),
                        Unary::Negate => -x,
                    })
                })
            }
            Self::Binary { operation, a, b } => {
                let a = child(a)?;
                let b = child(b)?;
                let kind = a.kind;
                if kind != b.kind || !matches!(kind, Kind::Scalar | Kind::Vector | Kind::Color) {
                    return Err("binary expression operands require identical numeric types; no implicit scalar/vector conversion".into());
                }
                let operation = *operation;
                a.zip(b, kind, move |a, b| {
                    a.numeric(&b, |a, b| match operation {
                        Binary::Add => a + b,
                        Binary::Subtract => a - b,
                        Binary::Multiply => a * b,
                        Binary::Divide => a / b,
                        Binary::Modulo => a.rem_euclid(b),
                        Binary::Min => a.min(b),
                        Binary::Max => a.max(b),
                    })
                })
            }
            Self::Vector { x, y } => {
                let x = child(x)?;
                let y = child(y)?;
                if x.kind != Kind::Scalar || y.kind != Kind::Scalar {
                    return Err("vector expression requires scalar components".into());
                }
                x.zip(y, Kind::Vector, |x, y| {
                    Ok(Datum::Vector([x.scalar()?, y.scalar()?]))
                })
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fields::{Budget, Sample};
    #[test]
    fn exact_time_offsets_domains_and_numeric_failures_are_preserved() {
        let time = Time::new(5, 3).unwrap();
        let field = Expression::Time.compile(&BTreeMap::new(), time).unwrap();
        let sample = Sample {
            time_offset: Some(Time::new(2, 3).unwrap()),
            index: 7,
            count: 12,
            ..Default::default()
        };
        assert_eq!(
            field.sample(&sample, &mut Budget::default()).unwrap(),
            Datum::Scalar(1.)
        );
        let index = Expression::Index.compile(&BTreeMap::new(), time).unwrap();
        assert!(index.varying);
        assert_eq!(
            index.sample(&sample, &mut Budget::default()).unwrap(),
            Datum::Scalar(7.)
        );
        let divide = Expression::Binary {
            operation: Binary::Divide,
            a: Box::new(Expression::Literal {
                value: Datum::Scalar(1.),
            }),
            b: Box::new(Expression::Literal {
                value: Datum::Scalar(0.),
            }),
        };
        assert!(
            divide
                .compile(&BTreeMap::new(), time)
                .unwrap()
                .sample(&sample, &mut Budget::default())
                .is_err()
        );
        let mut deep = Expression::Time;
        for _ in 0..34 {
            deep = Expression::Unary {
                operation: Unary::Abs,
                value: Box::new(deep),
            };
        }
        assert!(
            deep.compile(&BTreeMap::new(), time)
                .err()
                .unwrap()
                .contains("depth")
        );
    }
}
