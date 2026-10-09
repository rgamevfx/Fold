//! Planar spatial geometry from retained text outlines. No shaping or font lookup.
use super::outlines::Outlines;
use crate::{
    spatial::{Mesh, planar},
    vector,
    vector_geometry::MAX_INSTANCE_TRIANGLES,
};
use fold_media::Cancel;
use std::sync::Arc;

pub use crate::spatial::planar::Error;

#[derive(Clone, Debug, PartialEq)]
pub struct GlyphMesh {
    pub glyph: usize,
    /// Local Y-up plane, facing +Z. Whitespace has no mesh.
    pub mesh: Option<Arc<Mesh>>,
    /// Exact local outline → Y-down layout affine transform, not a world transform.
    pub transform: [f64; 6],
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub outlines: Arc<Outlines>,
    pub glyphs: Vec<GlyphMesh>,
}

/// Text association wrapper around shared planar preparation.
#[derive(Default)]
pub struct Cache {
    planar: planar::Cache,
}
impl Cache {
    /// `projected_scale` is pixels per local outline unit, including placement scale.
    /// It controls curve subdivision only; positions and placement are unchanged.
    pub fn prepare(
        &mut self,
        outlines: Arc<Outlines>,
        projected_scale: f64,
        cancel: &Cancel,
    ) -> Result<Prepared, Error> {
        self.prepare_with(outlines, projected_scale, &mut || {
            cancel.check().map_err(|_| Error::Cancelled)
        })
    }
    /// Caller-owned work and cancellation checkpoint, including tessellation.
    pub fn prepare_with(
        &mut self,
        outlines: Arc<Outlines>,
        projected_scale: f64,
        checkpoint: &mut dyn FnMut() -> Result<(), Error>,
    ) -> Result<Prepared, Error> {
        checkpoint()?;
        if outlines.glyphs.len() > vector::MAX_DRAWINGS {
            return Err(Error::Budget);
        }
        if !projected_scale.is_finite()
            || !(1e-9..=1e9).contains(&projected_scale)
            || outlines.glyphs.len() != outlines.layout.glyphs.len()
            || outlines
                .glyphs
                .iter()
                .enumerate()
                .any(|(i, g)| g.glyph != i)
        {
            return Err(Error::InvalidInput);
        }
        let drawings = outlines.drawings(Some([1.; 4]), None);
        vector::validate(&drawings).map_err(Error::Geometry)?;
        let mut glyphs = Vec::with_capacity(drawings.len());
        let mut expanded = 0usize;
        for (drawing, glyph) in drawings.iter().zip(&outlines.glyphs) {
            checkpoint()?;
            let mesh = self
                .planar
                .prepare_with(drawing, projected_scale, checkpoint)?
                .fill;
            expanded += mesh.as_ref().map_or(0, |m| m.indices.len() / 3);
            if expanded > MAX_INSTANCE_TRIANGLES {
                return Err(Error::Budget);
            }
            glyphs.push(GlyphMesh {
                glyph: glyph.glyph,
                mesh,
                transform: glyph.transform,
            });
        }
        checkpoint()?;
        Ok(Prepared { outlines, glyphs })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn outlines() -> Arc<Outlines> {
        let layout = crate::text::Service::new(
            include_bytes!("../../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
            "Noto Sans",
        )
        .unwrap()
        .layout("O B 8 O", 140., None)
        .unwrap();
        Arc::new(super::super::outlines::prepare(layout, &Cancel::default()).unwrap())
    }

    #[test]
    fn subdivision_limits_remain_typed_budget_failures() {
        assert!(matches!(
            Cache::default().prepare_with(outlines(), 1e9, &mut || Ok(())),
            Err(Error::Budget)
        ));
    }
    #[test]
    fn caller_stops_preserve_error_kind_and_allow_clean_retry() {
        let outlines = outlines();
        let mut count = 0;
        Cache::default()
            .prepare_with(outlines.clone(), 1., &mut || {
                count += 1;
                Ok(())
            })
            .unwrap();
        for at in [1, 4, 100, count / 2, count] {
            for error in [Error::Budget, Error::Cancelled] {
                let mut cache = Cache::default();
                let mut seen = 0;
                let result = cache.prepare_with(outlines.clone(), 1., &mut || {
                    seen += 1;
                    if seen == at {
                        Err(error.clone())
                    } else {
                        Ok(())
                    }
                });
                assert!(matches!(result, Err(ref actual) if actual == &error));
                assert_eq!(seen, at);
                let retry = cache
                    .prepare(outlines.clone(), 1., &Cancel::default())
                    .unwrap();
                assert_eq!(retry.glyphs.len(), 7);
                assert!(Arc::ptr_eq(
                    retry.glyphs[0].mesh.as_ref().unwrap(),
                    retry.glyphs[6].mesh.as_ref().unwrap()
                ));
                assert!(matches!(
                    cache.prepare_with(outlines.clone(), 1., &mut || Err(Error::Cancelled)),
                    Err(Error::Cancelled)
                ));
            }
        }
    }
    #[test]
    fn retained_layout_placement_and_repeated_meshes_survive_cache_reuse() {
        let outlines = outlines();
        let mut cache = Cache::default();
        let prepared = cache
            .prepare(outlines.clone(), 1., &Cancel::default())
            .unwrap();
        assert!(Arc::ptr_eq(&prepared.outlines, &outlines));
        assert_eq!(prepared.glyphs.len(), 7);
        for (index, glyph) in prepared.glyphs.iter().enumerate() {
            assert_eq!(glyph.glyph, index);
            assert_eq!(glyph.transform, outlines.glyphs[index].transform);
            assert_eq!(glyph.mesh.is_none(), index % 2 == 1);
        }
        let first = prepared.glyphs[0].mesh.as_ref().unwrap();
        assert!(Arc::ptr_eq(
            first,
            prepared.glyphs[6].mesh.as_ref().unwrap()
        ));
        let mut moved = (*outlines).clone();
        moved.glyphs[0].transform = [2., 0.5, 0.25, -1., 42., 17.];
        let again = cache
            .prepare(Arc::new(moved), 1., &Cancel::default())
            .unwrap();
        assert!(Arc::ptr_eq(first, again.glyphs[0].mesh.as_ref().unwrap()));
        assert_eq!(again.glyphs[0].transform, [2., 0.5, 0.25, -1., 42., 17.]);
        drop(cache);
        assert!(!first.positions.is_empty());
    }

    #[test]
    fn planar_mesh_coverage_matches_vector_interiors_and_faces_positive_z() {
        let outlines = outlines();
        let prepared = Cache::default()
            .prepare(outlines.clone(), 1., &Cancel::default())
            .unwrap();
        let pixels = crate::vector_geometry::rasterize(
            &outlines.drawings(Some([1.; 4]), None),
            640,
            220,
            &Cancel::default(),
        )
        .unwrap();
        let mut triangles = vec![];
        for glyph in &prepared.glyphs {
            let Some(mesh) = &glyph.mesh else { continue };
            assert!(mesh.normals.iter().all(|n| *n == [0., 0., 1.]));
            for indices in mesh.indices.chunks_exact(3) {
                let p = indices
                    .iter()
                    .map(|i| mesh.positions[*i as usize])
                    .collect::<Vec<_>>();
                assert!(
                    (p[1][0] - p[0][0]) * (p[2][1] - p[0][1])
                        - (p[1][1] - p[0][1]) * (p[2][0] - p[0][0])
                        > 0.
                );
                let [a, b, c, d, tx, ty] = glyph.transform;
                triangles.push(
                    p.iter()
                        .map(|p| {
                            [
                                a * f64::from(p[0]) + c * f64::from(p[1]) + tx,
                                b * f64::from(p[0]) + d * f64::from(p[1]) + ty,
                            ]
                        })
                        .collect::<Vec<_>>(),
                );
            }
        }
        let mut opaque = 0;
        let mut clear = 0;
        for y in (0..220).step_by(2) {
            for x in (0..640).step_by(2) {
                let alpha = pixels[y * 640 + x][3];
                if alpha > 1e-6 && alpha < 1. - 1e-6 {
                    continue;
                }
                let p = [x as f64 + 0.5, y as f64 + 0.5];
                let covered = triangles.iter().any(|t| {
                    let sides = [0, 1, 2].map(|i| {
                        let a = t[i];
                        let b = t[(i + 1) % 3];
                        (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0])
                    });
                    sides.iter().all(|s| *s <= 0.) || sides.iter().all(|s| *s >= 0.)
                });
                assert_eq!(covered, alpha > 0.5, "coverage at {x},{y}");
                if covered {
                    opaque += 1;
                } else {
                    clear += 1;
                }
            }
        }
        assert!(opaque > 1000 && clear > 10000);
    }

    #[test]
    fn invalid_scale_mapping_and_cancel_do_not_publish_geometry() {
        let outlines = outlines();
        let mut cache = Cache::default();
        for scale in [0., -1., f64::NAN, f64::INFINITY, 1e10] {
            assert!(matches!(
                cache.prepare(outlines.clone(), scale, &Cancel::default()),
                Err(Error::InvalidInput)
            ));
        }
        let mut bad = (*outlines).clone();
        bad.glyphs[0].glyph = 1;
        assert!(matches!(
            cache.prepare(Arc::new(bad), 1., &Cancel::default()),
            Err(Error::InvalidInput)
        ));
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(matches!(
            cache.prepare(outlines.clone(), 1., &cancel),
            Err(Error::Cancelled)
        ));
        assert!(cache.prepare(outlines, 1., &Cancel::default()).is_ok());
    }
}
