//! Explicit canvas-to-meter conversion and shared text solids. No shaping or I/O.
use super::mesh::Prepared as Planar;
use crate::spatial::{Mesh, extrude};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidInput,
    Extrusion(extrude::Error),
}
#[derive(Clone, Debug, PartialEq)]
pub struct GlyphSolid {
    pub glyph: usize,
    /// Whitespace retains its association but has no solid.
    pub solid: Option<Arc<extrude::Extrusion>>,
    /// Local metric XY to Y-up world XY. Z is unchanged; translation is meters.
    pub transform: [f64; 6],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub planar: Arc<Planar>,
    pub meters_per_canvas: f64,
    pub depth_meters: f64,
    pub segments: u32,
    pub glyphs: Vec<GlyphSolid>,
}

/// Maps layout origin to world origin and flips canvas Y-down to world Y-up.
/// Scale is explicitly authored; no camera or implicit physical scale is inferred.
/// Mesh coordinates use the renderer's f32 contract, while mapping and authored
/// depth remain f64. Repeated planar mesh objects share one extruded solid.
pub fn prepare(
    planar: Arc<Planar>,
    meters_per_canvas: f64,
    depth_meters: f64,
    segments: u32,
    checkpoint: &mut dyn FnMut() -> Result<(), extrude::Error>,
) -> Result<Prepared, Error> {
    let stop = |e| Error::Extrusion(e);
    checkpoint().map_err(stop)?;
    if planar.glyphs.len() > crate::vector::MAX_DRAWINGS {
        return Err(Error::Extrusion(extrude::Error::Budget));
    }
    if !meters_per_canvas.is_finite()
        || !(1e-9..=1e6).contains(&meters_per_canvas)
        || !depth_meters.is_finite()
        || !(1e-6..=1e6).contains(&depth_meters)
        || !(1..=4096).contains(&segments)
        || planar.glyphs.len() != planar.outlines.glyphs.len()
    {
        return Err(Error::InvalidInput);
    }
    let mut solids = BTreeMap::new();
    let mut glyphs = Vec::with_capacity(planar.glyphs.len());
    let mut expanded_vertices = 0usize;
    let mut expanded_indices = 0usize;
    for (index, glyph) in planar.glyphs.iter().enumerate() {
        checkpoint().map_err(stop)?;
        let [a, b, c, d, e, f] = glyph.transform;
        if glyph.glyph != index || !glyph.transform.iter().all(|v| v.is_finite()) {
            return Err(Error::InvalidInput);
        }
        let transform = [a, -b, c, -d, e * meters_per_canvas, -f * meters_per_canvas];
        if !transform.iter().all(|v| v.is_finite()) {
            return Err(Error::InvalidInput);
        }
        let solid = if let Some(mesh) = &glyph.mesh {
            // Admit before copying the cap, even for callers with hand-built input.
            if mesh.positions.len() > 500_000 || mesh.indices.len() > 1_500_000 {
                return Err(Error::Extrusion(extrude::Error::Budget));
            }
            // Address keys are only worker-local sharing keys, never persisted IDs.
            let key = Arc::as_ptr(mesh);
            let solid = if let Some(solid) = solids.get(&key) {
                Arc::clone(solid)
            } else {
                let mut cap = Mesh {
                    positions: Vec::with_capacity(mesh.positions.len()),
                    normals: vec![],
                    indices: mesh.indices.clone(),
                };
                for p in &mesh.positions {
                    checkpoint().map_err(stop)?;
                    cap.positions.push([
                        (f64::from(p[0]) * meters_per_canvas) as f32,
                        (f64::from(p[1]) * meters_per_canvas) as f32,
                        p[2],
                    ]);
                }
                let solid = Arc::new(
                    extrude::prepare_with(&cap, depth_meters as f32, segments, checkpoint)
                        .map_err(stop)?,
                );
                solids.insert(key, solid.clone());
                solid
            };
            expanded_vertices = expanded_vertices
                .checked_add(solid.mesh.positions.len())
                .ok_or(Error::Extrusion(extrude::Error::Budget))?;
            expanded_indices = expanded_indices
                .checked_add(solid.mesh.indices.len())
                .ok_or(Error::Extrusion(extrude::Error::Budget))?;
            if expanded_vertices > 1_000_000 || expanded_indices > 3_000_000 {
                return Err(Error::Extrusion(extrude::Error::Budget));
            }
            Some(solid)
        } else {
            None
        };
        glyphs.push(GlyphSolid {
            glyph: index,
            solid,
            transform,
        });
    }
    checkpoint().map_err(stop)?;
    Ok(Prepared {
        planar,
        meters_per_canvas,
        depth_meters,
        segments,
        glyphs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_media::Cancel;
    fn planar() -> Arc<Planar> {
        let layout = crate::text::Service::new(
            include_bytes!("../../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
            "Noto Sans",
        )
        .unwrap()
        .layout("O B 8 O", 140., None)
        .unwrap();
        let outlines =
            Arc::new(crate::text::outlines::prepare(layout, &Cancel::default()).unwrap());
        Arc::new(
            crate::text::mesh::Cache::default()
                .prepare(outlines, 1., &Cancel::default())
                .unwrap(),
        )
    }
    #[test]
    fn physical_mapping_retains_layout_holes_regions_and_shared_solids() {
        let planar = planar();
        let prepared = prepare(planar.clone(), 0.01, 0.15, 3, &mut || Ok(())).unwrap();
        assert!(Arc::ptr_eq(&planar, &prepared.planar));
        assert!(Arc::ptr_eq(
            prepared.glyphs[0].solid.as_ref().unwrap(),
            prepared.glyphs[6].solid.as_ref().unwrap()
        ));
        for (glyph, source) in prepared.glyphs.iter().zip(&planar.glyphs) {
            assert_eq!(glyph.glyph, source.glyph);
            let [a, b, c, d, x, y] = glyph.transform;
            assert_eq!([a, b, c, d], [1., 0., 0., 1.]);
            assert_eq!(x, source.transform[4] * 0.01);
            assert_eq!(y, -source.transform[5] * 0.01);
            let Some(solid) = &glyph.solid else {
                assert!(source.mesh.is_none());
                continue;
            };
            let cap = source.mesh.as_ref().unwrap();
            assert_eq!(solid.regions.front, 0..cap.indices.len());
            assert_eq!(solid.regions.sides.end, solid.mesh.indices.len());
            assert!(solid.regions.sides.end > solid.regions.sides.start);
            assert_eq!(
                &solid.mesh.indices[solid.regions.front.clone()],
                cap.indices
            );
            for (index, source_index) in solid.source_vertices.iter().enumerate() {
                let actual = solid.mesh.positions[index];
                let original = cap.positions[*source_index as usize];
                assert_eq!(actual[0], (f64::from(original[0]) * 0.01) as f32);
                assert_eq!(actual[1], (f64::from(original[1]) * 0.01) as f32);
                assert!((-0.150001..=0.).contains(&actual[2]));
            }
            let area: f64 = cap
                .indices
                .chunks_exact(3)
                .map(|t| {
                    let [a, b, c] =
                        [t[0], t[1], t[2]].map(|i| cap.positions[i as usize].map(f64::from));
                    ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2.
                })
                .sum();
            let volume: f64 = solid
                .mesh
                .indices
                .chunks_exact(3)
                .map(|t| {
                    let [a, b, c] =
                        [t[0], t[1], t[2]].map(|i| solid.mesh.positions[i as usize].map(f64::from));
                    (a[0] * (b[1] * c[2] - b[2] * c[1])
                        + a[1] * (b[2] * c[0] - b[0] * c[2])
                        + a[2] * (b[0] * c[1] - b[1] * c[0]))
                        / 6.
                })
                .sum();
            let expected = area * 0.01f64.powi(2) * 0.15;
            assert!((volume - expected).abs() < expected * 1e-5);
        }
    }
    #[test]
    fn invalid_mapping_and_interrupted_preparation_publish_no_solids() {
        let planar = planar();
        for scale in [0., -1., f64::NAN, f64::INFINITY] {
            assert!(matches!(
                prepare(planar.clone(), scale, 0.15, 1, &mut || Ok(())),
                Err(Error::InvalidInput)
            ));
        }
        let mut calls = 0;
        prepare(planar.clone(), 0.01, 0.15, 3, &mut || {
            calls += 1;
            Ok(())
        })
        .unwrap();
        for at in [1, 4, 100, calls / 2, calls] {
            for error in [extrude::Error::Cancelled, extrude::Error::Budget] {
                let mut seen = 0;
                let result = prepare(planar.clone(), 0.01, 0.15, 3, &mut || {
                    seen += 1;
                    if seen == at {
                        Err(error.clone())
                    } else {
                        Ok(())
                    }
                });
                assert!(matches!(result,Err(Error::Extrusion(ref e)) if e==&error));
                assert_eq!(seen, at);
            }
        }
    }
}
