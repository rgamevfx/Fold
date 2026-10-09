//! Explicit planar conversion. UVs retain canvas XY; extrusion duplicates them by
//! cap-vertex correspondence. Normals are generated, paint remains on the source.
use super::{FailureKind, Geometry, GeometryData, Scope, State, Value, mesh, style::Appearance};
use crate::{
    execution::Stop,
    types::{Literal, Space, Unit},
};
use fold_foundation::ObjectId;
use fold_render::{
    spatial::{self, extrude, planar},
    vector::{Drawing, Segment},
};
use std::{collections::BTreeMap, sync::Arc};

impl State<'_> {
    pub(super) fn profile_mesh(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
        solid: bool,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(source)) = args.get("profile") else {
            return Err("Expected planar profile".into());
        };
        if source.space != Space::Screen || source.unit != Unit::Canvas {
            return Err("Profile requires Screen canvas geometry".into());
        }
        let primitive = |name| {
            args.get(name)
                .ok_or_else(|| format!("Missing {name}"))?
                .primitive()
        };
        let Literal::Distance {
            value: scale,
            unit: Unit::Meters,
        } = primitive("canvas_unit")?
        else {
            return Err("Expected metric canvas conversion".into());
        };
        let Literal::Scalar { value: detail } = primitive("detail")? else {
            return Err("Expected curve detail".into());
        };
        let Literal::Bool { value: even_odd } = primitive("even_odd")? else {
            return Err("Expected fill rule".into());
        };
        if !scale.is_finite()
            || !(1e-9..=1e6).contains(scale)
            || !detail.is_finite()
            || !(1e-9..=1e9).contains(detail)
        {
            return Err("Profile scale/detail exceeds supported ranges".into());
        }
        let mut drawings = vec![];
        let drawing = |path, transform| Drawing {
            path,
            transform,
            fill: Some([1.; 4]),
            stroke: None,
            even_odd: *even_odd,
        };
        match &source.data {
            GeometryData::Contours(contours) => {
                let mut path = vec![];
                for contour in contours.iter() {
                    self.budget.step()?;
                    if !contour.closed || contour.points.len() < 3 {
                        return Err(
                            "Profile requires closed contours with at least three points".into(),
                        );
                    }
                    self.reserve(contour.points.len() + 1)?;
                    for (index, point) in contour.points.iter().enumerate() {
                        self.budget.step()?;
                        path.push(if index == 0 {
                            Segment::Move(point.position)
                        } else {
                            Segment::Line(point.position)
                        });
                    }
                    path.push(Segment::Close);
                }
                drawings.push(drawing(Arc::new(path), [1., 0., 0., 1., 0., 0.]));
            }
            GeometryData::Outlines(outlines) => {
                // Retain glyph profile boundaries, as in Screen and straight_text
                // conversion. A raster fill triangulation of all glyphs together
                // need not be conforming at unrelated scan-line intersections.
                self.reserve(outlines.curves.glyphs.len())?;
                for glyph in &outlines.curves.glyphs {
                    self.budget.step()?;
                    drawings.push(drawing(glyph.path.clone(), glyph.transform));
                }
            }
            _ => return Err("Profile requires closed contours or Outline Text".into()),
        }
        let mut metric = spatial::Mesh {
            positions: vec![],
            normals: vec![],
            indices: vec![],
        };
        let mut cap_uv = vec![];
        let mut cache = planar::Cache::default();
        for drawing in &drawings {
            let [a, b, c, d, x, y] = drawing.transform;
            let detail = detail * (a.abs() + c.abs()).max(b.abs() + d.abs());
            let prepared = cache
                .prepare_with(drawing, detail, &mut || {
                    self.budget.step().map_err(|_| {
                        if self.budget.stop() == Some(Stop::Cancelled) {
                            planar::Error::Cancelled
                        } else {
                            planar::Error::Budget
                        }
                    })
                })
                .map_err(|error| {
                    match error {
                        planar::Error::Budget => self.failure = Some(FailureKind::Budget),
                        planar::Error::Cancelled => self.failure = Some(FailureKind::Cancelled),
                        _ => {}
                    }
                    format!("Profile tessellation: {error:?}")
                })?;
            let Some(cap) = prepared.fill else {
                continue;
            };
            self.reserve(cap.positions.len() + cap.indices.len())?;
            let offset =
                u32::try_from(metric.positions.len()).map_err(|_| "Profile vertex overflow")?;
            for point in &cap.positions {
                self.budget.step()?;
                let px = f64::from(point[0]);
                let py = f64::from(point[1]);
                let canvas = [a * px + c * py + x, b * px + d * py + y];
                let position = [(canvas[0] * scale) as f32, (-canvas[1] * scale) as f32, 0.];
                if position.iter().any(|v| !v.is_finite() || v.abs() > 1e6) {
                    return Err("Converted profile exceeds metric range".into());
                }
                metric.positions.push(position);
                metric.normals.push([0., 0., 1.]);
                cap_uv.push(canvas.map(|v| v as f32));
            }
            for face in cap.indices.chunks_exact(3) {
                self.budget.step()?;
                let mut face = [face[0] + offset, face[1] + offset, face[2] + offset];
                if a * d - b * c > 0. {
                    face.swap(1, 2);
                }
                metric.indices.extend(face);
            }
        }
        if metric.indices.is_empty() {
            return Err("Profile has no filled area".into());
        }
        let (render_mesh, regions, source_vertices) = if solid {
            let Literal::Distance {
                value: depth,
                unit: Unit::Meters,
            } = primitive("depth")?
            else {
                return Err("Expected metric depth".into());
            };
            let Literal::Integer { value: segments } = primitive("segments")? else {
                return Err("Expected depth segments".into());
            };
            if !depth.is_finite() || !(1e-6..=1e6).contains(depth) || !(1..=4096).contains(segments)
            {
                return Err("Extrude depth/segments exceed supported ranges".into());
            }
            let extrusion =
                extrude::prepare_with(&metric, *depth as f32, *segments as u32, &mut || {
                    self.budget.step().map_err(|_| {
                        if self.budget.stop() == Some(Stop::Cancelled) {
                            extrude::Error::Cancelled
                        } else {
                            extrude::Error::Budget
                        }
                    })
                })
                .map_err(|e| {
                    match e {
                        extrude::Error::Budget => self.failure = Some(FailureKind::Budget),
                        extrude::Error::Cancelled => self.failure = Some(FailureKind::Cancelled),
                        _ => {}
                    }
                    format!("Profile extrusion: {e:?}")
                })?;
            (
                extrusion.mesh,
                [
                    ("front".into(), extrusion.regions.front),
                    ("back".into(), extrusion.regions.back),
                    ("sides".into(), extrusion.regions.sides),
                ]
                .into(),
                extrusion.source_vertices,
            )
        } else {
            let count = metric.indices.len();
            (
                Arc::new(metric),
                [("front".into(), 0..count)].into(),
                (0..cap_uv.len() as u32).collect(),
            )
        };
        self.reserve(render_mesh.positions.len() + render_mesh.indices.len() / 3)?;
        let mut signature = Vec::new();
        for index in &render_mesh.indices {
            self.budget.step()?;
            signature.extend_from_slice(&index.to_le_bytes());
        }
        let topology = self.identity(scope, id, source.id, &signature)?;
        let mut vertex_ids = vec![];
        let mut face_ids = vec![];
        let mut uv = vec![];
        for (index, &source_vertex) in source_vertices.iter().enumerate() {
            vertex_ids.push(self.identity(
                scope,
                id,
                topology,
                format!("vertex:{index}").as_bytes(),
            )?);
            uv.push(cap_uv[source_vertex as usize]);
        }
        for index in 0..render_mesh.indices.len() / 3 {
            face_ids.push(self.identity(
                scope,
                id,
                topology,
                format!("face:{index}").as_bytes(),
            )?);
        }
        Ok(Value::Geometry(Arc::new(Geometry {
            attributes: Default::default(),
            id: topology,
            space: Space::World,
            unit: Unit::Meters,
            data: GeometryData::Mesh(Arc::new(mesh::Mesh {
                source: Some(source.clone()),
                prepared: mesh::Prepared {
                    mesh: render_mesh,
                    uv,
                    regions,
                    source_vertices,
                    source_geometry: Some(source.id),
                },
                vertex_ids,
                face_ids,
            })),
            appearance: Appearance::default(),
        })))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        evaluation::{Contour, Point, Prepared},
        execution::Budget,
    };

    #[test]
    fn nested_contours_obey_fill_rule_and_open_profiles_reject() {
        let document = crate::package::create(fold_foundation::DocumentId::new()).unwrap();
        let construction = crate::model::Construction::from_document(&document).unwrap();
        let prepared = Prepared::new(construction.clone()).unwrap();
        let mut state = State {
            prepared: &prepared,
            scene_time: fold_foundation::Time::ZERO,
            budget: Budget::new(1_000_000, &|| Ok(())),
            remaining_elements: 100_000,
            failure: None,
            failure_context: None,
            values: BTreeMap::new(),
            scopes: BTreeMap::new(),
            identities: BTreeMap::new(),
            depth: 0,
            text: None,
            element: None,
        };
        let scope = state.instance(None, &construction.root).unwrap();
        let operation = ObjectId::new();
        let contour = |id, low, high| Contour {
            id,
            closed: true,
            points: [[low, low], [high, low], [high, high], [low, high]]
                .into_iter()
                .enumerate()
                .map(|(index, position)| Point {
                    id: id * 10 + index as u64,
                    position,
                })
                .collect(),
        };
        let source = Geometry {
            attributes: Default::default(),
            id: 123,
            space: Space::Screen,
            unit: Unit::Canvas,
            data: GeometryData::Contours(Arc::new(vec![
                contour(1, 0., 100.),
                contour(2, 25., 75.),
            ])),
            appearance: Appearance::default(),
        };
        let mut args = BTreeMap::from([
            ("profile", Value::Geometry(Arc::new(source.clone()))),
            (
                "canvas_unit",
                Value::Primitive(Literal::Distance {
                    value: 0.01,
                    unit: Unit::Meters,
                }),
            ),
            ("detail", Value::Primitive(Literal::Scalar { value: 1. })),
            ("even_odd", Value::Primitive(Literal::Bool { value: false })),
        ]);
        for (rule, expected_area) in [(false, 1.), (true, 0.75)] {
            args.insert("even_odd", Value::Primitive(Literal::Bool { value: rule }));
            let Value::Geometry(g) = state.profile_mesh(&scope, operation, &args, false).unwrap()
            else {
                panic!()
            };
            let GeometryData::Mesh(m) = &g.data else {
                panic!()
            };
            let mesh = &m.prepared.mesh;
            let area: f64 = mesh
                .indices
                .chunks_exact(3)
                .map(|t| {
                    let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[t[i] as usize]);
                    f64::from((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])).abs()
                        / 2.
                })
                .sum();
            assert!((area - expected_area).abs() < 1e-8);
        }
        let mut open = source;
        let GeometryData::Contours(contours) = &mut open.data else {
            panic!()
        };
        Arc::make_mut(contours)[0].closed = false;
        args.insert("profile", Value::Geometry(Arc::new(open)));
        assert!(
            state
                .profile_mesh(&scope, operation, &args, false)
                .unwrap_err()
                .contains("closed contours")
        );
    }
}
