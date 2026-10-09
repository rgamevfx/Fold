use super::{
    Scope, State, Value,
    attributes::{Domain, vector, xyz},
};
use crate::{
    registry::Builtin,
    types::{Literal, Unit},
};
use fold_foundation::ObjectId;
use std::{collections::BTreeMap, sync::Arc};
pub(super) fn text<'a>(args: &'a BTreeMap<&str, Value>, name: &str) -> Result<&'a str, String> {
    match args
        .get(name)
        .ok_or_else(|| format!("Missing {name}"))?
        .primitive()?
    {
        Literal::Text { value } => Ok(value),
        _ => Err(format!("{name} requires text")),
    }
}
impl State<'_> {
    pub(super) fn attribute_node(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
        kind: Builtin,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(input)) = args.get("geometry") else {
            return Err("Attribute operation requires geometry".into());
        };
        let domain = Domain::parse(text(args, "domain")?)?;
        let name = text(args, "attribute")?;
        let count = input.count(domain)?;
        self.reserve(count)?;
        let mut output = input.as_ref().clone();
        let values = match kind {
            Builtin::AttributeCreate => {
                let value = args.get("value").ok_or("Missing value")?.primitive()?;
                super::attributes::check_storage(count, value)?;
                self.reserve(
                    count.saturating_mul(
                        super::attributes::value_bytes(value)
                            .div_ceil(128)
                            .saturating_sub(1),
                    ),
                )?;
                vec![value.clone(); count]
            }
            Builtin::AttributeRandomize => {
                let min = args.get("minimum").ok_or("Missing minimum")?.primitive()?;
                let max = args.get("maximum").ok_or("Missing maximum")?.primitive()?;
                if min.ty() != max.ty() {
                    return Err("Random bounds must have matching types".into());
                }
                let Literal::Integer { value: seed } =
                    args.get("seed").ok_or("Missing seed")?.primitive()?
                else {
                    return Err("Seed must be integer".into());
                };
                let mut values = Vec::with_capacity(count);
                for element in input.element_ids(domain)? {
                    self.budget.step()?;
                    let random = |axis| {
                        crate::execution::random_v1(
                            *seed as u64,
                            id.to_bytes(),
                            Some(element),
                            &format!("{name}:{axis}"),
                        )
                    };
                    values.push(match (min, max) {
                        (Literal::Integer { value: a }, Literal::Integer { value: b }) => {
                            if a > b {
                                return Err("Minimum exceeds maximum".into());
                            }
                            let bits = super::attributes::stable_id(*seed as u64, element);
                            let width = (*b as i128 - *a as i128 + 1) as u128;
                            Literal::Integer {
                                value: (*a as i128 + (bits as u128 % width) as i128) as i64,
                            }
                        }
                        (Literal::Scalar { value: a }, Literal::Scalar { value: b }) => {
                            if a > b {
                                return Err("Minimum exceeds maximum".into());
                            }
                            Literal::Scalar {
                                value: a + (b - a) * random(0),
                            }
                        }
                        (
                            Literal::Vector {
                                value: a,
                                unit,
                                space,
                            },
                            Literal::Vector { value: b, .. },
                        ) => {
                            if a.iter().zip(b).any(|(a, b)| a > b) {
                                return Err("Minimum exceeds maximum".into());
                            }
                            let mut v = Literal::Vector {
                                value: a
                                    .iter()
                                    .zip(b)
                                    .enumerate()
                                    .map(|(i, (a, b))| a + (b - a) * random(i))
                                    .collect(),
                                unit: *unit,
                                space: *space,
                            };
                            if name == "N" {
                                let n = xyz(&v)?;
                                let length = n.iter().map(|x| x * x).sum::<f64>().sqrt();
                                if length < 1e-12 {
                                    return Err("Random normal has zero length".into());
                                }
                                v = vector(n.map(|x| x / length), Unit::Unitless)
                            }
                            v
                        }
                        (
                            Literal::Rotation { value: a, space },
                            Literal::Rotation { value: b, .. },
                        ) => Literal::Rotation {
                            value: fold_foundation::rotation::slerp(*a, *b, random(0)),
                            space: *space,
                        },
                        _ => {
                            return Err(
                                "Randomize supports scalar, integer, vector and quaternion bounds"
                                    .into(),
                            );
                        }
                    });
                }
                values
            }
            Builtin::AttributeTransfer => {
                let Some(Value::Geometry(source)) = args.get("source") else {
                    return Err("Transfer requires source geometry".into());
                };
                if source.space != input.space || source.unit != input.unit {
                    return Err(
                        "Transfer inputs must use the same coordinate space and units".into(),
                    );
                }
                let from = Domain::parse(text(args, "source_domain")?)?;
                let Literal::Distance {
                    value: radius,
                    unit: Unit::Meters,
                } = args.get("radius").ok_or("Missing radius")?.primitive()?
                else {
                    return Err("Radius requires meters".into());
                };
                if !radius.is_finite() || *radius <= 0. {
                    return Err("Radius must be positive".into());
                }
                let sampling = text(args, "sampling")?;
                if !matches!(sampling, "nearest" | "blend") {
                    return Err("Sampling must be nearest or blend".into());
                }
                let mut samples = Vec::new();
                for i in 0..source.count(from)? {
                    self.budget.step()?;
                    let value = source.read_attribute(from, name, i)?;
                    if sampling == "blend" {
                        blend(&value, &value, 0.)?;
                    }
                    samples.push((source.position(from, i)?, value));
                }
                let mut values = Vec::with_capacity(count);
                for index in 0..count {
                    self.budget.step()?;
                    let p = input.position(domain, index)?;
                    let mut nearest = None;
                    let mut blended = None;
                    let mut weight = 0.;
                    for (q, v) in &samples {
                        self.budget.step()?;
                        let d = p
                            .iter()
                            .zip(q)
                            .map(|(a, b)| (a - b) * (a - b))
                            .sum::<f64>()
                            .sqrt();
                        if d > *radius {
                            continue;
                        }
                        if nearest.as_ref().is_none_or(|(old, _)| d < *old) {
                            nearest = Some((d, v.clone()))
                        }
                        if sampling == "blend" {
                            let w = (1. - d / radius).max(0.);
                            if w > 0. {
                                blended = Some(match blended {
                                    None => v.clone(),
                                    Some(old) => blend(&old, v, w / (weight + w))?,
                                });
                                weight += w;
                            }
                        }
                    }
                    let value = blended
                        .or_else(|| nearest.map(|(_, v)| v))
                        .map(Ok)
                        .unwrap_or_else(|| input.read_attribute(domain, name, index))?;
                    self.reserve(
                        super::attributes::value_bytes(&value)
                            .div_ceil(128)
                            .saturating_sub(1),
                    )?;
                    values.push(value);
                }
                values
            }
            _ => return Err("Unknown attribute operation".into()),
        };
        if values.is_empty() {
            let ty = match kind {
                Builtin::AttributeCreate => args["value"].primitive()?.ty(),
                Builtin::AttributeRandomize => args["minimum"].primitive()?.ty(),
                _ => {
                    let Value::Geometry(source) = &args["source"] else {
                        unreachable!()
                    };
                    source
                        .attributes
                        .get(Domain::parse(text(args, "source_domain")?)?, name)
                        .ok_or("Missing source attribute schema")?
                        .value_type
                        .clone()
                }
            };
            output.set_attribute_typed(domain, name, ty, values)?;
        } else {
            output.set_attribute(domain, name, values)?;
        }
        output.id = self.identity(scope, id, input.id, b"attributes")?;
        Ok(Value::Geometry(Arc::new(output)))
    }
}
fn blend(a: &Literal, b: &Literal, t: f64) -> Result<Literal, String> {
    if a.ty() != b.ty() {
        return Err("Transfer source attribute type mismatch".into());
    }
    Ok(match(a,b){
 (Literal::Scalar{value:a},Literal::Scalar{value:b})=>Literal::Scalar{value:a+(b-a)*t},
 (Literal::Vector{value:a,unit,space},Literal::Vector{value:b,..})=>Literal::Vector{value:a.iter().zip(b).map(|(a,b)|a+(b-a)*t).collect(),unit:*unit,space:*space},
 (Literal::Rotation{value:a,space},Literal::Rotation{value:b,..})=>Literal::Rotation{value:fold_foundation::rotation::slerp(*a,*b,t),space:*space},
 _=>return Err("Blend supports scalar, vector and quaternion attributes; use nearest for categorical values".into())})
}
