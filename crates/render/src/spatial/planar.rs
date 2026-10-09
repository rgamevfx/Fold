//! Cached planar fill/stroke meshes from retained vector paths.
//! Positions stay in local path coordinates, facing +Z. Placement owns transforms.
mod overlay;
use super::Mesh;
use crate::{
    vector,
    vector_geometry::{GeometryCache, MAX_INSTANCE_TRIANGLES, PreparationError, Triangle},
};
pub use overlay::Disjoint;
use std::{
    collections::{BTreeMap, VecDeque},
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    Budget,
    InvalidInput,
    Geometry(String),
}

#[derive(Clone, Debug, PartialEq)]
pub struct Prepared {
    pub fill: Option<Arc<Mesh>>,
    pub stroke: Option<Arc<Mesh>>,
}

struct Entry {
    triangles: Arc<Vec<Triangle>>,
    mesh: Arc<Mesh>,
    bytes: usize,
}

/// Bounded preparation caches; returned meshes remain owned by their consumers.
#[derive(Default)]
pub struct Cache {
    geometry: GeometryCache,
    entries: VecDeque<Entry>,
    bytes: usize,
    partitions: VecDeque<overlay::Entry>,
    partition_bytes: usize,
}
const CACHE_BYTES: usize = 16 * 1024 * 1024;

impl Cache {
    /// Scale is pixels per local unit and controls subdivision only. Paint and
    /// translation do not affect cached geometry; callers retain source drawings.
    pub fn prepare_with(
        &mut self,
        drawing: &vector::Drawing,
        projected_scale: f64,
        checkpoint: &mut dyn FnMut() -> Result<(), Error>,
    ) -> Result<Prepared, Error> {
        checkpoint()?;
        if !projected_scale.is_finite() || !(1e-9..=1e9).contains(&projected_scale) {
            return Err(Error::InvalidInput);
        }
        vector::validate_working(std::slice::from_ref(drawing), true).map_err(Error::Geometry)?;
        let mut local = drawing.clone();
        local.transform = [projected_scale, 0., 0., projected_scale, 0., 0.];
        let mut expanded = 0usize;
        let mut part = |stroke: Option<&vector::Stroke>| -> Result<Option<Arc<Mesh>>, Error> {
            let mut stopped = None;
            let triangles = self
                .geometry
                .get_with(&local, stroke, &mut || {
                    checkpoint().map_err(|error| {
                        stopped = Some(error);
                        "planar mesh preparation stopped".to_string()
                    })
                })
                .map_err(|error| {
                    stopped.unwrap_or_else(|| match error {
                        PreparationError::Budget(_) => Error::Budget,
                        _ => Error::Geometry(error.to_string()),
                    })
                })?;
            expanded += triangles.len();
            if expanded > MAX_INSTANCE_TRIANGLES {
                return Err(Error::Budget);
            }
            if triangles.is_empty() {
                return Ok(None);
            }
            let mesh = self.mesh(triangles, checkpoint)?;
            Ok((!mesh.indices.is_empty()).then_some(mesh))
        };
        let fill = if drawing.fill.is_some() {
            part(None)?
        } else {
            None
        };
        let stroke = if let Some(stroke) = &drawing.stroke {
            part(Some(stroke))?
        } else {
            None
        };
        checkpoint()?;
        Ok(Prepared { fill, stroke })
    }

    fn mesh(
        &mut self,
        triangles: Arc<Vec<Triangle>>,
        checkpoint: &mut dyn FnMut() -> Result<(), Error>,
    ) -> Result<Arc<Mesh>, Error> {
        if let Some(i) = self
            .entries
            .iter()
            .position(|e| Arc::ptr_eq(&e.triangles, &triangles))
        {
            let entry = self.entries.remove(i).unwrap();
            let mesh = entry.mesh.clone();
            self.entries.push_back(entry);
            return Ok(mesh);
        }
        let mut mesh = Mesh {
            positions: vec![],
            normals: vec![],
            indices: vec![],
        };
        let mut vertices = BTreeMap::new();
        for triangle in triangles.iter() {
            checkpoint()?;
            let [a, b, c] = *triangle;
            let cross = (f64::from(b[0]) - f64::from(a[0])) * (f64::from(c[1]) - f64::from(a[1]))
                - (f64::from(b[1]) - f64::from(a[1])) * (f64::from(c[0]) - f64::from(a[0]));
            if cross == 0. {
                continue;
            }
            for p in if cross > 0. { [a, b, c] } else { [a, c, b] } {
                let key = p.map(|v| if v == 0. { 0 } else { v.to_bits() });
                let index = *vertices.entry(key).or_insert_with(|| {
                    let index = mesh.positions.len() as u32;
                    mesh.positions.push([p[0], p[1], 0.]);
                    mesh.normals.push([0., 0., 1.]);
                    index
                });
                mesh.indices.push(index);
            }
        }
        let bytes = triangles.capacity() * std::mem::size_of::<Triangle>()
            + (mesh.positions.capacity() + mesh.normals.capacity()) * 12
            + mesh.indices.capacity() * 4;
        let mesh = Arc::new(mesh);
        if bytes <= CACHE_BYTES {
            while self.bytes + bytes > CACHE_BYTES || self.entries.len() >= 256 {
                self.bytes -= self.entries.pop_front().unwrap().bytes;
            }
            self.bytes += bytes;
            self.entries.push_back(Entry {
                triangles,
                mesh: mesh.clone(),
                bytes,
            });
        }
        Ok(mesh)
    }
}
