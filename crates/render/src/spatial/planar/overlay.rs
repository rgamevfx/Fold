//! Geometric partition of two coplanar tessellations. No depth displacement.
use super::{Cache, Error, Mesh, Prepared};
use i_overlay::{
    core::{extract::BooleanExtractionBuffer, fill_rule::FillRule, overlay_rule::OverlayRule},
    float::overlay::FloatOverlay,
};
use std::sync::Arc;
type Point = [f64; 2];
const MAX_PIECES: usize = 262_144;

#[derive(Clone)]
pub struct Disjoint {
    pub fill: Option<Arc<Mesh>>,
    pub stroke: Option<Arc<Mesh>>,
    pub overlap: Option<Arc<Mesh>>,
}
pub(super) struct Entry {
    fill: Arc<Mesh>,
    stroke: Arc<Mesh>,
    result: Disjoint,
    pub(super) bytes: usize,
}
impl Cache {
    /// Partition Fill and Stroke into non-overlapping regions. Callers paint the
    /// intersection with Stroke-over-Fill and retain each exclusive region's paint.
    /// This avoids depending on a spatial renderer's coplanar material ordering.
    /// Checkpoints surround synchronous Boolean graph construction/extraction;
    /// the dependency does not expose checkpoints within those operations.
    pub fn disjoint_with(
        &mut self,
        prepared: Prepared,
        checkpoint: &mut dyn FnMut() -> Result<(), Error>,
    ) -> Result<Disjoint, Error> {
        checkpoint()?;
        let (Some(fill), Some(stroke)) = (&prepared.fill, &prepared.stroke) else {
            return Ok(Disjoint {
                fill: prepared.fill,
                stroke: prepared.stroke,
                overlap: None,
            });
        };
        if let Some(index) = self
            .partitions
            .iter()
            .position(|entry| Arc::ptr_eq(fill, &entry.fill) && Arc::ptr_eq(stroke, &entry.stroke))
        {
            let entry = self.partitions.remove(index).unwrap();
            let result = entry.result.clone();
            self.partitions.push_back(entry);
            checkpoint()?;
            return Ok(result);
        }
        let source_fill = fill.clone();
        let source_stroke = stroke.clone();
        let mut triangles = |mesh: &Mesh| -> Result<Vec<Vec<Point>>, Error> {
            if !mesh.indices.len().is_multiple_of(3) || mesh.indices.len() / 3 > MAX_PIECES {
                return Err(Error::InvalidInput);
            }
            mesh.indices
                .chunks_exact(3)
                .map(|t| {
                    checkpoint()?;
                    let mut triangle = [[0.; 2]; 3];
                    for i in 0..3 {
                        let p = mesh
                            .positions
                            .get(t[i] as usize)
                            .ok_or(Error::InvalidInput)?;
                        if p[2] != 0. || p.iter().any(|v| !v.is_finite()) {
                            return Err(Error::InvalidInput);
                        }
                        triangle[i] = [f64::from(p[0]), f64::from(p[1])];
                    }
                    if side(triangle[0], triangle[1], triangle[2]) <= 0. {
                        return Err(Error::InvalidInput);
                    }
                    Ok(triangle.to_vec())
                })
                .collect()
        };
        let fill = triangles(fill)?;
        let stroke = triangles(stroke)?;
        checkpoint()?;
        // Extract all paint regions from one integer overlay graph. Clipping
        // individual triangles produces artificial interior edges and slivers
        // that can overlap after GPU subpixel quantization.
        let mut overlay = FloatOverlay::with_subj_and_clip(&fill, &stroke);
        checkpoint()?;
        let graph = overlay.build_graph_view(FillRule::NonZero);
        checkpoint()?;
        let mut extraction = BooleanExtractionBuffer::default();
        let mut mesh = |rule| -> Result<Option<Arc<Mesh>>, Error> {
            checkpoint()?;
            let Some(graph) = &graph else { return Ok(None) };
            let shapes = graph.extract_shapes(rule, &mut extraction);
            checkpoint()?;
            let mut path = Vec::new();
            for contour in shapes.into_iter().flatten() {
                checkpoint()?;
                if contour.len() < 3 {
                    continue;
                }
                if path.len() + contour.len() + 1 > crate::vector::MAX_SEGMENTS {
                    return Err(Error::Budget);
                }
                path.push(crate::vector::Segment::Move(contour[0]));
                for point in &contour[1..] {
                    checkpoint()?;
                    path.push(crate::vector::Segment::Line(*point));
                }
                path.push(crate::vector::Segment::Close);
            }
            if path.is_empty() {
                return Ok(None);
            }
            let drawing = crate::vector::Drawing {
                feather: None,
                path: Arc::new(path),
                transform: [1., 0., 0., 1., 0., 0.],
                fill: Some([1.; 4]),
                stroke: None,
                even_odd: false,
            };
            let mut stopped = None;
            let triangles = self
                .geometry
                .get_with(&drawing, None, &mut || {
                    checkpoint().map_err(|error| {
                        stopped = Some(error);
                        "partition tessellation stopped".into()
                    })
                })
                .map_err(|error| {
                    stopped.unwrap_or_else(|| match error {
                        crate::vector_geometry::PreparationError::Budget(_) => Error::Budget,
                        other => Error::Geometry(other.to_string()),
                    })
                })?;
            self.mesh(triangles, checkpoint).map(Some)
        };
        let result = Disjoint {
            fill: mesh(OverlayRule::Difference)?,
            stroke: mesh(OverlayRule::InverseDifference)?,
            overlap: mesh(OverlayRule::Intersect)?,
        };
        checkpoint()?;
        // Conservative ownership bound: count even meshes also held by another
        // cache. Partition entries cannot keep an unaccounted evicted mesh alive.
        let bytes = [
            Some(&source_fill),
            Some(&source_stroke),
            result.fill.as_ref(),
            result.stroke.as_ref(),
            result.overlap.as_ref(),
        ]
        .into_iter()
        .flatten()
        .map(|mesh| {
            (mesh.positions.capacity() + mesh.normals.capacity()) * 12 + mesh.indices.capacity() * 4
        })
        .sum::<usize>();
        if bytes <= super::CACHE_BYTES {
            while self.partition_bytes + bytes > super::CACHE_BYTES || self.partitions.len() >= 64 {
                self.partition_bytes -= self.partitions.pop_front().unwrap().bytes;
            }
            self.partition_bytes += bytes;
            self.partitions.push_back(Entry {
                fill: source_fill,
                stroke: source_stroke,
                result: result.clone(),
                bytes,
            });
        }
        Ok(result)
    }
}
fn side(a: Point, b: Point, p: Point) -> f64 {
    (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
}
