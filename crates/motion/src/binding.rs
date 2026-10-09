//! Native parameter bindings and key identities; never a second source document.
use crate::{
    expression::Expr,
    model::{Edge, Parameter},
    types::{Literal, Type},
};
use fold_foundation::{ObjectId, Time};
use fold_project::Metadata;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Binding {
    Literal { value: Literal },
    Link { reference: Edge },
    Expression { expression: Expr },
    Track { track: Track },
    Ramp { ramp: crate::ramp::Ramp },
    List { values: Vec<Binding> },
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Interpolation {
    Linear,
    Hold,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Combination {
    Replace,
    Add,
    Multiply,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Track {
    pub id: ObjectId,
    pub ty: Type,
    pub keys: Vec<Key>,
    pub combination: Combination,
    /// Explicit base for Add/Multiply; replacement has no base.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<Literal>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct Key {
    pub id: ObjectId,
    pub time: Time,
    pub value: Literal,
    /// Segment beginning at this key. Endpoint extrapolation always holds.
    pub interpolation: Interpolation,
    #[serde(flatten)]
    pub extensions: Metadata,
}
impl Parameter {
    pub fn new(binding: Binding) -> Result<Self, String> {
        Ok(Self {
            id: ObjectId::new(),
            value: serde_json::to_value(binding).map_err(|e| e.to_string())?,
            protected: false,
            extensions: Metadata::default(),
        })
    }
    pub fn binding(&self) -> Result<Binding, String> {
        serde_json::from_value(self.value.clone()).map_err(|e| e.to_string())
    }
}
impl Track {
    /// Start an identity-bearing constant track without changing the authored value.
    pub fn constant(value: Literal, time: Time) -> Result<Self, String> {
        let track = Self {
            id: ObjectId::new(),
            ty: value.ty(),
            combination: Combination::Replace,
            base: None,
            extensions: Metadata::default(),
            keys: vec![Key {
                id: ObjectId::new(),
                time,
                value,
                interpolation: Interpolation::Linear,
                extensions: Metadata::default(),
            }],
        };
        track.validate()?;
        Ok(track)
    }
    pub fn validate(&self) -> Result<(), String> {
        crate::model::extension_keys(
            &self.extensions,
            &["id", "ty", "keys", "combination", "base"],
        )?;
        self.ty.validate()?;
        if !self.ty.is_interpolatable() || self.keys.is_empty() || self.keys.len() > 4096 {
            return Err("Keyframes require an interpolatable type and 1..4096 keys".into());
        }
        let mut ids = std::collections::BTreeSet::from([self.id]);
        for (i, key) in self.keys.iter().enumerate() {
            crate::model::extension_keys(
                &key.extensions,
                &["id", "time", "value", "interpolation"],
            )?;
            key.value.validate()?;
            if !ids.insert(key.id)
                || key.value.ty() != self.ty
                || i > 0 && self.keys[i - 1].time >= key.time
            {
                return Err(
                    "Key identities, types and strictly increasing times are required".into(),
                );
            }
        }
        match (&self.combination, &self.base) {
            (Combination::Replace, None) => {}
            (Combination::Add | Combination::Multiply, Some(base)) => {
                base.validate()?;
                let op = if self.combination == Combination::Add {
                    crate::expression::Binary::Add
                } else {
                    crate::expression::Binary::Multiply
                };
                let result = crate::expression::binary_type(op, &base.ty(), &self.ty)?;
                if base.ty() != self.ty || result != self.ty {
                    return Err("Channel base, keys and result must have the same type".into());
                }
            }
            _ => return Err("Replace has no base; Add/Multiply require an explicit base".into()),
        }
        Ok(())
    }
}

impl Track {
    /// Stateless sampling with endpoint hold and exact rational interval selection.
    pub fn sample(&self, time: Time) -> Result<Literal, String> {
        self.validate()?;
        let upper = self.keys.partition_point(|key| key.time <= time);
        let value = if upper == 0 {
            self.keys[0].value.clone()
        } else if upper == self.keys.len()
            || self.keys[upper - 1].interpolation == Interpolation::Hold
        {
            self.keys[upper - 1].value.clone()
        } else {
            let left = &self.keys[upper - 1];
            let right = &self.keys[upper];
            let elapsed = time.checked_sub(left.time).map_err(|e| e.to_string())?;
            let duration = right
                .time
                .checked_sub(left.time)
                .map_err(|e| e.to_string())?;
            let amount = (elapsed.numerator() as f64 * f64::from(duration.denominator()))
                / (duration.numerator() as f64 * f64::from(elapsed.denominator()));
            left.value.interpolate(&right.value, amount)?
        };
        match self.combination {
            Combination::Replace => Ok(value),
            Combination::Add | Combination::Multiply => crate::execution::binary(
                if self.combination == Combination::Add {
                    crate::expression::Binary::Add
                } else {
                    crate::expression::Binary::Multiply
                },
                self.base.clone().ok_or("Missing channel base")?,
                value,
            ),
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    fn track() -> Track {
        Track {
            id: ObjectId::new(),
            ty: Type::Scalar,
            combination: Combination::Replace,
            base: None,
            keys: [(0, 0.0), (1, 1.0), (2, 0.5)]
                .into_iter()
                .map(|(time, value)| Key {
                    id: ObjectId::new(),
                    time: Time::new(time, 2).unwrap(),
                    value: Literal::Scalar { value },
                    interpolation: Interpolation::Linear,
                    extensions: Metadata::new(),
                })
                .collect(),
            extensions: Metadata::new(),
        }
    }
    #[test]
    fn random_seek_endpoint_hold_and_explicit_channel_composition() {
        let mut track = track();
        for (n, d, expected) in [
            (2, 1, 0.5),
            (1, 4, 0.5),
            (-1, 1, 0.0),
            (3, 4, 0.75),
            (1, 2, 1.0),
        ] {
            assert_eq!(
                track.sample(Time::new(n, d).unwrap()).unwrap(),
                Literal::Scalar { value: expected }
            );
        }
        track.combination = Combination::Add;
        assert!(track.validate().is_err());
        track.base = Some(Literal::Scalar { value: 2.0 });
        assert_eq!(
            track.sample(Time::new(1, 4).unwrap()).unwrap(),
            Literal::Scalar { value: 2.5 }
        );
        track.combination = Combination::Multiply;
        assert_eq!(
            track.sample(Time::new(1, 4).unwrap()).unwrap(),
            Literal::Scalar { value: 1.0 }
        );
        track.keys[0].interpolation = Interpolation::Hold;
        assert_eq!(
            track.sample(Time::new(1, 4).unwrap()).unwrap(),
            Literal::Scalar { value: 0.0 }
        );
        track.keys[1].time = track.keys[0].time;
        assert!(track.validate().is_err());
    }
}
