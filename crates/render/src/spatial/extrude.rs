//! Straight extrusion of a conforming, indexed planar cap, including holes.
use super::Mesh;
use fold_media::Cancel;
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::Range,
    sync::Arc,
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    Cancelled,
    InvalidInput,
    NonManifold,
    Budget,
}

/// Index-buffer ranges, retained independently of material assignment.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Regions {
    pub front: Range<usize>,
    pub back: Range<usize>,
    pub sides: Range<usize>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Extrusion {
    pub mesh: Arc<Mesh>,
    pub regions: Regions,
    /// Output vertex → cap vertex, including duplicates at hard normal boundaries.
    /// Consumers can transfer source vertex attributes without matching positions.
    pub source_vertices: Vec<u32>,
}

const MAX_VERTICES: usize = 1_000_000;
const MAX_INDICES: usize = 3_000_000;

/// Front remains at Z=0; back is at -depth, in the cap's local units.
/// Requires a welded, non-overlapping +Z triangulation in XY. Normals are generated;
/// caps are flat and each boundary edge has a hard side normal. `segments` subdivides
/// depth uniformly. Material regions do not assign materials or alter source IDs.
pub fn prepare(cap: &Mesh, depth: f32, segments: u32, cancel: &Cancel) -> Result<Extrusion, Error> {
    prepare_with(cap, depth, segments, &mut || {
        cancel.check().map_err(|_| Error::Cancelled)
    })
}
/// Caller-owned budget/cancellation checks. No partial mesh is returned on failure.
pub fn prepare_with(
    cap: &Mesh,
    depth: f32,
    segments: u32,
    checkpoint: &mut dyn FnMut() -> Result<(), Error>,
) -> Result<Extrusion, Error> {
    checkpoint()?;
    if !depth.is_finite() || !(1e-6..=1e6).contains(&depth) || segments == 0 || segments > 4096 {
        return Err(Error::InvalidInput);
    }
    if cap.positions.len() > MAX_VERTICES / 2 || cap.indices.len() > MAX_INDICES / 2 {
        return Err(Error::Budget);
    }
    if cap.positions.is_empty() || cap.indices.is_empty() || !cap.indices.len().is_multiple_of(3) {
        return Err(Error::InvalidInput);
    }
    let mut points = BTreeSet::new();
    for p in &cap.positions {
        checkpoint()?;
        if p[2] != 0.
            || !p.iter().all(|v| v.is_finite() && v.abs() <= 1e6)
            || !points.insert([p[0], p[1]].map(|v| if v == 0. { 0 } else { v.to_bits() }))
        {
            return Err(Error::InvalidInput);
        }
    }
    // A second use of an edge must have opposite direction; a third is non-manifold.
    let mut edges = BTreeMap::<(u32, u32), (u32, u32, bool)>::new();
    for triangle in cap.indices.chunks_exact(3) {
        checkpoint()?;
        if triangle.iter().any(|i| *i as usize >= cap.positions.len()) {
            return Err(Error::InvalidInput);
        }
        let [a, b, c] = [triangle[0], triangle[1], triangle[2]].map(|i| cap.positions[i as usize]);
        let area = (f64::from(b[0]) - f64::from(a[0])) * (f64::from(c[1]) - f64::from(a[1]))
            - (f64::from(b[1]) - f64::from(a[1])) * (f64::from(c[0]) - f64::from(a[0]));
        if area <= 0. {
            return Err(Error::InvalidInput);
        }
        for (a, b) in [
            (triangle[0], triangle[1]),
            (triangle[1], triangle[2]),
            (triangle[2], triangle[0]),
        ] {
            let key = (a.min(b), a.max(b));
            if let Some((start, end, paired)) = edges.get_mut(&key) {
                if *paired || *start != b || *end != a {
                    return Err(Error::NonManifold);
                }
                *paired = true;
            } else {
                edges.insert(key, (a, b, false));
            }
        }
    }
    let boundary: Vec<_> = edges
        .into_values()
        .filter_map(|(a, b, paired)| (!paired).then_some((a, b)))
        .collect();
    let mut incoming = BTreeSet::new();
    let mut outgoing = BTreeSet::new();
    for &(a, b) in &boundary {
        checkpoint()?;
        if !outgoing.insert(a) || !incoming.insert(b) {
            return Err(Error::NonManifold);
        }
    }
    if boundary.is_empty() || incoming != outgoing {
        return Err(Error::NonManifold);
    }
    let segments = segments as usize;
    let vertex_count = boundary
        .len()
        .checked_mul(2 * (segments + 1))
        .and_then(|sides| sides.checked_add(cap.positions.len() * 2))
        .ok_or(Error::Budget)?;
    let index_count = boundary
        .len()
        .checked_mul(6 * segments)
        .and_then(|sides| sides.checked_add(cap.indices.len() * 2))
        .ok_or(Error::Budget)?;
    if vertex_count > MAX_VERTICES || index_count > MAX_INDICES {
        return Err(Error::Budget);
    }
    let mut mesh = Mesh {
        positions: Vec::with_capacity(vertex_count),
        normals: Vec::with_capacity(vertex_count),
        indices: Vec::with_capacity(index_count),
    };
    let mut source_vertices = Vec::with_capacity(vertex_count);
    source_vertices.extend(0..cap.positions.len() as u32);
    source_vertices.extend(0..cap.positions.len() as u32);
    mesh.positions.extend_from_slice(&cap.positions);
    mesh.normals.resize(cap.positions.len(), [0., 0., 1.]);
    mesh.positions
        .extend(cap.positions.iter().map(|p| [p[0], p[1], -depth]));
    mesh.normals.resize(cap.positions.len() * 2, [0., 0., -1.]);
    mesh.indices.extend_from_slice(&cap.indices);
    let front = 0..mesh.indices.len();
    let offset = cap.positions.len() as u32;
    for t in cap.indices.chunks_exact(3) {
        checkpoint()?;
        mesh.indices
            .extend([t[0] + offset, t[2] + offset, t[1] + offset]);
    }
    let back = front.end..mesh.indices.len();
    for (a, b) in boundary {
        checkpoint()?;
        let sources = [a, b];
        let a = cap.positions[a as usize];
        let b = cap.positions[b as usize];
        let dx = f64::from(b[0]) - f64::from(a[0]);
        let dy = f64::from(b[1]) - f64::from(a[1]);
        let length = dx.hypot(dy);
        let normal = [(dy / length) as f32, (-dx / length) as f32, 0.];
        let start = mesh.positions.len() as u32;
        for step in 0..=segments {
            checkpoint()?;
            let z = -depth * (step as f32 / segments as f32);
            mesh.positions.extend([[a[0], a[1], z], [b[0], b[1], z]]);
            mesh.normals.extend([normal; 2]);
            source_vertices.extend(sources);
        }
        for step in 0..segments as u32 {
            checkpoint()?;
            let top = start + step * 2;
            mesh.indices
                .extend([top, top + 2, top + 3, top, top + 3, top + 1]);
        }
    }
    let sides = back.end..mesh.indices.len();
    checkpoint()?;
    Ok(Extrusion {
        mesh: Arc::new(mesh),
        regions: Regions { front, back, sides },
        source_vertices,
    })
}
