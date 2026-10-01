//! Pure typed fields. Evaluation context is explicit and bounded; no playhead state.
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Kind {
    Scalar,
    Vector,
    Color,
    Bool,
    Text,
    Id,
    Content,
    Points,
    Generic,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Datum {
    Scalar(f64),
    Vector([f64; 2]),
    Color([f64; 4]),
    Bool(bool),
    Text(String),
    Id(u64),
}
impl Datum {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Scalar(_) => Kind::Scalar,
            Self::Vector(_) => Kind::Vector,
            Self::Color(_) => Kind::Color,
            Self::Bool(_) => Kind::Bool,
            Self::Text(_) => Kind::Text,
            Self::Id(_) => Kind::Id,
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        let finite = match self {
            Self::Scalar(v) => v.is_finite(),
            Self::Vector(v) => v.iter().all(|v| v.is_finite()),
            Self::Color(v) => v.iter().all(|v| v.is_finite()),
            Self::Text(t) => t.len() <= 65536,
            _ => true,
        };
        if finite {
            Ok(())
        } else {
            Err("nonfinite or oversized value".into())
        }
    }
    pub fn scalar(&self) -> Result<f64, String> {
        if let Self::Scalar(v) = self {
            Ok(*v)
        } else {
            Err("expected scalar".into())
        }
    }
    pub fn vector(&self) -> Result<[f64; 2], String> {
        if let Self::Vector(v) = self {
            Ok(*v)
        } else {
            Err("expected vector".into())
        }
    }
    pub fn color(&self) -> Result<[f64; 4], String> {
        if let Self::Color(v) = self {
            Ok(*v)
        } else {
            Err("expected color".into())
        }
    }
    pub fn boolean(&self) -> Result<bool, String> {
        if let Self::Bool(v) = self {
            Ok(*v)
        } else {
            Err("expected boolean".into())
        }
    }
    pub fn text(&self) -> Result<&str, String> {
        if let Self::Text(v) = self {
            Ok(v)
        } else {
            Err("expected text".into())
        }
    }
    pub fn numeric(&self, other: &Self, f: impl Fn(f64, f64) -> f64) -> Result<Self, String> {
        let result = match (self, other) {
            (Self::Scalar(a), Self::Scalar(b)) => Self::Scalar(f(*a, *b)),
            (Self::Vector(a), Self::Vector(b)) => {
                Self::Vector(std::array::from_fn(|i| f(a[i], b[i])))
            }
            (Self::Color(a), Self::Color(b)) => Self::Color(std::array::from_fn(|i| f(a[i], b[i]))),
            _ => return Err("numeric operands must have identical types".into()),
        };
        result.validate()?;
        Ok(result)
    }
    pub fn mix(&self, other: &Self, weight: f64) -> Result<Self, String> {
        self.numeric(other, |a, b| a + (b - a) * weight)
    }
}
#[derive(Clone, Copy, Debug, Default)]
pub struct Sample {
    pub position: [f64; 2],
    pub index: usize,
    pub id: u64,
    pub count: usize,
}
pub struct Budget {
    remaining: usize,
    cancel: fold_media::Cancel,
}
impl Default for Budget {
    fn default() -> Self {
        Self {
            remaining: 2_000_000,
            cancel: fold_media::Cancel::default(),
        }
    }
}
impl Budget {
    pub fn cancellable(cancel: &fold_media::Cancel) -> Self {
        Self {
            cancel: cancel.clone(),
            ..Self::default()
        }
    }
    pub fn spend(&mut self) -> Result<(), String> {
        self.cancel.check()?;
        self.remaining = self
            .remaining
            .checked_sub(1)
            .ok_or("motion field work budget exceeded")?;
        Ok(())
    }
}
type Function = dyn Fn(&Sample, &mut Budget) -> Result<Datum, String> + Send + Sync;
#[derive(Clone)]
pub struct Field {
    pub kind: Kind,
    pub varying: bool,
    eval: Arc<Function>,
}
impl std::fmt::Debug for Field {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Field")
            .field("kind", &self.kind)
            .field("varying", &self.varying)
            .finish()
    }
}
impl Field {
    pub fn new(
        kind: Kind,
        varying: bool,
        f: impl Fn(&Sample, &mut Budget) -> Result<Datum, String> + Send + Sync + 'static,
    ) -> Self {
        Self {
            kind,
            varying,
            eval: Arc::new(f),
        }
    }
    pub fn constant(value: Datum) -> Self {
        Self::new(value.kind(), false, move |_, _| Ok(value.clone()))
    }
    pub fn sample(&self, s: &Sample, b: &mut Budget) -> Result<Datum, String> {
        b.spend()?;
        let v = (self.eval)(s, b)?;
        v.validate()?;
        if v.kind() != self.kind {
            return Err("field returned incorrect type".into());
        }
        Ok(v)
    }
    pub fn uniform(&self, b: &mut Budget) -> Result<Datum, String> {
        if self.varying {
            return Err("element field requires an explicit consuming domain".into());
        }
        self.sample(&Sample::default(), b)
    }
    pub fn map(
        self,
        kind: Kind,
        f: impl Fn(Datum) -> Result<Datum, String> + Send + Sync + 'static,
    ) -> Self {
        Self::new(kind, self.varying, move |s, b| f(self.sample(s, b)?))
    }
    pub fn zip(
        self,
        other: Self,
        kind: Kind,
        f: impl Fn(Datum, Datum) -> Result<Datum, String> + Send + Sync + 'static,
    ) -> Self {
        Self::new(kind, self.varying || other.varying, move |s, b| {
            f(self.sample(s, b)?, other.sample(s, b)?)
        })
    }
}
