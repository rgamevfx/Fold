//! Immutable native 2D drawing contract. No authoring graph or feature models.
//! The reference adapter rasterizes linear-SDR colors with 8-bit coverage/color
//! precision, then expands to the engine's premultiplied RGBA32F working frame.
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
#[derive(Clone, Debug)]
pub struct Drawing {
    pub path: Arc<Vec<Segment>>,
    /// Column-vector affine: [a, b, c, d, tx, ty].
    pub transform: [f64; 6],
    /// Straight scene-linear sRGB; the adapter premultiplies once.
    pub fill: Option<[f64; 4]>,
    pub even_odd: bool,
    pub stroke: Option<Stroke>,
}
pub const MAX_DRAWINGS: usize = 16_384;
pub const MAX_SEGMENTS: usize = 262_144;

pub fn validate(drawings: &[Drawing]) -> Result<(), String> {
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
        let color_valid = |c: &[f64; 4]| c.iter().all(|v| v.is_finite() && (0. ..=1.).contains(v));
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

pub(crate) fn rasterize(
    drawings: &[Drawing],
    width: u32,
    height: u32,
    cancel: &Cancel,
) -> Result<Vec<[f32; 4]>, String> {
    validate(drawings)?;
    let mut pixmap =
        tiny_skia::Pixmap::new(width, height).ok_or("vector surface allocation failed")?;
    for drawing in drawings {
        cancel.check()?;
        let mut path = tiny_skia::PathBuilder::new();
        for segment in drawing.path.iter() {
            match *segment {
                Segment::Move([x, y]) => path.move_to(x as f32, y as f32),
                Segment::Line([x, y]) => path.line_to(x as f32, y as f32),
                Segment::Quad([x, y], [u, v]) => {
                    path.quad_to(x as f32, y as f32, u as f32, v as f32)
                }
                Segment::Cubic([x, y], [u, v], [a, b]) => {
                    path.cubic_to(x as f32, y as f32, u as f32, v as f32, a as f32, b as f32)
                }
                Segment::Close => path.close(),
            }
        }
        let Some(path) = path.finish() else {
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
            pixmap.fill_path(
                &path,
                &paint(color),
                if drawing.even_odd {
                    tiny_skia::FillRule::EvenOdd
                } else {
                    tiny_skia::FillRule::Winding
                },
                transform,
                None,
            );
        }
        if let Some(stroke) = &drawing.stroke
            && stroke.width > 0.
        {
            pixmap.stroke_path(
                &path,
                &paint(stroke.color),
                &tiny_skia::Stroke {
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
                },
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
