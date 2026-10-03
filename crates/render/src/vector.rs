//! Immutable native 2D drawing contract. No authoring graph or feature models.
//! The legacy adapter retains 8-bit SDR rasterization for appearance compatibility.
//! The ACES path uses analytic triangle/pixel coverage and float authored colors.
//! Neither working color nor coverage is quantized to an SDR paint image.
use fold_media::Cancel;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Segment {
    Move([f64; 2]),
    Line([f64; 2]),
    Quad([f64; 2], [f64; 2]),
    Cubic([f64; 2], [f64; 2], [f64; 2]),
    Close,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Stroke {
    pub color: [f64; 4],
    pub width: f64,
    pub cap: Cap,
    pub join: Join,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Cap {
    Butt,
    Round,
    Square,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Join {
    Miter,
    Round,
    Bevel,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Drawing {
    pub path: Arc<Vec<Segment>>,
    /// Column-vector affine: [a, b, c, d, tx, ty].
    pub transform: [f64; 6],
    /// Straight scene-linear working RGB (legacy sRGB or explicit ACEScg);
    /// the adapter premultiplies once.
    pub fill: Option<[f64; 4]>,
    pub even_odd: bool,
    pub stroke: Option<Stroke>,
}
pub const MAX_DRAWINGS: usize = 16_384;
pub const MAX_SEGMENTS: usize = 262_144;

pub fn validate(drawings: &[Drawing]) -> Result<(), String> {
    validate_working(drawings, false)
}

pub fn validate_working(drawings: &[Drawing], aces: bool) -> Result<(), String> {
    if drawings.len() > MAX_DRAWINGS {
        return Err("vector drawing budget exceeded".into());
    }
    let mut segments = 0usize;
    for d in drawings {
        segments = segments
            .checked_add(d.path.len())
            .ok_or("vector segment overflow")?;
        if segments > MAX_SEGMENTS {
            return Err("vector segment budget exceeded".into());
        }
        if !d.transform.iter().all(|v| v.is_finite() && v.abs() <= 1e9) {
            return Err("nonfinite or excessive vector transform".into());
        }
        let color_valid = |c: &[f64; 4]| {
            c.iter().all(|v| v.is_finite() && (*v as f32).is_finite())
                && (0. ..=1.).contains(&c[3])
                && (aces || c[..3].iter().all(|v| (0. ..=1.).contains(v)))
        };
        if d.fill.as_ref().is_some_and(|c| !color_valid(c)) {
            return Err("invalid linear fill color".into());
        }
        if let Some(s) = &d.stroke
            && (!color_valid(&s.color)
                || !s.width.is_finite()
                || !(0. ..=10000.).contains(&s.width))
        {
            return Err("invalid vector stroke".into());
        }
        let mut started = false;
        for s in d.path.iter() {
            let points: &[[f64; 2]] = match s {
                Segment::Move(p) => {
                    started = true;
                    std::slice::from_ref(p)
                }
                Segment::Line(p) => std::slice::from_ref(p),
                Segment::Quad(_, _) | Segment::Cubic(_, _, _) | Segment::Close => &[],
            };
            let valid = |p: &[f64; 2]| p.iter().all(|v| v.is_finite() && v.abs() <= 1e9);
            if !started
                || !points.iter().all(valid)
                || match s {
                    Segment::Quad(a, b) => !valid(a) || !valid(b),
                    Segment::Cubic(a, b, c) => !valid(a) || !valid(b) || !valid(c),
                    _ => false,
                }
            {
                return Err("invalid vector path coordinates or missing Move".into());
            }
        }
    }
    Ok(())
}

pub(crate) fn build_path(drawing: &Drawing) -> Option<tiny_skia::Path> {
    let mut path = tiny_skia::PathBuilder::new();
    for segment in drawing.path.iter() {
        match *segment {
            Segment::Move([x, y]) => path.move_to(x as f32, y as f32),
            Segment::Line([x, y]) => path.line_to(x as f32, y as f32),
            Segment::Quad([x, y], [u, v]) => path.quad_to(x as f32, y as f32, u as f32, v as f32),
            Segment::Cubic([x, y], [u, v], [a, b]) => {
                path.cubic_to(x as f32, y as f32, u as f32, v as f32, a as f32, b as f32)
            }
            Segment::Close => path.close(),
        }
    }
    path.finish()
}

pub(crate) fn raster_stroke(stroke: &Stroke) -> tiny_skia::Stroke {
    tiny_skia::Stroke {
        width: stroke.width as f32,
        line_cap: match stroke.cap {
            Cap::Butt => tiny_skia::LineCap::Butt,
            Cap::Round => tiny_skia::LineCap::Round,
            Cap::Square => tiny_skia::LineCap::Square,
        },
        line_join: match stroke.join {
            Join::Miter => tiny_skia::LineJoin::Miter,
            Join::Round => tiny_skia::LineJoin::Round,
            Join::Bevel => tiny_skia::LineJoin::Bevel,
        },
        ..Default::default()
    }
}

pub(crate) fn fill_rule(drawing: &Drawing) -> tiny_skia::FillRule {
    if drawing.even_odd {
        tiny_skia::FillRule::EvenOdd
    } else {
        tiny_skia::FillRule::Winding
    }
}

pub(crate) fn rasterize(
    drawings: &[Drawing],
    width: u32,
    height: u32,
    cancel: &Cancel,
    aces: bool,
) -> Result<Vec<[f32; 4]>, String> {
    if aces {
        return crate::vector_geometry::rasterize(drawings, width, height, cancel);
    }
    validate_working(drawings, false)?;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or("vector surface allocation failed")?;
    for drawing in drawings {
        cancel.check()?;
        let Some(path) = build_path(drawing) else {
            continue;
        };
        let [a, b, c, d, x, y] = drawing.transform.map(|v| v as f32);
        let transform = tiny_skia::Transform::from_row(a, b, c, d, x, y);
        // A zero scale intentionally makes geometry invisible; it is valid animation.
        if transform.invert().is_none() {
            continue;
        }
        let paint = |color: [f64; 4]| {
            let mut paint = tiny_skia::Paint::default();
            let [r, g, b, a] = color.map(|v| v as f32);
            paint.set_color(tiny_skia::Color::from_rgba(r, g, b, a).expect("validated SDR color"));
            paint.anti_alias = true;
            paint
        };
        if let Some(color) = drawing.fill {
            pixmap.fill_path(&path, &paint(color), fill_rule(drawing), transform, None);
        }
        if let Some(stroke) = &drawing.stroke
            && stroke.width > 0.
        {
            pixmap.stroke_path(
                &path,
                &paint(stroke.color),
                &raster_stroke(stroke),
                transform,
                None,
            );
        }
    }
    cancel.check()?;
    Ok(pixmap
        .data()
        .chunks_exact(4)
        .map(|p| {
            [
                p[0] as f32 / 255.,
                p[1] as f32 / 255.,
                p[2] as f32 / 255.,
                p[3] as f32 / 255.,
            ]
        })
        .collect())
}

#[cfg(test)]
mod color_tests {
    use super::*;

    #[test]
    fn float_color_is_not_quantized_with_coverage_or_clamped() {
        let drawing = Drawing {
            path: Arc::new(vec![
                Segment::Move([0., 0.]),
                Segment::Line([1., 0.]),
                Segment::Line([1., 1.]),
                Segment::Line([0., 1.]),
                Segment::Close,
            ]),
            transform: [1., 0., 0., 1., 0., 0.],
            fill: Some([-0.123456, 2.34567, 0.333333, 0.123456]),
            even_odd: false,
            stroke: None,
        };
        assert!(validate(std::slice::from_ref(&drawing)).is_err());
        let pixels = rasterize(
            std::slice::from_ref(&drawing),
            1,
            1,
            &Cancel::default(),
            true,
        )
        .unwrap();
        let color = drawing.fill.unwrap().map(|v| v as f32);
        assert_eq!(
            pixels,
            vec![[
                color[0] * color[3],
                color[1] * color[3],
                color[2] * color[3],
                color[3]
            ]]
        );
        let pixels =
            rasterize(&[drawing.clone(), drawing], 1, 1, &Cancel::default(), true).unwrap();
        let alpha = color[3] + color[3] * (1. - color[3]);
        assert!((pixels[0][1] - color[1] * alpha).abs() < 1e-6);
        assert_eq!(pixels[0][3], alpha);
    }
}
