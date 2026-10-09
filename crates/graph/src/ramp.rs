//! Identity-bearing scalar/color stops. Positions are dimensionless, never time.
use crate::types::{Alpha, Encoding, Literal, Type};
use fold_foundation::ObjectId;
use fold_project::Metadata;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Ramp {
    pub id: ObjectId,
    pub ty: Type,
    pub stops: Vec<Stop>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Stop {
    pub id: ObjectId,
    pub position: f64,
    pub value: Literal,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Ramp {
    pub fn two(left: Literal, right: Literal) -> Result<Self, String> {
        let ramp = Self {
            id: ObjectId::new(),
            ty: left.ty(),
            stops: vec![
                Stop {
                    id: ObjectId::new(),
                    position: 0.,
                    value: left,
                    extensions: Metadata::default(),
                },
                Stop {
                    id: ObjectId::new(),
                    position: 1.,
                    value: right,
                    extensions: Metadata::default(),
                },
            ],
            extensions: Metadata::default(),
        };
        ramp.validate()?;
        Ok(ramp)
    }
    pub fn validate(&self) -> Result<(), String> {
        crate::model::extension_keys(&self.extensions, &["id", "ty", "stops"])?;
        if !matches!(
            self.ty,
            Type::Scalar
                | Type::Color {
                    encoding: Encoding::LinearRec709,
                    alpha: Alpha::Straight
                }
        ) || self.stops.is_empty()
            || self.stops.len() > 256
        {
            return Err(
                "Ramp requires Scalar or straight linear Rec.709 Color and 1..256 stops".into(),
            );
        }
        let mut ids = std::collections::BTreeSet::from([self.id]);
        for (index, stop) in self.stops.iter().enumerate() {
            crate::model::extension_keys(&stop.extensions, &["id", "position", "value"])?;
            stop.value.validate()?;
            if !ids.insert(stop.id)
                || stop.value.ty() != self.ty
                || !stop.position.is_finite()
                || !(0. ..=1.).contains(&stop.position)
                || index > 0 && self.stops[index - 1].position >= stop.position
            {
                return Err("Ramp stops require unique IDs, matching types and increasing positions in [0,1]".into());
            }
        }
        Ok(())
    }
    /// Endpoint clamp; component-linear interpolation in the authored value type.
    pub fn sample(&self, position: f64) -> Result<Literal, String> {
        self.validate()?;
        if !position.is_finite() {
            return Err("Ramp input must be finite".into());
        }
        let upper = self.stops.partition_point(|s| s.position <= position);
        if upper == 0 {
            return Ok(self.stops[0].value.clone());
        }
        if upper == self.stops.len() {
            return Ok(self.stops[upper - 1].value.clone());
        }
        let left = &self.stops[upper - 1];
        let right = &self.stops[upper];
        left.value.interpolate(
            &right.value,
            (position - left.position) / (right.position - left.position),
        )
    }
    pub fn insert(&mut self, position: f64) -> Result<ObjectId, String> {
        self.validate()?;
        if self.stops.len() == 256
            || !position.is_finite()
            || !(0. ..=1.).contains(&position)
            || self.stops.iter().any(|s| s.position == position)
        {
            return Err("Choose an unused ramp position in [0,1]".into());
        }
        let value = self.sample(position)?;
        let id = ObjectId::new();
        self.stops.push(Stop {
            id,
            position,
            value,
            extensions: Metadata::default(),
        });
        self.stops.sort_by(|a, b| a.position.total_cmp(&b.position));
        Ok(id)
    }
}
