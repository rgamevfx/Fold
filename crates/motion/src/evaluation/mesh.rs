//! Native mesh prototypes retain topology and prepared buffers independently of placement.
use super::{Geometry, GeometryData, Scope, State, Value, style::Appearance};
use crate::types::{Literal, Space, Unit};
use fold_foundation::ObjectId;
use fold_render::spatial::primitives;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct Mesh {
    /// Retained conversion input, including curves and font/resource provenance.
    pub source: Option<Arc<Geometry>>,
    pub prepared: Prepared,
    pub vertex_ids: Vec<u64>,
    /// Triangle slots; named regions retain the primitive or conversion partitions.
    pub face_ids: Vec<u64>,
}
/// Prepared native mesh data and explicit attribute correspondence.
#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub mesh: Arc<fold_render::spatial::Mesh>,
    pub uv: Vec<[f32; 2]>,
    /// Named index-buffer regions: six box faces, or front/back/sides.
    pub regions: BTreeMap<String, std::ops::Range<usize>>,
    /// Output vertex to planar cap vertex. Empty for primitive meshes.
    pub source_vertices: Vec<u32>,
    /// Retained input geometry identity for conversion provenance.
    pub source_geometry: Option<u64>,
}
impl State<'_> {
    pub(super) fn box_mesh(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let Literal::Vector {
            value,
            unit: Unit::Meters,
            space: Space::World,
        } = args.get("size").ok_or("Missing box size")?.primitive()?
        else {
            return Err("Box size requires a World metric vector".into());
        };
        if value.len() != 3
            || value
                .iter()
                .any(|v| !v.is_finite() || !(1e-6..=1e6).contains(v))
        {
            return Err("Box size must be in [1e-6,1e6] meters".into());
        }
        self.reserve(1 + 24 + 12)?;
        let prepared = primitives::box_mesh([value[0] as f32, value[1] as f32, value[2] as f32])
            .map_err(|e| format!("Box preparation failed: {e:?}"))?;
        let geometry_id = self.identity(scope, id, 0, b"box")?;
        let mut vertex_ids = Vec::with_capacity(24);
        for index in 0..24 {
            self.budget.step()?;
            vertex_ids.push(self.identity(
                scope,
                id,
                geometry_id,
                format!("vertex:{index}").as_bytes(),
            )?);
        }
        let mut face_ids = Vec::with_capacity(12);
        for index in 0..12 {
            self.budget.step()?;
            face_ids.push(self.identity(
                scope,
                id,
                geometry_id,
                format!("face:{index}").as_bytes(),
            )?);
        }
        Ok(Value::Geometry(Arc::new(Geometry {
            id: geometry_id,
            space: Space::World,
            unit: Unit::Meters,
            data: GeometryData::Mesh(Arc::new(Mesh {
                source: None,
                prepared: Prepared {
                    mesh: prepared.mesh,
                    uv: prepared.uv,
                    regions: [
                        "positive_x",
                        "negative_x",
                        "positive_y",
                        "negative_y",
                        "positive_z",
                        "negative_z",
                    ]
                    .into_iter()
                    .zip(prepared.faces)
                    .map(|(name, range)| (name.into(), range))
                    .collect(),
                    source_vertices: vec![],
                    source_geometry: None,
                },
                vertex_ids,
                face_ids,
            })),
            appearance: Appearance::default(),
        })))
    }
}
