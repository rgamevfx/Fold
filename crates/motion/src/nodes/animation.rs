use super::*;
use crate::{
    animation::{Interpolation, Key, Track},
    evaluation::field,
    fields::Field,
};
use fold_foundation::Time;
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Serialize, Deserialize, Default)]
pub enum Waveform {
    #[default]
    Sine,
    Triangle,
    Square,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct WaveSettings {
    pub waveform: Waveform,
}
pub fn definitions() -> Vec<Definition> {
    let mut wave = definition(
        "fold.motion.wave",
        "Wave",
        "Animation",
        vec![
            scalar("coordinate", 0., "cycles"),
            scalar("frequency", 1., ""),
            scalar("phase", 0., "cycles"),
            scalar("amplitude", 1., ""),
            scalar("offset", 0., ""),
        ],
        vec![("value", Kind::Scalar)],
        |e, n| {
            let shape = n.settings::<WaveSettings>()?.waveform;
            let coordinate = e.field(n, "coordinate")?;
            let frequency = e.field(n, "frequency")?;
            let phase = e.field(n, "phase")?;
            let amplitude = e.field(n, "amplitude")?;
            let offset = e.field(n, "offset")?;
            let varying = [&coordinate, &frequency, &phase, &amplitude, &offset]
                .iter()
                .any(|f| f.varying);
            Ok(field(Field::new(Kind::Scalar, varying, move |s, b| {
                let t = coordinate.sample(s, b)?.scalar()? * frequency.sample(s, b)?.scalar()?
                    + phase.sample(s, b)?.scalar()?;
                let value = match shape {
                    Waveform::Sine => (t * std::f64::consts::TAU).sin(),
                    Waveform::Triangle => 1. - 4. * (t.rem_euclid(1.) - 0.5).abs(),
                    Waveform::Square => {
                        if t.rem_euclid(1.) < 0.5 {
                            1.
                        } else {
                            -1.
                        }
                    }
                };
                Ok(Datum::Scalar(
                    offset.sample(s, b)?.scalar()? + amplitude.sample(s, b)?.scalar()? * value,
                ))
            })))
        },
    );
    wave.defaults = || serde_json::to_value(WaveSettings::default()).unwrap();
    wave.validate = |n| n.settings::<WaveSettings>().map(|_| ());
    let mut keys = definition(
        "fold.motion.keyframes",
        "Keyframes",
        "Animation",
        vec![],
        vec![("value", Kind::Scalar)],
        |e, n| {
            Ok(field(Field::constant(
                n.settings::<Track>()?.sample(e.time)?,
            )))
        },
    );
    keys.outputs[0].1 = Kind::Generic;
    keys.defaults = || {
        serde_json::to_value(Track {
            channels: vec![],
            keys: vec![Key {
                time: Time::ZERO,
                value: Datum::Scalar(0.),
                interpolation: Interpolation::Linear,
            }],
        })
        .unwrap()
    };
    keys.validate = |n| n.settings::<Track>()?.validate();
    vec![
        definition(
            "fold.motion.time",
            "Time",
            "Animation",
            vec![],
            vec![("value", Kind::Scalar)],
            |e, _| {
                Ok(field(Field::constant(Datum::Scalar(
                    e.time.numerator() as f64 / e.time.denominator() as f64,
                ))))
            },
        ),
        wave,
        keys,
    ]
}
