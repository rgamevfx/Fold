//! Deterministic shaping with embedded font bytes, never ambient system fallback.
//! Graphite's text-to-path pipeline is the architectural reference (see docs).
//! This adapter uses Rustybuzz + Unicode bidi, retaining glyph cluster offsets.
use crate::{
    document::Font,
    geometry::{Content, Element, Geometry, IDENTITY, derived_id},
};
use fold_render::vector::Segment;
use rustybuzz::{
    Direction, Face, UnicodeBuffer,
    ttf_parser::{GlyphId, OutlineBuilder},
};
use std::sync::Arc;
#[derive(Default)]
struct Outline {
    segments: Vec<Segment>,
    scale: f64,
}
impl OutlineBuilder for Outline {
    fn move_to(&mut self, x: f32, y: f32) {
        self.segments.push(Segment::Move([
            x as f64 * self.scale,
            -(y as f64) * self.scale,
        ]));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.segments.push(Segment::Line([
            x as f64 * self.scale,
            -(y as f64) * self.scale,
        ]));
    }
    fn quad_to(&mut self, x: f32, y: f32, u: f32, v: f32) {
        self.segments.push(Segment::Quad(
            [x as f64 * self.scale, -(y as f64) * self.scale],
            [u as f64 * self.scale, -(v as f64) * self.scale],
        ));
    }
    fn curve_to(&mut self, x: f32, y: f32, u: f32, v: f32, a: f32, b: f32) {
        self.segments.push(Segment::Cubic(
            [x as f64 * self.scale, -(y as f64) * self.scale],
            [u as f64 * self.scale, -(v as f64) * self.scale],
            [a as f64 * self.scale, -(b as f64) * self.scale],
        ));
    }
    fn close(&mut self) {
        self.segments.push(Segment::Close);
    }
}
pub fn shape(
    text: &str,
    font: &Font,
    size: f64,
    line_spacing: f64,
    alignment: f64,
    seed: u64,
) -> Result<Content, String> {
    if text.len() > 65536
        || !size.is_finite()
        || !(0. ..=10000.).contains(&size)
        || !line_spacing.is_finite()
        || line_spacing <= 0.
        || !(0. ..=1.).contains(&alignment)
    {
        return Err("invalid text layout parameters".into());
    }
    let face =
        Face::from_slice(font.bytes(), font.face_index).ok_or("invalid embedded font face")?;
    let scale = size / f64::from(face.units_per_em());
    let mut elements = Vec::new();
    let mut byte_offset = 0usize;
    let mut segments = 0usize;
    for (line_index, line) in text.split('\n').enumerate() {
        let info = unicode_bidi::BidiInfo::new(line, None);
        let mut x = 0.;
        let start = elements.len();
        for paragraph in &info.paragraphs {
            let (_, runs) = info.visual_runs(paragraph, paragraph.range.clone());
            for run in runs {
                let rtl = info.levels[run.start].is_rtl();
                let mut buffer = UnicodeBuffer::new();
                buffer.push_str(&line[run.clone()]);
                buffer.guess_segment_properties();
                buffer.set_direction(if rtl {
                    Direction::RightToLeft
                } else {
                    Direction::LeftToRight
                });
                let glyphs = rustybuzz::shape(&face, &[], buffer);
                for (ordinal, (glyph, position)) in glyphs
                    .glyph_infos()
                    .iter()
                    .zip(glyphs.glyph_positions())
                    .enumerate()
                {
                    if glyph.glyph_id == 0 {
                        return Err(format!(
                            "font '{}' has no glyph for text cluster {}",
                            font.name,
                            byte_offset + run.start + glyph.cluster as usize
                        ));
                    }
                    if elements.len() >= 16384 {
                        return Err("text exceeds 16384 glyphs".into());
                    }
                    let mut outline = Outline {
                        scale,
                        ..Default::default()
                    };
                    face.outline_glyph(GlyphId(glyph.glyph_id as u16), &mut outline);
                    segments += outline.segments.len();
                    if segments > fold_render::vector::MAX_SEGMENTS {
                        return Err("text outline budget exceeded".into());
                    }
                    let cluster = (byte_offset + run.start + glyph.cluster as usize) as u32;
                    // Identity survives property/time edits. Editing the text can
                    // change cluster correspondence; no false topology guarantee.
                    let mut element = Element::new(
                        derived_id(seed, ((cluster as u64) << 32) | ordinal as u64),
                        Geometry::Glyph {
                            path: Arc::new(outline.segments),
                            cluster,
                        },
                    );
                    element.transform = IDENTITY;
                    element.transform[4] = x + position.x_offset as f64 * scale;
                    element.transform[5] = size + line_index as f64 * size * line_spacing
                        - position.y_offset as f64 * scale;
                    x += position.x_advance as f64 * scale;
                    elements.push(element);
                }
            }
        }
        for glyph in &mut elements[start..] {
            glyph.transform[4] -= x * alignment;
        }
        byte_offset += line.len() + 1;
    }
    Ok(Arc::new(elements))
}
