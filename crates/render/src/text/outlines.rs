//! Outline geometry derived from the retained shaped layout, without reshaping.
use super::Layout;
use crate::vector::{self, Drawing, Segment, Stroke};
use fold_media::Cancel;
use skrifa::{
    FontRef, GlyphId, MetadataProvider,
    instance::{LocationRef, NormalizedCoord, Size},
    outline::{DrawSettings, OutlinePen},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Budget,
    InvalidLayout,
    InvalidFont,
    UnsupportedGlyph { glyph: usize },
    Draw { glyph: usize, message: String },
}
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphOutline {
    /// Index into the retained layout's glyph array; cluster mappings remain there.
    pub glyph: usize,
    /// Unhinted local curves in font-size-scaled, Y-up coordinates.
    pub path: Arc<Vec<Segment>>,
    /// Local outline to the normalized layout's Y-down coordinate system.
    pub transform: [f64; 6],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Outlines {
    pub layout: Arc<Layout>,
    pub glyphs: Vec<GlyphOutline>,
}
impl Outlines {
    /// Vector drawing reuses the exact curves/placement exposed to outline consumers.
    /// Colors follow the existing Drawing scene-linear contract.
    pub fn drawings(&self, fill: Option<[f64; 4]>, stroke: Option<Stroke>) -> Vec<Drawing> {
        self.glyphs
            .iter()
            .map(|glyph| Drawing {
                path: glyph.path.clone(),
                transform: glyph.transform,
                fill,
                stroke: stroke.clone(),
                even_odd: false,
            })
            .collect()
    }
}
struct Pen<'a> {
    path: Vec<Segment>,
    remaining: &'a mut usize,
    checkpoint: &'a mut dyn FnMut() -> Result<(), Error>,
    error: Option<Error>,
}
impl Pen<'_> {
    fn push(&mut self, segment: Segment) {
        if self.error.is_some() {
            return;
        }
        if let Err(error) = (self.checkpoint)() {
            self.error = Some(error);
            return;
        }
        if *self.remaining == 0 {
            self.error = Some(Error::Budget);
            return;
        }
        *self.remaining -= 1;
        self.path.push(segment);
    }
}
impl OutlinePen for Pen<'_> {
    fn move_to(&mut self, x: f32, y: f32) {
        self.push(Segment::Move([x.into(), y.into()]));
    }
    fn line_to(&mut self, x: f32, y: f32) {
        self.push(Segment::Line([x.into(), y.into()]));
    }
    fn quad_to(&mut self, cx: f32, cy: f32, x: f32, y: f32) {
        self.push(Segment::Quad([cx.into(), cy.into()], [x.into(), y.into()]));
    }
    fn curve_to(&mut self, ax: f32, ay: f32, bx: f32, by: f32, x: f32, y: f32) {
        self.push(Segment::Cubic(
            [ax.into(), ay.into()],
            [bx.into(), by.into()],
            [x.into(), y.into()],
        ));
    }
    fn close(&mut self) {
        self.push(Segment::Close);
    }
}
/// Font and placement come exclusively from `layout`; no system lookup or second layout.
/// Color, bitmap-only and unresolved glyphs fail explicitly until qualified consumers exist.
pub fn prepare(layout: Arc<Layout>, cancel: &Cancel) -> Result<Outlines, Error> {
    prepare_with(layout, &mut || cancel.check().map_err(|_| Error::Cancelled))
}
/// Extract with the caller's cancellation and work-budget checkpoint. It runs at
/// entry, between runs/glyphs, for every emitted curve command, and before return.
/// On failure no partial outlines are published. Skrifa's current glyph draw may
/// finish internally, but its pen stops accepting commands after the first error.
pub fn prepare_with(
    layout: Arc<Layout>,
    checkpoint: &mut dyn FnMut() -> Result<(), Error>,
) -> Result<Outlines, Error> {
    checkpoint()?;
    if layout.glyphs.len() > vector::MAX_DRAWINGS {
        return Err(Error::Budget);
    }
    let mut remaining = vector::MAX_SEGMENTS;
    let mut cache = BTreeMap::new();
    let mut glyphs = vec![];
    let mut expanded_segments = 0usize;
    for run in &layout.runs {
        checkpoint()?;
        if !run.font_size.is_finite() || run.font_size <= 0. {
            return Err(Error::InvalidLayout);
        }
        let font =
            FontRef::from_index(&layout.font, run.face_index).map_err(|_| Error::InvalidFont)?;
        let outlines = font.outline_glyphs();
        let bitmaps = font.bitmap_strikes();
        if bitmaps.len() > 256 {
            return Err(Error::Budget);
        }
        let coords: Vec<_> = run
            .normalized_coords
            .iter()
            .map(|c| NormalizedCoord::from_bits(*c))
            .collect();
        for index in run.glyphs.clone() {
            checkpoint()?;
            if index != glyphs.len() {
                return Err(Error::InvalidLayout);
            }
            let glyph = layout.glyphs.get(index).ok_or(Error::InvalidLayout)?;
            let id = GlyphId::new(glyph.id);
            if glyph.id == 0
                || font.color_glyphs().get(id).is_some()
                || (0..bitmaps.len()).any(|i| {
                    bitmaps
                        .get(i)
                        .is_some_and(|strike| strike.get(id).is_some())
                })
            {
                return Err(Error::UnsupportedGlyph { glyph: index });
            }
            let key = (
                run.face_index,
                run.font_size.to_bits(),
                run.normalized_coords.clone(),
                glyph.id,
            );
            let path = if let Some(path) = cache.get(&key) {
                Arc::clone(path)
            } else {
                let outline = outlines
                    .get(id)
                    .ok_or(Error::UnsupportedGlyph { glyph: index })?;
                let mut pen = Pen {
                    path: vec![],
                    remaining: &mut remaining,
                    checkpoint,
                    error: None,
                };
                outline
                    .draw(
                        DrawSettings::unhinted(Size::new(run.font_size), LocationRef::new(&coords)),
                        &mut pen,
                    )
                    .map_err(|e| Error::Draw {
                        glyph: index,
                        message: e.to_string(),
                    })?;
                if let Some(error) = pen.error {
                    return Err(error);
                }
                let path = Arc::new(pen.path);
                cache.insert(key, path.clone());
                path
            };
            expanded_segments = expanded_segments
                .checked_add(path.len())
                .ok_or(Error::Budget)?;
            if expanded_segments > vector::MAX_SEGMENTS {
                return Err(Error::Budget);
            }
            glyphs.push(GlyphOutline {
                glyph: index,
                path,
                transform: [
                    1.,
                    0.,
                    0.,
                    -1.,
                    glyph.position[0].into(),
                    glyph.position[1].into(),
                ],
            });
        }
    }
    if glyphs.len() != layout.glyphs.len() {
        return Err(Error::InvalidLayout);
    }
    let result = Outlines { layout, glyphs };
    // Includes expanded (not just unique) segment count and finite path coordinates.
    vector::validate(&result.drawings(None, None)).map_err(|_| Error::InvalidLayout)?;
    checkpoint()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn layout(text: &str) -> Arc<Layout> {
        super::super::Service::new(
            include_bytes!("../../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
            "Noto Sans",
        )
        .unwrap()
        .layout(text, 140., None)
        .unwrap()
    }
    #[test]
    fn glyph_contours_and_vector_paths_share_one_layout() {
        let layout = layout("O B 8 O");
        let outlines = prepare(layout.clone(), &Cancel::default()).unwrap();
        assert!(Arc::ptr_eq(&layout, &outlines.layout));
        let drawings = outlines.drawings(Some([1., 0., 0., 1.]), None);
        vector::validate(&drawings).unwrap();
        let mut contours = vec![];
        for (index, (glyph, drawing)) in outlines.glyphs.iter().zip(&drawings).enumerate() {
            assert_eq!(glyph.glyph, index);
            assert!(Arc::ptr_eq(&glyph.path, &drawing.path));
            assert_eq!(glyph.transform, drawing.transform);
            assert_eq!(
                glyph.transform[4],
                f64::from(layout.glyphs[index].position[0])
            );
            assert_eq!(
                glyph.transform[5],
                f64::from(layout.glyphs[index].position[1])
            );
            let moves = glyph
                .path
                .iter()
                .filter(|s| matches!(s, Segment::Move(_)))
                .count();
            let closes = glyph
                .path
                .iter()
                .filter(|s| matches!(s, Segment::Close))
                .count();
            assert_eq!(moves, closes);
            contours.push(moves);
        }
        assert_eq!(contours, [2, 0, 3, 0, 3, 0, 2]);
        assert!(Arc::ptr_eq(
            &outlines.glyphs[0].path,
            &outlines.glyphs[6].path
        ));
        assert_ne!(outlines.glyphs[0].transform, outlines.glyphs[6].transform);
        assert!(
            outlines.glyphs[0]
                .path
                .iter()
                .any(|s| matches!(s, Segment::Quad(..) | Segment::Cubic(..)))
        );
    }
    #[test]
    fn ob8_holes_remain_transparent_in_cpu_vector_output() {
        let outlines = prepare(layout("O B 8"), &Cancel::default()).unwrap();
        let drawings = outlines.drawings(Some([1.; 4]), None);
        let pixels =
            crate::vector_geometry::rasterize(&drawings, 640, 220, &Cancel::default()).unwrap();
        assert!(pixels.iter().filter(|p| p[3] > 0.99).count() > 1000);
        let mut holes = 0;
        for glyph in &outlines.glyphs {
            let mut bounds = vec![];
            for contour in glyph.path.split_inclusive(|s| matches!(s, Segment::Close)) {
                let mut b = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for segment in contour {
                    let points: &[[f64; 2]] = match segment {
                        Segment::Move(p) | Segment::Line(p) => std::slice::from_ref(p),
                        Segment::Quad(_, p) | Segment::Cubic(_, _, p) => std::slice::from_ref(p),
                        Segment::Close => &[],
                    };
                    for p in points {
                        b[0] = b[0].min(p[0]);
                        b[1] = b[1].min(p[1]);
                        b[2] = b[2].max(p[0]);
                        b[3] = b[3].max(p[1]);
                    }
                }
                if b.iter().all(|v| v.is_finite()) {
                    bounds.push(b);
                }
            }
            bounds.sort_by(|a, b| {
                ((b[2] - b[0]) * (b[3] - b[1])).total_cmp(&((a[2] - a[0]) * (a[3] - a[1])))
            });
            for b in bounds.iter().skip(1) {
                let x = ((b[0] + b[2]) / 2. + glyph.transform[4]).floor() as usize;
                let y = (-(b[1] + b[3]) / 2. + glyph.transform[5]).floor() as usize;
                assert!(x < 640 && y < 220);
                assert_eq!(
                    pixels[y * 640 + x][3],
                    0.,
                    "filled hole in glyph {} at {x},{y}",
                    glyph.glyph
                );
                holes += 1;
            }
        }
        assert_eq!(holes, 5);
        if let Some(path) = std::env::var_os("FOLD_TEXT_CAPTURE_PATH") {
            let mut ppm = b"P6\n640 220\n255\n".to_vec();
            for pixel in pixels {
                for value in &pixel[..3] {
                    ppm.push((value.clamp(0., 1.) * 255.).round() as u8);
                }
            }
            std::fs::write(path, ppm).unwrap();
        }
    }
    #[test]
    fn caller_budget_and_cancellation_interrupt_curve_extraction() {
        let layout = layout("O B 8 O");
        for failure in [Error::Budget, Error::Cancelled] {
            let mut checkpoints = 0;
            let result = prepare_with(layout.clone(), &mut || {
                checkpoints += 1;
                if checkpoints == 12 {
                    Err(failure.clone())
                } else {
                    Ok(())
                }
            });
            assert_eq!(result, Err(failure));
            // No further callbacks or commands after a pen failure.
            assert_eq!(checkpoints, 12);
        }
        let mut checkpoints = 0;
        let result = prepare_with(layout.clone(), &mut || {
            checkpoints += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!(result, prepare(layout, &Cancel::default()).unwrap());
        assert!(checkpoints > result.glyphs.len());
    }
    #[test]
    fn caller_checkpoint_covers_empty_layout_and_cached_glyphs() {
        for wording in ["", "OOOO"] {
            let layout = layout(wording);
            let mut count = 0;
            prepare_with(layout.clone(), &mut || {
                count += 1;
                Ok(())
            })
            .unwrap();
            // A stop at every checkpoint, including the final one, must fail
            // without exposing a result, even when no new curves are extracted.
            for stop_at in 1..=count {
                let mut seen = 0;
                assert_eq!(
                    prepare_with(layout.clone(), &mut || {
                        seen += 1;
                        if seen == stop_at {
                            Err(Error::Cancelled)
                        } else {
                            Ok(())
                        }
                    }),
                    Err(Error::Cancelled)
                );
                assert_eq!(seen, stop_at);
            }
        }
    }
    #[test]
    fn unsupported_missing_glyphs_and_cancellation_do_not_publish_partial_paths() {
        let missing = layout("A🛸B");
        assert!(matches!(
            prepare(missing, &Cancel::default()),
            Err(Error::UnsupportedGlyph { .. })
        ));
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(matches!(
            prepare(layout("O B 8"), &cancel),
            Err(Error::Cancelled)
        ));
        let mut remaining = 1;
        let cancel = Cancel::default();
        let mut pen = Pen {
            path: vec![],
            remaining: &mut remaining,
            checkpoint: &mut || cancel.check().map_err(|_| Error::Cancelled),
            error: None,
        };
        pen.move_to(0., 0.);
        pen.line_to(1., 1.);
        pen.close();
        assert_eq!(pen.error, Some(Error::Budget));
        assert_eq!(pen.path.len(), 1);
    }
}
