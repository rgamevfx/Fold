//! Delay any sampled value per element, including animated inputs and groups.
use super::*;
use crate::{evaluation::field, fields::Field};
pub fn definition_stagger() -> Definition {
    let mut d = definition(
        "fold.motion.stagger",
        "Stagger",
        "Animation",
        vec![
            scalar("value", 0., ""),
            scalar("delay", 0., "seconds / copy"),
        ],
        vec![("value", Kind::Generic)],
        |e, n| {
            let value = e.field(n, "value")?;
            let delay = e.field(n, "delay")?;
            Ok(field(Field::new(
                value.kind,
                true,
                move |sample, budget| {
                    let seconds = delay.sample(sample, budget)?.scalar()?;
                    let nanos = seconds * 1_000_000_000.;
                    if !nanos.is_finite() || nanos.abs() >= i64::MAX as f64 {
                        return Err("Stagger delay exceeds supported time range".into());
                    }
                    let offset = fold_foundation::Time::new(nanos.round() as i64, 1_000_000_000)
                        .and_then(|t| t.checked_scale(sample.index as i64, 1))
                        .map_err(|e| e.to_string())?;
                    let mut shifted = *sample;
                    shifted.time_offset = Some(
                        sample
                            .time_offset
                            .unwrap_or(fold_foundation::Time::ZERO)
                            .checked_add(offset)
                            .map_err(|e| e.to_string())?,
                    );
                    value.sample(&shifted, budget)
                },
            )))
        },
    );
    d.inputs[0].kind = Kind::Generic;
    d
}
