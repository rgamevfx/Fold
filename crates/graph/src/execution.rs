//! CPU execution of the shared expression IR. No imports, mutable host state or hidden time.
use crate::{
    expression::{Binary, ClockScope, Element, Expr, Function, Unary, binary_type, function_type},
    model::Reference,
    types::{Literal, Unit},
};
use fold_foundation::Time;
use std::cmp::Ordering;

/// Element identity stays an integer even when random output is a scalar.
#[derive(Clone)]
pub struct ElementContext {
    pub id: u64,
    pub index: i64,
    pub count: i64,
    pub position: Option<Literal>,
}
pub struct Context {
    pub scene_time: Time,
    pub asset_time: Time,
    pub seed: u64,
    /// Canonical UUID bytes of the owning operation (network byte order).
    pub operation: [u8; 16],
    pub element: Option<ElementContext>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stop {
    Exhausted,
    Cancelled,
}
/// Shared by all expression evaluations in one graph request.
pub struct Budget<'a> {
    remaining: usize,
    stop: Option<Stop>,
    cancellation: &'a dyn Fn() -> Result<(), String>,
}
impl<'a> Budget<'a> {
    pub fn new(steps: usize, cancellation: &'a dyn Fn() -> Result<(), String>) -> Self {
        Self {
            remaining: steps,
            stop: None,
            cancellation,
        }
    }
    pub fn stop(&self) -> Option<Stop> {
        self.stop
    }
    pub fn step(&mut self) -> Result<(), String> {
        (self.cancellation)().inspect_err(|_| self.stop = Some(Stop::Cancelled))?;
        self.remaining = self.remaining.checked_sub(1).ok_or_else(|| {
            self.stop = Some(Stop::Exhausted);
            "Evaluation budget exhausted".to_string()
        })?;
        Ok(())
    }
}
impl Expr {
    pub fn evaluate(
        &self,
        context: &Context,
        budget: &mut Budget<'_>,
        resolve: &mut impl FnMut(Reference) -> Result<Literal, String>,
    ) -> Result<Literal, String> {
        self.references()?;
        self.execute(context, budget, resolve)
    }
    fn execute(
        &self,
        context: &Context,
        budget: &mut Budget<'_>,
        resolve: &mut impl FnMut(Reference) -> Result<Literal, String>,
    ) -> Result<Literal, String> {
        budget.step()?;
        let result = match self {
            Self::Literal { value } => value.clone(),
            Self::Reference { reference } => resolve(reference.target)?,
            Self::Time { scope } => Literal::Duration {
                value: match scope {
                    ClockScope::Scene => context.scene_time,
                    ClockScope::Asset => context.asset_time,
                },
            },
            Self::Element { property } => {
                let element = context
                    .element
                    .as_ref()
                    .ok_or("Expression requires an element domain")?;
                if element.count < 0 || element.index < 0 || element.index >= element.count {
                    return Err("Invalid element index/count".into());
                }
                match property {
                    Element::Index => Literal::Integer {
                        value: element.index,
                    },
                    Element::Count => Literal::Integer {
                        value: element.count,
                    },
                    Element::Position { unit, space } => {
                        let value = element.position.as_ref().ok_or("Element has no position")?;
                        if value.ty()
                            != (crate::types::Type::Vector {
                                dimensions: 3,
                                unit: *unit,
                                space: *space,
                            })
                        {
                            return Err("Element position unit/space mismatch".into());
                        }
                        value.clone()
                    }
                }
            }
            Self::Random { stream, .. } => Literal::Scalar {
                value: random_v1(
                    context.seed,
                    context.operation,
                    context.element.as_ref().map(|e| e.id),
                    stream,
                ),
            },
            Self::Unary { op, value } => {
                let value = value.execute(context, budget, resolve)?;
                match (op, value) {
                    (Unary::Not, Literal::Bool { value }) => Literal::Bool { value: !value },
                    (Unary::Negate, value) => negate(value)?,
                    _ => return Err("Unary expression type mismatch".into()),
                }
            }
            Self::Binary { op, left, right } => {
                let a = left.execute(context, budget, resolve)?;
                let b = right.execute(context, budget, resolve)?;
                binary(*op, a, b)?
            }
            Self::Call {
                function,
                arguments,
            } => {
                let args = arguments
                    .iter()
                    .map(|a| a.execute(context, budget, resolve))
                    .collect::<Result<Vec<_>, _>>()?;
                call(*function, &args)?
            }
            Self::Vector {
                components,
                unit,
                space,
            } => {
                let values = components
                    .iter()
                    .map(|c| match c.execute(context, budget, resolve)? {
                        Literal::Scalar { value } if *unit == Unit::Unitless => Ok(value),
                        Literal::Distance { value, unit: u } if u == *unit => Ok(value),
                        _ => Err("Vector component units do not match".to_string()),
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                Literal::Vector {
                    value: values,
                    unit: *unit,
                    space: *space,
                }
            }
        };
        result.validate()?;
        Ok(result)
    }
}

/// V1: FNV-1a over seed LE, operation UUID bytes, domain tag, element ID LE,
/// and UTF-8 stream; SplitMix64 finalizer; upper 53 bits mapped to [0, 1).
/// Uniform and element ID zero use distinct domain tags. Time/order never enter the hash.
pub fn random_v1(seed: u64, operation: [u8; 16], element: Option<u64>, stream: &str) -> f64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in seed
        .to_le_bytes()
        .into_iter()
        .chain(operation)
        .chain([u8::from(element.is_some())])
        .chain(element.unwrap_or(0).to_le_bytes())
        .chain(stream.bytes())
    {
        hash = (hash ^ u64::from(byte)).wrapping_mul(0x100000001b3);
    }
    hash = (hash ^ (hash >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
    hash = (hash ^ (hash >> 27)).wrapping_mul(0x94d049bb133111eb);
    hash ^= hash >> 31;
    (hash >> 11) as f64 / 9_007_199_254_740_992.0
}
fn negate(value: Literal) -> Result<Literal, String> {
    match value {
        Literal::Integer { value } => Ok(Literal::Integer {
            value: value.checked_neg().ok_or("Integer overflow")?,
        }),
        Literal::Duration { value } => Ok(Literal::Duration {
            value: value.checked_scale(-1, 1).map_err(|e| e.to_string())?,
        }),
        value => map_float(value, |v| -v),
    }
}
fn map_float(value: Literal, f: impl Fn(f64) -> f64) -> Result<Literal, String> {
    Ok(match value {
        Literal::Scalar { value } => Literal::Scalar { value: f(value) },
        Literal::Distance { value, unit } => Literal::Distance {
            value: f(value),
            unit,
        },
        Literal::Vector { value, unit, space } => Literal::Vector {
            value: value.into_iter().map(f).collect(),
            unit,
            space,
        },
        _ => return Err("Expected scalar, distance or vector".into()),
    })
}
fn compare(a: &Literal, b: &Literal) -> Result<Ordering, String> {
    match (a, b) {
        (Literal::Integer { value: a }, Literal::Integer { value: b }) => Ok(a.cmp(b)),
        (Literal::Duration { value: a }, Literal::Duration { value: b }) => Ok(a.cmp(b)),
        (Literal::Scalar { value: a }, Literal::Scalar { value: b })
        | (Literal::Distance { value: a, .. }, Literal::Distance { value: b, .. }) => {
            a.partial_cmp(b).ok_or("Non-finite comparison".into())
        }
        _ => Err("Values cannot be ordered".into()),
    }
}
/// Checked primitive arithmetic, also used by channel composition.
pub fn binary(op: Binary, a: Literal, b: Literal) -> Result<Literal, String> {
    a.validate()?;
    b.validate()?;
    binary_type(op, &a.ty(), &b.ty())?;
    use Binary::*;
    let result = match (op, a, b) {
        (Equal, a, b) => Literal::Bool { value: a == b },
        (Less, a, b) => Literal::Bool {
            value: compare(&a, &b)? == Ordering::Less,
        },
        (And, Literal::Bool { value: a }, Literal::Bool { value: b }) => {
            Literal::Bool { value: a && b }
        }
        (Or, Literal::Bool { value: a }, Literal::Bool { value: b }) => {
            Literal::Bool { value: a || b }
        }
        (op, Literal::Integer { value: a }, Literal::Integer { value: b }) => Literal::Integer {
            value: match op {
                Add => a.checked_add(b),
                Subtract => a.checked_sub(b),
                Multiply => a.checked_mul(b),
                _ => None,
            }
            .ok_or("Integer overflow")?,
        },
        (op @ (Add | Subtract), Literal::Duration { value: a }, Literal::Duration { value: b }) => {
            Literal::Duration {
                value: if op == Add {
                    a.checked_add(b)
                } else {
                    a.checked_sub(b)
                }
                .map_err(|e| e.to_string())?,
            }
        }
        (Multiply, Literal::Duration { value: a }, Literal::Integer { value: b })
        | (Multiply, Literal::Integer { value: b }, Literal::Duration { value: a }) => {
            Literal::Duration {
                value: a.checked_scale(b, 1).map_err(|e| e.to_string())?,
            }
        }
        (Divide, Literal::Duration { value: a }, Literal::Integer { value: b }) => {
            if b == 0 {
                return Err("Division by zero".into());
            }
            Literal::Duration {
                value: a.checked_divide(b).map_err(|e| e.to_string())?,
            }
        }
        (Divide, Literal::Duration { value: a }, Literal::Duration { value: b }) => {
            if b.numerator() == 0 {
                return Err("Division by zero".into());
            }
            Literal::Scalar {
                value: (a.numerator() as f64 * f64::from(b.denominator()))
                    / (b.numerator() as f64 * f64::from(a.denominator())),
            }
        }
        (op, Literal::Scalar { value: a }, Literal::Scalar { value: b }) => Literal::Scalar {
            value: float_op(op, a, b)?,
        },
        (op, Literal::Distance { value: a, unit }, Literal::Distance { value: b, .. }) => {
            let value = float_op(op, a, b)?;
            if op == Divide {
                Literal::Scalar { value }
            } else {
                Literal::Distance { value, unit }
            }
        }
        (
            op @ (Add | Subtract),
            Literal::Vector {
                value: a,
                unit,
                space,
            },
            Literal::Vector { value: b, .. },
        ) => Literal::Vector {
            value: a
                .into_iter()
                .zip(b)
                .map(|(a, b)| float_op(op, a, b))
                .collect::<Result<_, _>>()?,
            unit,
            space,
        },
        (Multiply, Literal::Scalar { value: factor }, value)
        | (Multiply, value, Literal::Scalar { value: factor }) => map_float(value, |v| v * factor)?,
        (Divide, value, Literal::Scalar { value: divisor }) => {
            if divisor == 0.0 {
                return Err("Division by zero".into());
            }
            map_float(value, |v| v / divisor)?
        }
        _ => return Err("Unsupported arithmetic operands".into()),
    };
    result.validate()?;
    Ok(result)
}
fn float_op(op: Binary, a: f64, b: f64) -> Result<f64, String> {
    Ok(match op {
        Binary::Add => a + b,
        Binary::Subtract => a - b,
        Binary::Multiply => a * b,
        Binary::Divide if b != 0.0 => a / b,
        Binary::Divide => return Err("Division by zero".into()),
        _ => return Err("Not a numeric operator".into()),
    })
}
fn call(function: Function, args: &[Literal]) -> Result<Literal, String> {
    function_type(function, &args.iter().map(Literal::ty).collect::<Vec<_>>())?;
    use Function::*;
    match (function, args) {
        (Sin, [Literal::Scalar { value }]) => Ok(Literal::Scalar { value: value.sin() }),
        (Cos, [Literal::Scalar { value }]) => Ok(Literal::Scalar { value: value.cos() }),
        (Abs, [Literal::Integer { value }]) => Ok(Literal::Integer {
            value: value.checked_abs().ok_or("Integer overflow")?,
        }),
        (Abs, [Literal::Duration { value }]) => Ok(Literal::Duration {
            value: value
                .checked_scale(if value.numerator() < 0 { -1 } else { 1 }, 1)
                .map_err(|e| e.to_string())?,
        }),
        (Abs, [value]) => map_float(value.clone(), f64::abs),
        (Min | Max, [a, b]) => Ok(if (compare(a, b)? == Ordering::Less) == (function == Min) {
            a
        } else {
            b
        }
        .clone()),
        (Clamp, [value, low, high]) => {
            if compare(low, high)? == Ordering::Greater {
                return Err("Clamp bounds are reversed".into());
            }
            Ok(if compare(value, low)? == Ordering::Less {
                low
            } else if compare(value, high)? == Ordering::Greater {
                high
            } else {
                value
            }
            .clone())
        }
        (Lerp, [a, b, Literal::Scalar { value }]) => a.interpolate(b, *value),
        (Length, [Literal::Vector { value, unit, .. }]) => {
            let length = value.iter().fold(0.0f64, |length, v| length.hypot(*v));
            Ok(if *unit == Unit::Unitless {
                Literal::Scalar { value: length }
            } else {
                Literal::Distance {
                    value: length,
                    unit: *unit,
                }
            })
        }
        _ => Err("Invalid function arguments".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn context() -> Context {
        Context {
            scene_time: Time::new(1, 4).unwrap(),
            asset_time: Time::new(-1, 2).unwrap(),
            seed: 7,
            operation: [9; 16],
            element: None,
        }
    }
    fn evaluate(expr: Expr) -> Result<Literal, String> {
        expr.evaluate(&context(), &mut Budget::new(100, &|| Ok(())), &mut |_| {
            Err("Missing reference".into())
        })
    }
    fn lit(value: Literal) -> Box<Expr> {
        Box::new(Expr::Literal { value })
    }
    #[test]
    fn exact_duration_arithmetic_and_failures() {
        assert_eq!(
            binary(
                Binary::Add,
                Literal::Duration {
                    value: Time::new(1, 3).unwrap()
                },
                Literal::Duration {
                    value: Time::new(1, 6).unwrap()
                }
            )
            .unwrap(),
            Literal::Duration {
                value: Time::new(1, 2).unwrap()
            }
        );
        assert!(
            binary(
                Binary::Add,
                Literal::Integer { value: i64::MAX },
                Literal::Integer { value: 1 }
            )
            .is_err()
        );
        assert!(
            binary(
                Binary::Divide,
                Literal::Scalar { value: 1.0 },
                Literal::Scalar { value: 0.0 }
            )
            .is_err()
        );
        assert!(
            binary(
                Binary::Multiply,
                Literal::Scalar { value: f64::MAX },
                Literal::Scalar { value: 2.0 }
            )
            .is_err()
        );
        assert_eq!(
            evaluate(Expr::Time {
                scope: ClockScope::Asset
            })
            .unwrap(),
            Literal::Duration {
                value: Time::new(-1, 2).unwrap()
            }
        );
    }
    #[test]
    fn typed_width_expression_executes_and_checks_reference_values() {
        let expr = Expr::Binary {
            op: Binary::Multiply,
            left: lit(Literal::Distance {
                value: 100.0,
                unit: Unit::Canvas,
            }),
            right: lit(Literal::Scalar { value: 2.0 }),
        };
        assert_eq!(
            evaluate(expr.clone()).unwrap(),
            Literal::Distance {
                value: 200.0,
                unit: Unit::Canvas
            }
        );
        assert!(
            expr.evaluate(
                &context(),
                &mut Budget::new(2, &|| Ok(())),
                &mut |_| unreachable!()
            )
            .is_err()
        );
        assert_eq!(
            expr.evaluate(
                &context(),
                &mut Budget::new(100, &|| Err("cancelled".into())),
                &mut |_| unreachable!()
            )
            .unwrap_err(),
            "cancelled"
        );
        assert!(
            evaluate(Expr::Element {
                property: Element::Index
            })
            .is_err()
        );
    }
    #[test]
    fn random_uses_full_integer_identity_and_named_streams() {
        let id = (1u64 << 60) + 1;
        let a = random_v1(7, [9; 16], Some(id), "jitter");
        assert_eq!(a, random_v1(7, [9; 16], Some(id), "jitter"));
        assert_ne!(a, random_v1(7, [9; 16], Some(id + 1), "jitter"));
        assert_ne!(a, random_v1(7, [9; 16], Some(id), "color"));
        assert_ne!(
            random_v1(7, [9; 16], None, "jitter"),
            random_v1(7, [9; 16], Some(0), "jitter")
        );
        assert!((0.0..1.0).contains(&a));
    }
    #[test]
    fn common_ir_evaluates_element_fields_with_explicit_space_and_stable_random() {
        let mut context = context();
        context.element = Some(ElementContext {
            id: (1u64 << 60) + 1,
            index: 2,
            count: 10,
            position: Some(Literal::Vector {
                value: vec![3., 4., 0.],
                unit: Unit::Meters,
                space: crate::types::Space::World,
            }),
        });
        let length = Expr::Call {
            function: Function::Length,
            arguments: vec![Expr::Element {
                property: Element::Position {
                    unit: Unit::Meters,
                    space: crate::types::Space::World,
                },
            }],
        };
        assert_eq!(
            length
                .evaluate(
                    &context,
                    &mut Budget::new(20, &|| Ok(())),
                    &mut |_| unreachable!()
                )
                .unwrap(),
            Literal::Distance {
                value: 5.,
                unit: Unit::Meters
            }
        );
        let random = Expr::Random {
            version: 1,
            stream: "rotation".into(),
        };
        let first = random
            .evaluate(
                &context,
                &mut Budget::new(20, &|| Ok(())),
                &mut |_| unreachable!(),
            )
            .unwrap();
        context.scene_time = Time::new(100, 1).unwrap();
        context.asset_time = Time::new(-10, 1).unwrap();
        context.element.as_mut().unwrap().index = 7;
        assert_eq!(
            random
                .evaluate(
                    &context,
                    &mut Budget::new(20, &|| Ok(())),
                    &mut |_| unreachable!()
                )
                .unwrap(),
            first
        );
        context.element.as_mut().unwrap().position = Some(Literal::Vector {
            value: vec![3., 4., 0.],
            unit: Unit::Canvas,
            space: crate::types::Space::Screen,
        });
        assert!(
            length
                .evaluate(
                    &context,
                    &mut Budget::new(20, &|| Ok(())),
                    &mut |_| unreachable!()
                )
                .unwrap_err()
                .contains("unit/space")
        );
    }
}
