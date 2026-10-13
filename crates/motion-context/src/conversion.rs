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
        let start = Instant::now();
        let mut runtime = loop {
            cancel.check()?;
            match self.runtime.try_lock() {
                Ok(runtime) => break runtime,
                Err(TryLockError::Poisoned(_)) => {
                    return Err("Motion capture service is unavailable".into());
                }
                Err(TryLockError::WouldBlock) => {
                    if start.elapsed() >= self.config.compile_timeout {
                        return Err("Motion capture preparation deadline exceeded".into());
                    }
                    std::thread::sleep(std::time::Duration::from_millis(10));
                }
            }
        };
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
