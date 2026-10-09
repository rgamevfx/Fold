//! Curves from one retained text layout, with source and topology correspondence.
use super::{FailureKind, Geometry, GeometryData, Scope, State, Value, text::Text};
use crate::execution::Stop;
use fold_foundation::ObjectId;
use fold_render::{text::outlines, vector::Segment};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphTopology {
    pub source_glyph: u64,
    pub contour_ids: Vec<u64>,
    /// One ID per retained path command, including moves and closes.
    pub command_ids: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TextOutlines {
    pub source: Arc<Text>,
    pub curves: Arc<outlines::Outlines>,
    pub topology: Vec<GlyphTopology>,
}
impl State<'_> {
    pub(super) fn outline_text(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(geometry)) = args.get("text") else {
            return Err("Outline Text requires text geometry".into());
        };
        let GeometryData::Text(text) = &geometry.data else {
            return Err("Outline Text requires retained Text output".into());
        };
        let curves = text
            .outlines_with(&mut || {
                self.budget.step().map_err(|_| match self.budget.stop() {
                    Some(Stop::Cancelled) => outlines::Error::Cancelled,
                    _ => outlines::Error::Budget,
                })
            })
            .map_err(|error| {
                match error {
                    outlines::Error::Budget => self.failure = Some(FailureKind::Budget),
                    outlines::Error::Cancelled => self.failure = Some(FailureKind::Cancelled),
                    _ => {}
                }
                format!("Outline Text failed: {error:?}")
            })?;
        let mut topology = Vec::with_capacity(curves.glyphs.len());
        self.reserve(1 + curves.glyphs.len())?;
        for glyph in &curves.glyphs {
            let source_glyph = text.glyph_ids[glyph.glyph];
            self.reserve(glyph.path.len())?;
            let mut contour_ids = vec![];
            let mut command_ids = Vec::with_capacity(glyph.path.len());
            for (index, command) in glyph.path.iter().enumerate() {
                if matches!(command, Segment::Move(_)) {
                    self.reserve(1)?;
                    contour_ids.push(self.identity(
                        scope,
                        id,
                        source_glyph,
                        format!("contour:{}", contour_ids.len()).as_bytes(),
                    )?);
                }
                command_ids.push(self.identity(
                    scope,
                    id,
                    source_glyph,
                    format!("command:{index}").as_bytes(),
                )?);
            }
            topology.push(GlyphTopology {
                source_glyph,
                contour_ids,
                command_ids,
            });
        }
        Ok(Value::Geometry(Arc::new(Geometry {
            attributes: Default::default(),
            id: self.identity(scope, id, geometry.id, b"outlines")?,
            space: geometry.space,
            unit: geometry.unit,
            data: GeometryData::Outlines(Arc::new(TextOutlines {
                source: text.clone(),
                curves: Arc::new(curves),
                topology,
            })),
            appearance: geometry.appearance.clone(),
        })))
    }
}
