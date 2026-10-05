//! Small scalar/vector field operators; no geometry or editor dependencies.
use super::*;
use crate::{
    evaluation::{Value, field, output},
    fields::Field,
    geometry::derived_id,
};
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
pub enum MathOp {
    #[default]
    Add,
    Subtract,
    Multiply,
    Divide,
    Min,
    Max,
    Absolute,
    Negate,
    Round,
    Floor,
    Ceil,
    Truncate,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct MathSettings {
    pub operation: MathOp,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub enum Comparison {
    #[default]
    Less,
    LessEqual,
    Equal,
    Greater,
    GreaterEqual,
    NotEqual,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct CompareSettings {
    pub operation: Comparison,
}
pub fn definitions() -> Vec<Definition> {
    let mut math = definition(
        "fold.motion.math",
        "Math",
        "Values",
        vec![scalar("a", 0., ""), scalar("b", 0., "")],
        vec![("value", Kind::Scalar)],
        |e, n| {
            let operation = n.settings::<MathSettings>()?.operation;
            let a = e.field(n, "a")?;
            let kind = a.kind;
            if matches!(
                operation,
                MathOp::Absolute
                    | MathOp::Negate
                    | MathOp::Round
                    | MathOp::Floor
                    | MathOp::Ceil
                    | MathOp::Truncate
            ) {
                return Ok(field(a.map(kind, move |a| {
                    a.numeric(&a, |a, _| match operation {
                        MathOp::Absolute => a.abs(),
                        MathOp::Negate => -a,
                        MathOp::Round => a.round(),
                        MathOp::Floor => a.floor(),
                        MathOp::Ceil => a.ceil(),
                        MathOp::Truncate => a.trunc(),
                        _ => unreachable!(),
                    })
                })));
            }
            let b = e.field(n, "b")?;
            Ok(field(a.zip(b, kind, move |a, b| {
                a.numeric(&b, |a, b| match operation {
                    MathOp::Add => a + b,
                    MathOp::Subtract => a - b,
                    MathOp::Multiply => a * b,
                    MathOp::Divide => a / b,
                    MathOp::Min => a.min(b),
                    MathOp::Max => a.max(b),
                    _ => unreachable!(),
                })
            })))
        },
    );
    for socket in &mut math.inputs {
        socket.kind = Kind::Generic;
    }
    math.outputs[0].1 = Kind::Generic;
    math.defaults = || serde_json::to_value(MathSettings::default()).unwrap();
    math.validate = |n| n.settings::<MathSettings>().map(|_| ());
    let mut compare = definition(
        "fold.motion.compare",
        "Compare",
        "Values",
        vec![scalar("a", 0., ""), scalar("b", 0., "")],
        vec![("value", Kind::Bool)],
        |e, n| {
            let op = n.settings::<CompareSettings>()?.operation;
            Ok(field(e.field(n, "a")?.zip(
                e.field(n, "b")?,
                Kind::Bool,
                move |a, b| {
                    let a = a.scalar()?;
                    let b = b.scalar()?;
                    Ok(Datum::Bool(match op {
                        Comparison::Less => a < b,
                        Comparison::LessEqual => a <= b,
                        Comparison::Equal => a == b,
                        Comparison::Greater => a > b,
                        Comparison::GreaterEqual => a >= b,
                        Comparison::NotEqual => a != b,
                    }))
                },
            )))
        },
    );
    compare.defaults = || serde_json::to_value(CompareSettings::default()).unwrap();
    compare.validate = |n| n.settings::<CompareSettings>().map(|_| ());
    let mut definitions = vec![
        definition(
            "fold.motion.value",
            "Value",
            "Values",
            vec![scalar("value", 0., "")],
            vec![("value", Kind::Scalar)],
            |e, n| Ok(field(e.field(n, "value")?)),
        ),
        definition(
            "fold.motion.combine_xy",
            "Combine XY",
            "Values",
            vec![scalar("x", 0., ""), scalar("y", 0., "")],
            vec![("value", Kind::Vector)],
            |e, n| {
                Ok(field(e.field(n, "x")?.zip(
                    e.field(n, "y")?,
                    Kind::Vector,
                    |x, y| Ok(Datum::Vector([x.scalar()?, y.scalar()?])),
                )))
            },
        ),
        definition(
            "fold.motion.separate_xy",
            "Separate XY",
            "Values",
            vec![vector("value", [0., 0.], "")],
            vec![("x", Kind::Scalar), ("y", Kind::Scalar)],
            |e, n| {
                let f = e.field(n, "value")?;
                let mut o = output(
                    "x",
                    Value::Field(
                        f.clone()
                            .map(Kind::Scalar, |v| Ok(Datum::Scalar(v.vector()?[0]))),
                    ),
                );
                o.insert(
                    "y".into(),
                    Value::Field(f.map(Kind::Scalar, |v| Ok(Datum::Scalar(v.vector()?[1])))),
                );
                Ok(o)
            },
        ),
        math,
        compare,
        definition(
            "fold.motion.map_range",
            "Map Range",
            "Values",
            vec![
                scalar("value", 0., ""),
                scalar("source_min", 0., ""),
                scalar("source_max", 1., ""),
                scalar("target_min", 0., ""),
                scalar("target_max", 1., ""),
                boolean("clamp", true),
                boolean("smooth", false),
            ],
            vec![("value", Kind::Scalar)],
            |e, n| {
                let f = e.field(n, "value")?;
                let a = e.scalar(n, "source_min")?;
                let b = e.scalar(n, "source_max")?;
                let c = e.scalar(n, "target_min")?;
                let d = e.scalar(n, "target_max")?;
                let clamp = e.uniform(n, "clamp")?.boolean()?;
                let smooth = e.uniform(n, "smooth")?.boolean()?;
                if a == b {
                    return Err("source range cannot be zero".into());
                }
                Ok(field(f.map(Kind::Scalar, move |v| {
                    let mut t = (v.scalar()? - a) / (b - a);
                    if clamp {
                        t = t.clamp(0., 1.);
                    }
                    if smooth {
                        t = t * t * (3. - 2. * t);
                    }
                    Ok(Datum::Scalar(c + (d - c) * t))
                })))
            },
        ),
        definition(
            "fold.motion.mix",
            "Mix",
            "Values",
            vec![
                scalar("a", 0., ""),
                scalar("b", 1., ""),
                scalar("weight", 0.5, ""),
            ],
            vec![("value", Kind::Scalar)],
            |e, n| {
                let a = e.field(n, "a")?;
                let b = e.field(n, "b")?;
                let w = e.field(n, "weight")?;
                Ok(field(Field::new(
                    a.kind,
                    a.varying || b.varying || w.varying,
                    move |s, budget| {
                        a.sample(s, budget)?
                            .mix(&b.sample(s, budget)?, w.sample(s, budget)?.scalar()?)
                    },
                )))
            },
        ),
        definition(
            "fold.motion.random_value",
            "Random Value",
            "Values",
            vec![
                Socket::required("id", Kind::Id),
                scalar("seed", 0., "integer"),
                scalar("min", 0., ""),
                scalar("max", 1., ""),
            ],
            vec![("value", Kind::Scalar)],
            |e, n| {
                let seed = e.scalar(n, "seed")?;
                if seed.fract() != 0. || seed.abs() > 9007199254740991. {
                    return Err("seed must be an exactly representable integer".into());
                }
                let seed = derived_id(n.seed(), seed as i64 as u64);
                let min = e.scalar(n, "min")?;
                let max = e.scalar(n, "max")?;
                let identity = e.field(n, "id")?;
                Ok(field(Field::new(
                    Kind::Scalar,
                    identity.varying,
                    move |s, b| {
                        let Datum::Id(id) = identity.sample(s, b)? else {
                            return Err("expected stable element identity".into());
                        };
                        Ok(Datum::Scalar(
                            min + (max - min)
                                * ((derived_id(seed, id) >> 11) as f64 / 9007199254740992.),
                        ))
                    },
                )))
            },
        ),
        context("fold.motion.position", "Position", Kind::Vector, |_, _| {
            Ok(field(Field::new(Kind::Vector, true, |s, _| {
                Ok(Datum::Vector(s.position))
            })))
        }),
        context("fold.motion.index", "Index", Kind::Scalar, |_, _| {
            Ok(field(Field::new(Kind::Scalar, true, |s, _| {
                Ok(Datum::Scalar(s.index as f64))
            })))
        }),
        context("fold.motion.element_id", "Element ID", Kind::Id, |_, _| {
            Ok(field(Field::new(Kind::Id, true, |s, _| {
                Ok(Datum::Id(s.id))
            })))
        }),
        context(
            "fold.motion.domain_size",
            "Domain Size",
            Kind::Scalar,
            |_, _| {
                Ok(field(Field::new(Kind::Scalar, true, |s, _| {
                    Ok(Datum::Scalar(s.count as f64))
                })))
            },
        ),
    ];
    for d in &mut definitions {
        if matches!(d.id, "fold.motion.value" | "fold.motion.mix") {
            for s in &mut d.inputs {
                if s.id != "weight" {
                    s.kind = Kind::Generic;
                }
            }
            d.outputs[0].1 = Kind::Generic;
        }
    }
    definitions
}
fn context(
    id: &'static str,
    name: &'static str,
    kind: Kind,
    evaluate: fn(&mut Evaluator<'_>, &Node) -> Result<Outputs, String>,
) -> Definition {
    definition(id, name, "Context", vec![], vec![("value", kind)], evaluate)
}
