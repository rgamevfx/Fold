//! Instance-duration timing and reusable normalized curve mapping.
use super::*;
use crate::{evaluation::Value, fields::Field};
use fold_animation::timing::{Behavior, Curve, Range};
use fold_foundation::Time;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    /// None inherits the explicit owning instance/object/document range.
    #[serde(default)]
    pub range: Option<Range>,
    #[serde(default)]
    pub behavior: Behavior,
    #[serde(flatten)]
    pub curve: Curve,
}
impl Settings {
    pub fn validate(&self) -> Result<(), String> {
        self.curve.validate()?;
        if let Some(range) = self.range {
            range.validate()?;
        }
        Ok(())
    }
}

pub fn definitions() -> Vec<Definition> {
    let mut timing = definition(
        "fold.motion.timing",
        "Timing",
        "Animation",
        vec![],
        vec![
            ("progress", Kind::Scalar),
            ("value", Kind::Scalar),
            ("elapsed", Kind::Scalar),
            ("duration", Kind::Scalar),
        ],
        |e, n| {
            let settings = Arc::new(n.settings::<Settings>()?);
            settings.validate()?;
            let range = match settings.range {
                Some(range) => range,
                None => e.timing_range()?,
            };
            let time = e.time;
            // Ordinary uniform sampling reuses this result across all consumers.
            // Stagger can explicitly request a shifted sample of the same field.
            let initial = range.sample(time, settings.behavior)?;
            let shaped = settings.curve.sample(initial.progress);
            let mut outputs = Outputs::new();
            for (socket, index) in [
                ("progress", 0),
                ("value", 1),
                ("elapsed", 2),
                ("duration", 3),
            ] {
                let settings = settings.clone();
                outputs.insert(
                    socket.into(),
                    Value::Field(Field::new(Kind::Scalar, false, move |sample, _| {
                        let (sample, value) =
                            if let Some(offset) = sample.time_offset.filter(|t| *t != Time::ZERO) {
                                let sample = range.sample(
                                    time.checked_sub(offset).map_err(|e| e.to_string())?,
                                    settings.behavior,
                                )?;
                                (sample, settings.curve.sample(sample.progress))
                            } else {
                                (initial, shaped)
                            };
                        Ok(Datum::Scalar(
                            [sample.progress, value, sample.elapsed, sample.duration][index],
                        ))
                    })),
                );
            }
            Ok(outputs)
        },
    );
    timing.description = "Fit progress and an editable curve to the owning instance duration; optional exact range override. Does not retime media.";
    timing.defaults = || serde_json::to_value(Settings::default()).unwrap();
    timing.validate = |n| n.settings::<Settings>()?.validate();
    timing.atomic_settings = &["range", "curve"];
    let mut curve = definition(
        "fold.motion.curve",
        "Curve",
        "Animation",
        vec![scalar("progress", 0., "normalized")],
        vec![("value", Kind::Scalar)],
        |e, n| {
            let curve = n.settings::<Curve>()?;
            curve.validate()?;
            Ok(crate::evaluation::field(
                e.field(n, "progress")?.map(Kind::Scalar, move |v| {
                    Ok(Datum::Scalar(curve.sample(v.scalar()?)))
                }),
            ))
        },
    );
    curve.description = "Map normalized progress through an editable curve. Endpoints clamp; compose with Timing or Oscillator.";
    curve.defaults = || serde_json::to_value(Curve::default()).unwrap();
    curve.validate = |n| n.settings::<Curve>()?.validate();
    curve.atomic_settings = &["curve"];
    let mut segments = definition(
        "fold.motion.timing_segments",
        "Timing Segments",
        "Animation",
        vec![
            scalar("elapsed", 0., "s"),
            scalar("duration", 1., "s"),
            scalar("intro", 0.25, "s"),
            scalar("outro", 0.25, "s"),
        ],
        vec![
            ("intro", Kind::Scalar),
            ("hold", Kind::Scalar),
            ("outro", Kind::Scalar),
        ],
        |e, n| {
            let fields = Arc::new([
                e.field(n, "elapsed")?,
                e.field(n, "duration")?,
                e.field(n, "intro")?,
                e.field(n, "outro")?,
            ]);
            let varying = fields.iter().any(|f| f.varying);
            let mut outputs = Outputs::new();
            for (index, name) in ["intro", "hold", "outro"].iter().enumerate() {
                let fields = fields.clone();
                outputs.insert(
                    (*name).into(),
                    Value::Field(Field::new(Kind::Scalar, varying, move |sample, budget| {
                        let mut values = [0.; 4];
                        for (i, field) in fields.iter().enumerate() {
                            values[i] = field.sample(sample, budget)?.scalar()?;
                        }
                        Ok(Datum::Scalar(
                            fold_animation::timing::segments(
                                values[0], values[1], values[2], values[3],
                            )?[index],
                        ))
                    })),
                );
            }
            Ok(outputs)
        },
    );
    segments.description = "Connect Timing Elapsed and Duration. Fixed intro/outro seconds surround a flexible hold; short durations compress both proportionally. Shape each phase with Curve and combine with arithmetic.";
    vec![timing, curve, segments]
}
