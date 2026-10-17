//! Immutable script-conversion preparation. Invoke on an authoring job worker;
//! publication still belongs to the host's guarded candidate/Apply lifecycle.
use crate::runtime::{Config, Inputs, Runtime};
use fold_foundation::{Failure, ObjectId, Time};
use fold_media::Cancel;
use fold_motion_canvas::{
    editing::{self, Edit},
    model::{Canvas, Content, ControlValue},
};
use fold_project::Document;
use std::{
    sync::{Mutex, TryLockError},
    time::Instant,
};

/// One retained capture process for a configured Motion provider. Source
/// changes use the existing compiler cache; controls remain runtime inputs.
pub struct Service {
    config: Config,
    fonts: Vec<Vec<u8>>,
    runtime: Mutex<Option<Runtime>>,
}
impl Service {
    pub fn new(config: Config, fonts: Vec<Vec<u8>>) -> Self {
        Self {
            config,
            fonts,
            runtime: Mutex::new(None),
        }
    }

    pub fn prepare_motion(
        &self,
        document: &Document,
        object: ObjectId,
        spec: fold_motion_canvas::motion::Spec,
        project: &fold_project::Snapshot,
        cancel: &Cancel,
    ) -> Result<Document, Failure> {
        cancel.check()?;
        spec.validate()?;
        let before = Canvas::from_document(document)?;
        if (fold_animation::seconds(before.duration) * f64::from(spec.sample_rate)).ceil()
            >= 18_000.
        {
            return Err(
                "Motion sampling exceeds 18000 keys per channel; reduce duration or sample rate"
                    .into(),
            );
        }
        editing::editable(&before, object)?;
        let expected = before
            .objects
            .get(&object)
            .ok_or("Missing motion target")?
            .clone();
        let mut owner = expected.clone();
        owner.motion = Some(fold_motion_canvas::motion::Motion {
            spec,
            enabled: true,
            curves: [0., 0., 0., 1., 1., 1.].map(|value| {
                let mut curve = fold_animation::Curve::default();
                curve.insert(Time::ZERO, value);
                curve
            }),
        });
        let draft = editing::apply(
            &before,
            &[Edit::Replace {
                id: object,
                expected: Box::new(expected.clone()),
                object: Box::new(owner.clone()),
            }],
        )?;
        let assets = crate::resources::bind(&draft, project)?;
        let mut runtime = self.lock_runtime(cancel)?;
        if runtime.is_none() {
            *runtime = Some(Runtime::new(self.config.clone(), cancel)?);
        }
        let mut curves = runtime.as_mut().ok_or("Missing motion runtime")?.animate(
            &draft,
            object,
            Inputs {
                fonts: &self.fonts,
                assets: &assets,
            },
            cancel,
        )?;
        // Constant spans need only their endpoints. Keep exact samples where
        // values change, without filling documents/MCP guards with static keys.
        for curve in &mut curves {
            let mut keys: Vec<fold_animation::Key> = Vec::new();
            for key in std::mem::take(&mut curve.keys) {
                if keys.len() >= 2
                    && keys[keys.len() - 2].value == key.value
                    && keys[keys.len() - 1].value == key.value
                {
                    keys.pop();
                }
                keys.push(key);
            }
            curve.keys = keys;
        }
        owner.motion.as_mut().unwrap().curves = curves;
        cancel.check()?;
        let after = editing::apply(
            &before,
            &[Edit::Replace {
                id: object,
                expected: Box::new(expected),
                object: Box::new(owner),
            }],
        )?;
        Ok(after.replace_document(document)?)
    }
    fn lock_runtime(
        &self,
        cancel: &Cancel,
    ) -> Result<std::sync::MutexGuard<'_, Option<Runtime>>, Failure> {
        let start = Instant::now();
        loop {
            cancel.check()?;
            match self.runtime.try_lock() {
                Ok(runtime) => return Ok(runtime),
                Err(TryLockError::Poisoned(_)) => {
                    return Err("Motion preparation service is unavailable".into());
                }
                Err(TryLockError::WouldBlock) => {
                    if start.elapsed() >= self.config.compile_timeout {
                        return Err("Motion preparation deadline exceeded".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        }
    }

    pub fn prepare(
        &self,
        document: &Document,
        object: ObjectId,
        time: Time,
        project: &fold_project::Snapshot,
        cancel: &Cancel,
    ) -> Result<Document, Failure> {
        cancel.check()?;
        let canvas = Canvas::from_document(document)?;
        let expected = canvas
            .objects
            .get(&object)
            .ok_or("Missing script object")?
            .clone();
        // Check protection before compiling, using the same baseline rules.
        editing::editable(&canvas, object)?;
        let Content::Script { controls, .. } = &expected.content else {
            return Err("Only script objects can be converted".into());
        };
        if controls.values().any(|control| {
            control.protected
                || matches!(&control.value, ControlValue::Number(value) if value.protected)
        }) {
            return Err("Script control is protected".into());
        }
        if time < Time::ZERO || time >= canvas.duration {
            return Err("Capture time is outside the document".into());
        }
        let assets = crate::resources::bind(&canvas, project)?;
        let mut runtime = self.lock_runtime(cancel)?;
        if runtime.is_none() {
            *runtime = Some(Runtime::new(self.config.clone(), cancel)?);
        }
        let captured = runtime.as_mut().ok_or("Missing capture runtime")?.capture(
            &canvas,
            object,
            time,
            Inputs {
                fonts: &self.fonts,
                assets: &assets,
            },
            cancel,
        )?;
        cancel.check()?;
        let after = editing::apply(
            &canvas,
            &[Edit::Capture {
                id: object,
                expected: Box::new(expected),
                value: Box::new(captured.artwork),
                time,
            }],
        )?;
        let prepared = after.replace_document(document)?;
        cancel.check()?;
        Ok(prepared)
    }
}
