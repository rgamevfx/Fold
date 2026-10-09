//! Language types and finite authored literals. Units and spaces are not coercions.
use fold_foundation::rotation::normalize;
use fold_foundation::{AssetId, ObjectId, Time};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Space {
    Screen,
    World,
    /// Coordinates on a particular camera plane require a binding at placement.
    CameraPlane,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Unit {
    Unitless,
    Canvas,
    Meters,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Encoding {
    LinearRec709,
    AcesCg,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Alpha {
    Straight,
    Premultiplied,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Domain {
    Collection,
    Point,
    Curve,
    Face,
    FaceCorner,
    Instance,
    Grapheme,
    Word,
    Line,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Type {
    /// Named-channel image data; color/alpha metadata belongs to the image resource.
    Image,
    /// Coverage data, distinct from authored color values.
    Mask,
    Bool,
    Integer,
    Scalar,
    Text,
    Duration,
    Distance {
        unit: Unit,
    },
    Vector {
        dimensions: u8,
        unit: Unit,
        space: Space,
    },
    Color {
        encoding: Encoding,
        alpha: Alpha,
    },
    Rotation {
        space: Space,
    },
    AssetRef,
    Geometry,
    Placement,
    Camera,
    Light,
    Scene,
    Signal {
        value: Box<Type>,
    },
    Field {
        value: Box<Type>,
        domain: Domain,
        space: Space,
    },
}
impl Type {
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Distance {
                unit: Unit::Unitless,
            } => Err("Distance requires a length unit".into()),
            Self::Vector {
                dimensions,
                unit,
                space,
            } => {
                if !(2..=3).contains(dimensions) {
                    return Err("Vectors require two or three components".into());
                }
                if matches!(
                    (unit, space),
                    (Unit::Canvas, Space::World) | (Unit::Meters, Space::Screen)
                ) {
                    return Err("Vector units do not match its space".into());
                }
                Ok(())
            }
            Self::Signal { value } | Self::Field { value, .. } => {
                if !value.is_primitive() {
                    return Err("Signals and fields require a primitive value type".into());
                }
                value.validate()
            }
            _ => Ok(()),
        }
    }
    pub fn is_primitive(&self) -> bool {
        matches!(
            self,
            Self::Bool
                | Self::Integer
                | Self::Scalar
                | Self::Text
                | Self::Duration
                | Self::Distance { .. }
                | Self::Vector { .. }
                | Self::Color { .. }
                | Self::Rotation { .. }
                | Self::AssetRef
        )
    }
    pub fn is_interpolatable(&self) -> bool {
        matches!(
            self,
            Self::Scalar
                | Self::Distance { .. }
                | Self::Vector { .. }
                | Self::Color { .. }
                | Self::Rotation { .. }
        )
    }
    pub fn sampled(&self) -> &Self {
        match self {
            Self::Signal { value } | Self::Field { value, .. } => value,
            value => value,
        }
    }
}

/// Definitions belong to the native construction; resources belong to the project.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", content = "id", rename_all = "snake_case")]
pub enum AssetReference {
    Definition(ObjectId),
    Resource(AssetId),
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Literal {
    Bool {
        value: bool,
    },
    Integer {
        value: i64,
    },
    Scalar {
        value: f64,
    },
    Text {
        value: String,
    },
    Duration {
        value: Time,
    },
    Distance {
        value: f64,
        unit: Unit,
    },
    Vector {
        value: Vec<f64>,
        unit: Unit,
        space: Space,
    },
    Color {
        value: [f64; 4],
        encoding: Encoding,
        alpha: Alpha,
    },
    /// Unit quaternion in x/y/z/w order. Euler construction requires an order.
    Rotation {
        value: [f64; 4],
        space: Space,
    },
    AssetRef {
        value: AssetReference,
    },
}
impl Literal {
    pub fn ty(&self) -> Type {
        match self {
            Self::Bool { .. } => Type::Bool,
            Self::Integer { .. } => Type::Integer,
            Self::Scalar { .. } => Type::Scalar,
            Self::Text { .. } => Type::Text,
            Self::Duration { .. } => Type::Duration,
            Self::Distance { unit, .. } => Type::Distance { unit: *unit },
            Self::Vector { value, unit, space } => Type::Vector {
                dimensions: u8::try_from(value.len()).unwrap_or(u8::MAX),
                unit: *unit,
                space: *space,
            },
            Self::Color {
                encoding, alpha, ..
            } => Type::Color {
                encoding: *encoding,
                alpha: *alpha,
            },
            Self::Rotation { space, .. } => Type::Rotation { space: *space },
            Self::AssetRef { .. } => Type::AssetRef,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        self.ty().validate()?;
        let finite = match self {
            Self::Scalar { value } | Self::Distance { value, .. } => value.is_finite(),
            Self::Vector { value, .. } => value.iter().all(|v| v.is_finite()),
            Self::Color { value, alpha, .. } => {
                value.iter().all(|v| v.is_finite())
                    && (0. ..=1.).contains(&value[3])
                    && (*alpha != Alpha::Premultiplied || value[3] != 0. || value[..3] == [0.; 3])
            }
            Self::Rotation { value, .. } => {
                value.iter().all(|v| v.is_finite()) && (dot(*value, *value) - 1.).abs() <= 1e-9
            }
            Self::Text { value } => value.len() <= 64 * 1024,
            _ => true,
        };
        if !finite {
            return Err("Invalid literal: nonfinite value, alpha, quaternion or text limit".into());
        }
        Ok(())
    }
    /// Linear interpolation of authored components; quaternions use shortest-arc SLERP.
    /// Color encoding/alpha, vector dimensions, units and spaces must match exactly.
    pub fn interpolate(&self, other: &Self, amount: f64) -> Result<Self, String> {
        self.validate()?;
        other.validate()?;
        if self.ty() != other.ty() || !amount.is_finite() || !(0. ..=1.).contains(&amount) {
            return Err(
                "Interpolation requires identical types and a finite weight in 0..1".into(),
            );
        }
        let mix = |a: f64, b: f64| a * (1. - amount) + b * amount;
        let result = match (self, other) {
            (Self::Scalar { value: a }, Self::Scalar { value: b }) => {
                Self::Scalar { value: mix(*a, *b) }
            }
            (Self::Distance { value: a, unit }, Self::Distance { value: b, .. }) => {
                Self::Distance {
                    value: mix(*a, *b),
                    unit: *unit,
                }
            }
            (
                Self::Vector {
                    value: a,
                    unit,
                    space,
                },
                Self::Vector { value: b, .. },
            ) => Self::Vector {
                value: a.iter().zip(b).map(|(a, b)| mix(*a, *b)).collect(),
                unit: *unit,
                space: *space,
            },
            (
                Self::Color {
                    value: a,
                    encoding,
                    alpha,
                },
                Self::Color { value: b, .. },
            ) => Self::Color {
                value: std::array::from_fn(|i| mix(a[i], b[i])),
                encoding: *encoding,
                alpha: *alpha,
            },
            (Self::Rotation { value: a, space }, Self::Rotation { value: b, .. }) => {
                Self::Rotation {
                    value: fold_foundation::rotation::slerp(*a, *b, amount),
                    space: *space,
                }
            }
            _ => return Err("Type does not support interpolation".into()),
        };
        result.validate()?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum EulerOrder {
    Xyz,
    Xzy,
    Yxz,
    Yzx,
    Zxy,
    Zyx,
}
impl Literal {
    /// Intrinsic rotations, applied in the explicitly named order, in radians.
    pub fn euler(radians: [f64; 3], order: EulerOrder, space: Space) -> Result<Self, String> {
        if radians.iter().any(|v| !v.is_finite()) {
            return Err("Euler angles must be finite".into());
        }
        let axes = match order {
            EulerOrder::Xyz => [0, 1, 2],
            EulerOrder::Xzy => [0, 2, 1],
            EulerOrder::Yxz => [1, 0, 2],
            EulerOrder::Yzx => [1, 2, 0],
            EulerOrder::Zxy => [2, 0, 1],
            EulerOrder::Zyx => [2, 1, 0],
        };
        let mut value = [0., 0., 0., 1.];
        for axis in axes {
            let mut q = [0.; 4];
            q[axis] = (radians[axis] * 0.5).sin();
            q[3] = (radians[axis] * 0.5).cos();
            value = multiply(value, q);
        }
        let result = Self::Rotation {
            value: normalize(value),
            space,
        };
        result.validate()?;
        Ok(result)
    }
}
fn dot(a: [f64; 4], b: [f64; 4]) -> f64 {
    (0..4).map(|i| a[i] * b[i]).sum()
}
fn multiply(a: [f64; 4], b: [f64; 4]) -> [f64; 4] {
    [
        a[3] * b[0] + a[0] * b[3] + a[1] * b[2] - a[2] * b[1],
        a[3] * b[1] - a[0] * b[2] + a[1] * b[3] + a[2] * b[0],
        a[3] * b[2] + a[0] * b[1] - a[1] * b[0] + a[2] * b[3],
        a[3] * b[3] - a[0] * b[0] - a[1] * b[1] - a[2] * b[2],
    ]
}
/// Dimensionless exact ratio. Reuses the validated rational representation, not time units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(transparent)]
pub struct Ratio(Time);
impl Ratio {
    pub fn new(numerator: i64, denominator: u32) -> Result<Self, String> {
        Time::new(numerator, denominator)
            .map(Self)
            .map_err(|e| e.to_string())
    }
    pub fn numerator(self) -> i64 {
        self.0.numerator()
    }
    pub fn denominator(self) -> u32 {
        self.0.denominator()
    }
}
impl Default for Ratio {
    fn default() -> Self {
        Self(Time::new(1, 1).expect("unit ratio"))
    }
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct Clock {
    pub offset: Time,
    pub rate: Ratio,
}
impl Default for Clock {
    fn default() -> Self {
        Self {
            offset: Time::ZERO,
            rate: Ratio::default(),
        }
    }
}
impl Clock {
    pub fn at(self, parent: Time) -> Result<Time, String> {
        parent
            .checked_sub(self.offset)
            .and_then(|v| v.checked_scale(self.rate.numerator(), self.rate.denominator()))
            .map_err(|e| e.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn units_spaces_and_color_contracts_cannot_be_interpolated_implicitly() {
        let a = Literal::Distance {
            value: 2.,
            unit: Unit::Canvas,
        };
        let b = Literal::Distance {
            value: 2.,
            unit: Unit::Meters,
        };
        assert!(a.interpolate(&b, 0.5).is_err());
        assert!(
            Literal::Vector {
                value: vec![1., 2.],
                unit: Unit::Meters,
                space: Space::Screen
            }
            .validate()
            .is_err()
        );
        let a = Literal::Color {
            value: [1., 0., 0., 0.5],
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Straight,
        };
        let b = Literal::Color {
            value: [1., 0., 0., 0.5],
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Premultiplied,
        };
        assert!(a.interpolate(&b, 0.5).is_err());
        assert!(
            Literal::Scalar {
                value: f64::INFINITY
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn quaternion_interpolation_takes_the_short_arc_and_requires_explicit_euler_order() {
        let a = Literal::euler(
            [0., 0., 170_f64.to_radians()],
            EulerOrder::Xyz,
            Space::World,
        )
        .unwrap();
        let b = Literal::euler(
            [0., 0., -170_f64.to_radians()],
            EulerOrder::Xyz,
            Space::World,
        )
        .unwrap();
        let Literal::Rotation { value, .. } = a.interpolate(&b, 0.5).unwrap() else {
            panic!()
        };
        assert!((value[2].abs() - 1.).abs() < 1e-12);
        assert!(value[3].abs() < 1e-12);
        let angles = [0.4, 0.6, 0.8];
        assert_ne!(
            Literal::euler(angles, EulerOrder::Xyz, Space::World).unwrap(),
            Literal::euler(angles, EulerOrder::Zyx, Space::World).unwrap()
        );
        assert!(
            Literal::Rotation {
                value: [0.; 4],
                space: Space::World
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn asset_clocks_are_exact_for_negative_freeze_and_reverse_time() {
        let offset = Time::new(1, 4).unwrap();
        let clock = Clock {
            offset,
            rate: Ratio::new(-3, 2).unwrap(),
        };
        assert_eq!(
            clock.at(Time::new(-1, 4).unwrap()).unwrap(),
            Time::new(3, 4).unwrap()
        );
        assert_eq!(
            Clock {
                offset,
                rate: Ratio::new(0, 1).unwrap()
            }
            .at(Time::new(-8, 1).unwrap())
            .unwrap(),
            Time::ZERO
        );
        assert!(Ratio::new(1, 0).is_err());
        assert!(
            Clock {
                offset: Time::new(i64::MIN, 1).unwrap(),
                rate: Ratio::default()
            }
            .at(Time::new(i64::MAX, 1).unwrap())
            .is_err()
        );
    }
}
