//! Word-local signal sampling over a retained shaped layout.
use super::{Geometry, GeometryData, Scope, State, Value};
use crate::{
    binding::Binding,
    types::{Literal, Space, Unit},
};
use fold_foundation::{ObjectId, Time};
use std::{collections::BTreeMap, sync::Arc};

impl State<'_> {
    pub(super) fn animate_words(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
        position: &Binding,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(geometry)) = args.get("text") else {
            return Err("Text Animator requires Text geometry".into());
        };
        let GeometryData::Text(source) = &geometry.data else {
            return Err("Text Animator requires retained Text output".into());
        };
        let Literal::Duration { value: stagger } = args
            .get("stagger")
            .ok_or("Missing word delay")?
            .primitive()?
        else {
            return Err("Word delay requires duration".into());
        };
        if *stagger < Time::ZERO {
            return Err("Word delay must be nonnegative".into());
        }
        self.reserve(1 + source.offsets.len())?;
        let mut text = source.as_ref().clone();
        let mut sample_time = scope.time;
        for (index, word) in source.layout.words.iter().enumerate() {
            self.budget.step()?;
            let sampled =
                self.sample_binding(scope, id, position, sample_time, self.element.clone())?;
            let Literal::Vector {
                value,
                unit: Unit::Canvas,
                space: Space::Screen,
            } = sampled.primitive()?
            else {
                return Err("Word position requires a Screen canvas vector".into());
            };
            if value.len() != 2 || value.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
                return Err("Word position exceeds supported range".into());
            }
            for &glyph in &word.glyphs {
                self.budget.step()?;
                for (axis, delta) in value.iter().enumerate() {
                    text.offsets[glyph][axis] += delta;
                    if text.offsets[glyph][axis].abs() > 1e9 {
                        return Err("Combined word position exceeds supported range".into());
                    }
                }
            }
            if index + 1 < source.layout.words.len() {
                sample_time = sample_time
                    .checked_sub(*stagger)
                    .map_err(|e| e.to_string())?;
            }
        }
        Ok(Value::Geometry(Arc::new(Geometry {
            attributes: Default::default(),
            id: self.identity(scope, id, geometry.id, b"word_position")?,
            space: geometry.space,
            unit: geometry.unit,
            data: GeometryData::Text(Arc::new(text)),
            appearance: geometry.appearance.clone(),
        })))
    }
}
