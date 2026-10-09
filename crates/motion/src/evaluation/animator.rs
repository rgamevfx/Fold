use super::{Geometry, GeometryData, Scope, State, Value};
use crate::{
    binding::Binding,
    execution::ElementContext,
    expression::{Binary, Element, Expr, Function},
    registry::Builtin,
    types::{Literal, Space, Unit},
};
use fold_foundation::ObjectId;
use std::{collections::BTreeMap, sync::Arc};

impl State<'_> {
    pub(super) fn animate_instances(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
        channel: Builtin,
        binding: &Binding,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(source)) = args.get("geometry") else {
            return Err("Animator requires instances".into());
        };
        let GeometryData::Instances(collection) = &source.data else {
            return Err("Animator requires Cloner instances".into());
        };
        self.reserve(1 + collection.instances.len())?;
        let mut animated = collection.as_ref().clone();
        for (index, instance) in animated.instances.iter_mut().enumerate() {
            self.budget.step()?;
            let sampled = self.sample_binding(
                scope,
                id,
                binding,
                scope.time,
                Some(ElementContext {
                    id: instance.id,
                    index: index as i64,
                    count: collection.instances.len() as i64,
                    position: Some(Literal::Vector {
                        value: instance.position.to_vec(),
                        unit: Unit::Meters,
                        space: Space::World,
                    }),
                }),
            )?;
            match channel {
                Builtin::InstanceScale => {
                    let Literal::Scalar { value } = sampled.primitive()? else {
                        return Err("Scale multiplier must be scalar".into());
                    };
                    let scale = instance.scale * value;
                    if !value.is_finite()
                        || *value <= 0.
                        || !scale.is_finite()
                        || !(1e-6..=1e6).contains(&scale)
                    {
                        return Err("Instance scale must remain in [1e-6,1e6]".into());
                    }
                    instance.scale = scale;
                }
                Builtin::InstanceRotationZ => {
                    let Literal::Scalar { value } = sampled.primitive()? else {
                        return Err("Z rotation requires a scalar angle in radians".into());
                    };
                    if !value.is_finite() || value.abs() > 1e6 {
                        return Err("Z rotation angle must be within ±1e6 radians".into());
                    }
                    instance.rotation_z =
                        (instance.rotation_z + value).rem_euclid(std::f64::consts::TAU);
                }
                Builtin::InstanceColor => {
                    instance.color = Some(super::material::surface_color(sampled.primitive()?)?)
                }
                _ => return Err("Unsupported instance animation channel".into()),
            }
        }
        Ok(Value::Geometry(Arc::new(Geometry {
            id: self.identity(
                scope,
                id,
                source.id,
                match channel {
                    Builtin::InstanceScale => b"scale_instances",
                    Builtin::InstanceColor => b"color_instances",
                    _ => b"rotation_instances",
                },
            )?,
            space: source.space,
            unit: source.unit,
            appearance: source.appearance.clone(),
            data: GeometryData::Instances(Arc::new(animated)),
        })))
    }

    pub(super) fn instance_random(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        if self.element.is_none() {
            return Err("Random requires an instance context".into());
        }
        let bound = |name| match args
            .get(name)
            .ok_or_else(|| format!("Missing random {name}"))?
            .primitive()?
        {
            Literal::Scalar { value } if value.is_finite() && value.abs() <= 1e6 => Ok(*value),
            _ => Err("Random bounds must be finite scalars within ±1e6".to_string()),
        };
        let minimum = bound("minimum")?;
        let maximum = bound("maximum")?;
        if minimum > maximum {
            return Err("Random minimum must not exceed maximum".into());
        }
        let Literal::Text { value: stream } = args
            .get("stream")
            .ok_or("Missing random stream")?
            .primitive()?
        else {
            return Err("Random stream must be text".into());
        };
        // The expression owns stream/version validation and the established hash.
        let sampled = self.binding(
            scope,
            id,
            &Binding::Expression {
                expression: Expr::Random {
                    version: 1,
                    stream: stream.clone(),
                },
            },
        )?;
        let Literal::Scalar { value: amount } = sampled.primitive()? else {
            return Err("Random expression must return a scalar".into());
        };
        Ok(Value::Primitive(Literal::Scalar {
            value: minimum + (maximum - minimum) * amount,
        }))
    }

    pub(super) fn linear_field(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let endpoint = |name| -> Result<[f64; 3], String> {
            let Literal::Vector {
                value,
                unit: Unit::Meters,
                space: Space::World,
            } = args
                .get(name)
                .ok_or_else(|| format!("Missing field {name}"))?
                .primitive()?
            else {
                return Err("Linear Field endpoints require World meters".into());
            };
            if value.len() != 3 || value.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
                return Err("Linear Field endpoint exceeds supported extent".into());
            }
            Ok([value[0], value[1], value[2]])
        };
        let start = endpoint("start")?;
        let end = endpoint("end")?;
        let direction: [f64; 3] = std::array::from_fn(|i| end[i] - start[i]);
        let length_squared = direction.iter().map(|v| v * v).sum::<f64>();
        if length_squared < 1e-12 {
            return Err("Linear Field endpoints must be at least 1e-6 meters apart".into());
        }
        // Read position through the same typed expression context as Radial Field.
        // Projection is a fixed three-component spatial operation, sharing the
        // sampler's clock, dependency cache, cancellation and work budget.
        let position = self.binding(
            scope,
            id,
            &Binding::Expression {
                expression: Expr::Element {
                    property: Element::Position {
                        unit: Unit::Meters,
                        space: Space::World,
                    },
                },
            },
        )?;
        let Literal::Vector {
            value: position, ..
        } = position.primitive()?
        else {
            return Err("Linear Field requires an instance position".into());
        };
        let amount = (0..3)
            .map(|i| (position[i] - start[i]) * direction[i])
            .sum::<f64>()
            / length_squared;
        Ok(Value::Primitive(Literal::Scalar {
            value: amount.clamp(0., 1.),
        }))
    }

    pub(super) fn radial_field(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let center = args
            .get("center")
            .ok_or("Missing field center")?
            .primitive()?
            .clone();
        let Literal::Vector {
            value,
            unit: Unit::Meters,
            space: Space::World,
        } = &center
        else {
            return Err("Radial Field requires a World metric center".into());
        };
        if value.len() != 3 || value.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
            return Err("Field center exceeds supported extent".into());
        }
        let radius = args
            .get("radius")
            .ok_or("Missing field radius")?
            .primitive()?
            .clone();
        let Literal::Distance {
            value,
            unit: Unit::Meters,
        } = &radius
        else {
            return Err("Field radius requires meters".into());
        };
        if !value.is_finite() || !(1e-6..=1e6).contains(value) {
            return Err("Field radius must be in [1e-6,1e6] meters".into());
        }
        let scalar = |value| Expr::Literal {
            value: Literal::Scalar { value },
        };
        let expression = Expr::Call {
            function: Function::Clamp,
            arguments: vec![
                Expr::Binary {
                    op: Binary::Subtract,
                    left: Box::new(scalar(1.)),
                    right: Box::new(Expr::Binary {
                        op: Binary::Divide,
                        left: Box::new(Expr::Call {
                            function: Function::Length,
                            arguments: vec![Expr::Binary {
                                op: Binary::Subtract,
                                left: Box::new(Expr::Element {
                                    property: Element::Position {
                                        unit: Unit::Meters,
                                        space: Space::World,
                                    },
                                }),
                                right: Box::new(Expr::Literal { value: center }),
                            }],
                        }),
                        right: Box::new(Expr::Literal { value: radius }),
                    }),
                },
                scalar(0.),
                scalar(1.),
            ],
        };
        self.binding(scope, id, &Binding::Expression { expression })
    }
}
