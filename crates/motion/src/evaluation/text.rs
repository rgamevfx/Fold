//! Explicit packaged text resources and retained shaping. No system discovery or file I/O.
use super::{FailureKind, Geometry, GeometryData, Scope, State, Value, style::Appearance};
use crate::types::{Literal, Space, Unit};
use fold_foundation::ObjectId;
use fold_render::text::{FontSource, Layout, Service};
use std::{
    collections::BTreeMap,
    sync::{Arc, OnceLock},
};

pub const FONT_NAME: &str = "Noto Sans Regular";
pub const FONT_RESOURCE: &str = "fold.font.noto_sans.regular";
pub const FONT_FAMILY: &str = "Noto Sans";
/// The version-one packaged resource must not silently change across builds.
pub const FONT_FINGERPRINT: &str =
    "sha256:b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5";

#[derive(Clone, Debug, PartialEq)]
pub struct Text {
    pub resource: &'static str,
    pub layout: Arc<Layout>,
    /// IDs name glyph/index, cluster/word index/range and line/index slots. They remain
    /// stable while that correspondence survives changes in metrics or time.
    /// Arbitrary insertion/deletion correspondence is not inferred from these IDs.
    pub glyph_ids: Vec<u64>,
    pub cluster_ids: Vec<u64>,
    pub line_ids: Vec<u64>,
    pub word_ids: Vec<u64>,
    /// Evaluated canvas translations, separate from the immutable shaped layout.
    pub offsets: Vec<[f64; 2]>,
}
impl Text {
    pub fn layout_bounds(&self) -> [[f64; 2]; 2] {
        // Conservative envelope of the original layout and its glyph offsets;
        // this preserves the existing layout-box policy, never claims ink bounds.
        let mut low = [0f64; 2];
        let mut high = [0f64; 2];
        for offset in &self.offsets {
            for axis in 0..2 {
                low[axis] = low[axis].min(offset[axis]);
                high[axis] = high[axis].max(offset[axis]);
            }
        }
        [
            low,
            [
                f64::from(self.layout.width) + high[0],
                f64::from(self.layout.height) + high[1],
            ],
        ]
    }
    pub fn bounds_kind(&self) -> &'static str {
        if self.offsets.iter().any(|offset| *offset != [0.; 2]) {
            "layout_envelope"
        } else {
            "layout"
        }
    }
    pub fn outlines_with(
        &self,
        checkpoint: &mut dyn FnMut() -> Result<(), fold_render::text::outlines::Error>,
    ) -> Result<fold_render::text::outlines::Outlines, fold_render::text::outlines::Error> {
        if self.offsets.len() != self.layout.glyphs.len()
            || self
                .offsets
                .iter()
                .flatten()
                .any(|v| !v.is_finite() || v.abs() > 1e9)
        {
            return Err(fold_render::text::outlines::Error::InvalidLayout);
        }
        let mut curves =
            fold_render::text::outlines::prepare_with(self.layout.clone(), checkpoint)?;
        for glyph in &mut curves.glyphs {
            checkpoint()?;
            glyph.transform[4] += self.offsets[glyph.glyph][0];
            glyph.transform[5] += self.offsets[glyph.glyph][1];
        }
        Ok(curves)
    }
}
fn font() -> Result<FontSource, String> {
    static FONT: OnceLock<Result<FontSource, String>> = OnceLock::new();
    FONT.get_or_init(|| {
        let font = FontSource::from_bytes(
            include_bytes!("../../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
        )
        .map_err(|e| format!("Packaged font unavailable: {e:?}"))?;
        if font.fingerprint() != FONT_FINGERPRINT {
            return Err(
                "Packaged font content does not match the version-one resource fingerprint".into(),
            );
        }
        Ok(font)
    })
    .clone()
}
impl State<'_> {
    pub(super) fn text_geometry(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let primitive = |name| {
            args.get(name)
                .ok_or_else(|| format!("Missing {name}"))?
                .primitive()
        };
        let Literal::Text { value: font_name } = primitive("font")? else {
            return Err("Expected packaged font name".into());
        };
        if font_name != FONT_NAME {
            return Err(format!("Unsupported packaged font: {font_name}"));
        }
        let Literal::Text { value: wording } = primitive("text")? else {
            return Err("Expected text".into());
        };
        let length = |name| match primitive(name)? {
            Literal::Distance {
                value,
                unit: Unit::Canvas,
            } if value.is_finite() && *value > 0. && *value <= 10000. => Ok(*value as f32),
            _ => Err(format!("{name} must be in (0,10000] canvas units")),
        };
        let size = length("size")?;
        let Literal::Bool { value: wrap } = primitive("wrap")? else {
            return Err("Expected wrap choice".into());
        };
        let width = length("width")?;
        for _ in wording.bytes() {
            self.budget.step()?;
        }
        if self.text.is_none() {
            self.text = Some(
                Service::from_font(font()?, FONT_FAMILY)
                    .map_err(|e| format!("Packaged font failed: {e:?}"))?,
            );
        }
        self.budget.step()?;
        let result = self
            .text
            .as_mut()
            .unwrap()
            .layout(wording, size, wrap.then_some(width));
        self.budget.step()?;
        let layout = result.map_err(|e| {
            if e == fold_render::text::Error::Budget {
                self.failure = Some(FailureKind::Budget);
            }
            format!("Text layout failed: {e:?}")
        })?;
        self.reserve(
            1 + layout.glyphs.len()
                + layout.clusters.len()
                + layout.line_count
                + layout.words.len(),
        )?;
        let geometry_id = self.identity(scope, id, 0, b"geometry")?;
        let mut glyph_ids = Vec::with_capacity(layout.glyphs.len());
        for (index, glyph) in layout.glyphs.iter().enumerate() {
            glyph_ids.push(self.identity(
                scope,
                id,
                geometry_id,
                format!("glyph:{index}:{}", glyph.id).as_bytes(),
            )?);
        }
        let mut cluster_ids = Vec::with_capacity(layout.clusters.len());
        for (index, cluster) in layout.clusters.iter().enumerate() {
            cluster_ids.push(
                self.identity(
                    scope,
                    id,
                    geometry_id,
                    format!(
                        "cluster:{index}:{}:{}",
                        cluster.text.start, cluster.text.end
                    )
                    .as_bytes(),
                )?,
            );
        }
        let mut line_ids = Vec::with_capacity(layout.line_count);
        for index in 0..layout.line_count {
            line_ids.push(self.identity(
                scope,
                id,
                geometry_id,
                format!("line:{index}").as_bytes(),
            )?);
        }
        let mut word_ids = Vec::with_capacity(layout.words.len());
        for (index, word) in layout.words.iter().enumerate() {
            word_ids.push(self.identity(
                scope,
                id,
                geometry_id,
                format!("word:{index}:{}:{}", word.text.start, word.text.end).as_bytes(),
            )?);
        }
        Ok(Value::Geometry(Arc::new(Geometry {
            id: geometry_id,
            space: Space::Screen,
            unit: Unit::Canvas,
            data: GeometryData::Text(Arc::new(Text {
                resource: FONT_RESOURCE,
                offsets: vec![[0.; 2]; layout.glyphs.len()],
                layout,
                glyph_ids,
                cluster_ids,
                line_ids,
                word_ids,
            })),
            appearance: Appearance::default(),
        })))
    }
}
