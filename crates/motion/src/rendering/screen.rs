//! Ordered Screen artwork in output-pixel coordinates, retaining native curves.
use super::{Error, Evaluated, Limits, conversion_step as step, evaluate};
use crate::{evaluation, execution::Budget};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{Revision, Snapshot};
use fold_render::{
    text::outlines,
    vector::{self, Drawing, Segment},
};
use std::{collections::BTreeMap, sync::Arc};

pub struct ScreenFrame {
    pub document: DocumentId,
    pub revision: Revision,
    pub output: ObjectId,
    pub time: Time,
    pub duration: Time,
    pub dimensions: [u32; 2],
    pub native: Arc<evaluation::Scene>,
    /// Straight linear Rec.709 paint; consumers must explicitly convert working
    /// space when needed. Placement opacity multiplies alpha exactly once here.
    pub drawings: Arc<Vec<Drawing>>,
}
type Path = (Arc<Vec<Segment>>, [f64; 6]);

/// Prepare a Screen-only scene. World entries fail explicitly; mixed-pass
/// composition must preserve their ordering instead of silently dropping them.
pub fn screen(
    snapshot: &Snapshot,
    document: DocumentId,
    output: ObjectId,
    time: Time,
    dimensions: [u32; 2],
    limits: Limits,
    cancellation: &dyn Fn() -> Result<(), String>,
) -> Result<ScreenFrame, Error> {
    let evaluated = evaluate(
        snapshot,
        document,
        output,
        time,
        dimensions,
        limits.evaluation,
        super::Services {
            cancellation,
            images: None,
        },
    )?;
    let mut budget = Budget::new(limits.conversion_steps, cancellation);
    let drawings = prepare(&evaluated, dimensions, &mut budget)?;
    Ok(ScreenFrame {
        document,
        revision: snapshot.revision(),
        output,
        time,
        duration: evaluated.duration,
        dimensions,
        native: evaluated.native,
        drawings,
    })
}
pub(super) fn prepare(
    evaluated: &Evaluated,
    dimensions: [u32; 2],
    budget: &mut Budget<'_>,
) -> Result<Arc<Vec<Drawing>>, Error> {
    let mut paths = BTreeMap::<u64, Vec<Path>>::new();
    let mut drawings = vec![];
    let scale = [0, 1].map(|i| f64::from(dimensions[i]) / f64::from(evaluated.canvas[i]));
    let mut segments = 0usize;
    for entry in &evaluated.native.entries {
        step(budget)?;
        let placement = &entry.placement;
        let evaluation::placement::Transform::Screen { position } = placement.transform else {
            return Err(Error::Invalid(
                "World entries require mixed-pass rendering".into(),
            ));
        };
        let geometry = &placement.geometry;
        if let std::collections::btree_map::Entry::Vacant(slot) = paths.entry(geometry.id) {
            slot.insert(geometry_paths(&geometry.data, &mut || step(budget))?);
        }
        let appearance = &geometry.appearance;
        let paint = |mut color: [f64; 4]| {
            color[3] *= placement.opacity;
            color
        };
        let fill = appearance.fill.as_ref().map(|fill| paint(fill.color.value));
        let stroke = appearance.stroke.as_ref().map(|stroke| vector::Stroke {
            color: paint(stroke.color.value),
            width: stroke.width,
            cap: stroke.mode.cap(),
            join: stroke.mode.join(),
        });
        for (path, transform) in &paths[&geometry.id] {
            step(budget)?;
            segments = segments.saturating_add(path.len());
            if drawings.len() == vector::MAX_DRAWINGS || segments > vector::MAX_SEGMENTS {
                return Err(Error::Budget);
            }
            // Charge validation of shared paths for every drawing occurrence.
            for _ in path.iter() {
                step(budget)?;
            }
            let [a, b, c, d, tx, ty] = *transform;
            drawings.push(Drawing {
                feather: None,
                path: path.clone(),
                transform: [
                    a * scale[0],
                    b * scale[1],
                    c * scale[0],
                    d * scale[1],
                    (tx + position[0]) * scale[0],
                    (ty + position[1]) * scale[1],
                ],
                fill,
                stroke: stroke.clone(),
                even_odd: appearance.fill.as_ref().is_some_and(|fill| fill.even_odd),
            });
        }
    }
    // This checks the float/HDR contract without performing a color conversion.
    // `true` permits signed HDR components; our declared space remains Rec.709.
    vector::validate_working(&drawings, true).map_err(Error::Invalid)?;
    step(budget)?;
    Ok(Arc::new(drawings))
}

pub(super) fn geometry_paths(
    data: &evaluation::GeometryData,
    checkpoint: &mut dyn FnMut() -> Result<(), Error>,
) -> Result<Vec<Path>, Error> {
    let curves = match data {
        evaluation::GeometryData::Contours(contours) => {
            // Keep contours together so winding and holes apply to the collection.
            let mut path = vec![];
            for contour in contours.iter() {
                checkpoint()?;
                for (index, point) in contour.points.iter().enumerate() {
                    checkpoint()?;
                    if path.len() == vector::MAX_SEGMENTS {
                        return Err(Error::Budget);
                    }
                    path.push(if index == 0 {
                        Segment::Move(point.position)
                    } else {
                        Segment::Line(point.position)
                    });
                }
                if contour.closed && !contour.points.is_empty() {
                    checkpoint()?;
                    if path.len() == vector::MAX_SEGMENTS {
                        return Err(Error::Budget);
                    }
                    path.push(Segment::Close);
                }
            }
            return Ok(vec![(Arc::new(path), [1., 0., 0., 1., 0., 0.])]);
        }
        evaluation::GeometryData::Text(text) => Arc::new(
            text.outlines_with(&mut || {
                checkpoint().map_err(|error| match error {
                    Error::Cancelled => outlines::Error::Cancelled,
                    _ => outlines::Error::Budget,
                })
            })
            .map_err(|error| match error {
                outlines::Error::Cancelled => Error::Cancelled,
                outlines::Error::Budget => Error::Budget,
                other => Error::Invalid(format!("Text outline preparation failed: {other:?}")),
            })?,
        ),
        evaluation::GeometryData::Outlines(outlines) => outlines.curves.clone(),
        evaluation::GeometryData::Solids(_)
        | evaluation::GeometryData::Mesh(_)
        | evaluation::GeometryData::Instances(_) => {
            return Err(Error::Invalid(
                "Screen rendering requires planar geometry".into(),
            ));
        }
    };
    let mut paths = vec![];
    for glyph in &curves.glyphs {
        checkpoint()?;
        if paths.len() == vector::MAX_DRAWINGS {
            return Err(Error::Budget);
        }
        paths.push((glyph.path.clone(), glyph.transform));
    }
    Ok(paths)
}
