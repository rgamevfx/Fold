//! Native metric solids from retained outlined text.
use super::{FailureKind, Geometry, GeometryData, Scope, State, Value, style::Appearance};
use crate::{
    execution::Stop,
    types::{Literal, Space, Unit},
};
use fold_foundation::ObjectId;
use fold_render::{
    spatial::extrude,
    text::{extrude as text_extrude, mesh},
};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct Topology {
    pub id: u64,
    pub vertex_ids: Vec<u64>,
    pub face_ids: Vec<u64>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub id: u64,
    pub source_glyph: u64,
    pub topology: Option<Arc<Topology>>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct TextSolids {
    /// Includes input appearance as provenance; spatial paint requires Material.
    pub source: Arc<Geometry>,
    pub prepared: Arc<text_extrude::Prepared>,
    pub glyphs: Vec<Glyph>,
}
impl State<'_> {
    pub(super) fn extrude_text(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let Some(Value::Geometry(source)) = args.get("profile") else {
            return Err("Extrude requires a profile".into());
        };
        let GeometryData::Outlines(outlines) = &source.data else {
            return Err("This Extrude mode requires Outline Text output".into());
        };
        let primitive = |name| {
            args.get(name)
                .ok_or_else(|| format!("Missing {name}"))?
                .primitive()
        };
        let meters = |name| match primitive(name)? {
            Literal::Distance {
                value,
                unit: Unit::Meters,
            } if value.is_finite() && *value > 0. => Ok(*value),
            _ => Err(format!("{name} must be a positive metric distance")),
        };
        let scale = meters("canvas_unit")?;
        let depth = meters("depth")?;
        let Literal::Integer { value: segments } = primitive("segments")? else {
            return Err("Expected depth segments".into());
        };
        let Literal::Scalar { value: detail } = primitive("detail")? else {
            return Err("Expected curve detail".into());
        };
        if !(1..=4096).contains(segments)
            || !(1e-9..=1e6).contains(&scale)
            || !(1e-6..=1e6).contains(&depth)
            || !detail.is_finite()
            || !(1e-9..=1e9).contains(detail)
        {
            return Err("Extrude controls exceed supported ranges".into());
        }
        let planar = mesh::Cache::default()
            .prepare_with(outlines.curves.clone(), *detail, &mut || {
                self.budget.step().map_err(|_| match self.budget.stop() {
                    Some(Stop::Cancelled) => mesh::Error::Cancelled,
                    _ => mesh::Error::Budget,
                })
            })
            .map_err(|error| {
                match error {
                    mesh::Error::Budget => self.failure = Some(FailureKind::Budget),
                    mesh::Error::Cancelled => self.failure = Some(FailureKind::Cancelled),
                    _ => {}
                }
                format!("Extrude profile preparation failed: {error:?}")
            })?;
        let prepared = text_extrude::prepare(
            Arc::new(planar),
            scale,
            depth,
            *segments as u32,
            &mut || {
                self.budget.step().map_err(|_| match self.budget.stop() {
                    Some(Stop::Cancelled) => extrude::Error::Cancelled,
                    _ => extrude::Error::Budget,
                })
            },
        )
        .map_err(|error| {
            match error {
                text_extrude::Error::Extrusion(extrude::Error::Budget) => {
                    self.failure = Some(FailureKind::Budget)
                }
                text_extrude::Error::Extrusion(extrude::Error::Cancelled) => {
                    self.failure = Some(FailureKind::Cancelled)
                }
                _ => {}
            }
            format!("Extrude failed: {error:?}")
        })?;
        self.reserve(1 + prepared.glyphs.len())?;
        let mut topologies = BTreeMap::new();
        let mut glyphs = Vec::with_capacity(prepared.glyphs.len());
        for glyph in &prepared.glyphs {
            let source_glyph = outlines.source.glyph_ids[glyph.glyph];
            let topology = if let Some(solid) = &glyph.solid {
                let key = Arc::as_ptr(solid);
                if let Some(topology) = topologies.get(&key) {
                    Some(Arc::clone(topology))
                } else {
                    self.reserve(1 + solid.mesh.positions.len() + solid.mesh.indices.len() / 3)?;
                    // Correspondence follows indexed topology, not metric coordinates.
                    // Different tessellation or segmentation invalidates these IDs.
                    let mut signature = Vec::new();
                    signature.extend_from_slice(b"solid-topology");
                    signature.extend_from_slice(&(solid.mesh.positions.len() as u64).to_le_bytes());
                    for value in solid.mesh.indices.iter().chain(&solid.source_vertices) {
                        self.budget.step()?;
                        signature.extend_from_slice(&value.to_le_bytes());
                    }
                    let topology_id = self.identity(scope, id, source_glyph, &signature)?;
                    let mut vertex_ids = vec![];
                    for index in 0..solid.mesh.positions.len() {
                        vertex_ids.push(self.identity(
                            scope,
                            id,
                            topology_id,
                            format!("vertex:{index}").as_bytes(),
                        )?);
                    }
                    let mut face_ids = vec![];
                    for index in 0..solid.mesh.indices.len() / 3 {
                        face_ids.push(self.identity(
                            scope,
                            id,
                            topology_id,
                            format!("face:{index}").as_bytes(),
                        )?);
                    }
                    let topology = Arc::new(Topology {
                        id: topology_id,
                        vertex_ids,
                        face_ids,
                    });
                    topologies.insert(key, topology.clone());
                    Some(topology)
                }
            } else {
                None
            };
            glyphs.push(Glyph {
                id: self.identity(scope, id, source_glyph, b"solid-glyph")?,
                source_glyph,
                topology,
            });
        }
        Ok(Value::Geometry(Arc::new(Geometry {
            id: self.identity(scope, id, source.id, b"solids")?,
            space: Space::World,
            unit: Unit::Meters,
            data: GeometryData::Solids(Arc::new(TextSolids {
                source: source.clone(),
                prepared: Arc::new(prepared),
                glyphs,
            })),
            appearance: Appearance::default(),
        })))
    }
}
