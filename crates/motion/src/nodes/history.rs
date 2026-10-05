//! Sample upstream geometry at exact past times. Works for pure graphs and solvers.
use super::*;
use crate::{
    evaluation::content,
    geometry::{Element, Geometry, derived_id},
};
use fold_foundation::Time;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub interval: Time,
    pub start: Time,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            interval: Time::new(1, 25).unwrap(),
            start: Time::ZERO,
        }
    }
}
pub fn definition_history() -> Definition {
    let mut d = definition(
        "fold.motion.history",
        "Geometry History",
        "Simulation",
        vec![
            Socket::required("content", Kind::Content),
            scalar("samples", 8., "integer"),
            scalar("fade", 0.8, "factor"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let settings = n.settings::<Settings>()?;
            let samples = bounded_count(e.scalar(n, "samples")?, 128)?;
            let fade = e.scalar(n, "fade")?;
            if !(0. ..=1.).contains(&fade) {
                return Err("history fade must be in 0..1".into());
            }
            let mut result = Vec::new();
            // Establish identity once for all temporal samples, including nested solvers.
            e.simulation_key(n.id)?;
            for age in (0..samples).rev() {
                e.budget.spend()?;
                let delta = settings
                    .interval
                    .checked_scale(age as i64, 1)
                    .map_err(|e| e.to_string())?;
                let time = e.time.checked_sub(delta).map_err(|e| e.to_string())?;
                if time < settings.start {
                    continue;
                }
                let crate::evaluation::Value::Content(source) =
                    e.temporal_input(n, "content", time)?
                else {
                    return Err("history requires Content".into());
                };
                let mut wrapper =
                    Element::new(derived_id(n.seed(), age as u64), Geometry::Group(source));
                wrapper.opacity = fade.powi(age as i32);
                wrapper.attributes.insert(
                    "history_age".into(),
                    Datum::Scalar(delta.numerator() as f64 / delta.denominator() as f64),
                );
                wrapper
                    .attributes
                    .insert("history_index".into(), Datum::Scalar(age as f64));
                result.push(wrapper);
            }
            let result = Arc::new(result);
            crate::simulation::state_bytes(&result, &mut e.budget)?;
            Ok(content(result))
        },
    );
    d.defaults = || serde_json::to_value(Settings::default()).unwrap();
    d.validate = |n| {
        let s = n.settings::<Settings>()?;
        if s.interval < Time::new(1, 1000).unwrap() {
            Err("history interval must be at least 1 ms".into())
        } else {
            Ok(())
        }
    };
    d
}
