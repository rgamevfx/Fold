//! Bounded local-space mesh preparation, independent of document and runtime state.
use super::Mesh;
use std::{ops::Range, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct BoxMesh {
    pub mesh: Arc<Mesh>,
    /// Per-face coordinates, with the same indexing as positions and normals.
    pub uv: Vec<[f32; 2]>,
    /// Index-buffer ranges in fixed +X, -X, +Y, -Y, +Z, -Z order.
    pub faces: [Range<usize>; 6],
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidSize,
}

/// A centered box with full extents `size`, in the caller's declared units.
/// Fixed face/corner slots preserve topology correspondence through size edits.
/// Each face has four distinct vertices so normals and UV seams remain hard.
/// Work and allocation are constant: 24 vertices and 36 indices.
pub fn box_mesh(size: [f32; 3]) -> Result<BoxMesh, Error> {
    if size
        .iter()
        .any(|v| !v.is_finite() || !(1e-6..=1e6).contains(v))
    {
        return Err(Error::InvalidSize);
    }
    let half = size.map(|v| v * 0.5);
    // (normal axis, sign, tangent axis, bitangent axis). Tangent cross bitangent
    // points outward; reversing the tangent handles the negative faces.
    let frames = [
        (0, 1., 1, 2),
        (0, -1., 1, 2),
        (1, 1., 2, 0),
        (1, -1., 2, 0),
        (2, 1., 0, 1),
        (2, -1., 0, 1),
    ];
    let mut positions = Vec::with_capacity(24);
    let mut normals = Vec::with_capacity(24);
    let mut uv = Vec::with_capacity(24);
    let mut indices = Vec::with_capacity(36);
    for (axis, sign, tangent, bitangent) in frames {
        let base = positions.len() as u32;
        let mut normal = [0.; 3];
        normal[axis] = sign;
        for coord in [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
            let mut p = [0.; 3];
            p[axis] = sign * half[axis];
            p[tangent] = (2. * coord[0] - 1.) * sign * half[tangent];
            p[bitangent] = (2. * coord[1] - 1.) * half[bitangent];
            positions.push(p);
            normals.push(normal);
            uv.push(coord);
        }
        indices.extend([base, base + 1, base + 2, base, base + 2, base + 3]);
    }
    Ok(BoxMesh {
        mesh: Arc::new(Mesh {
            positions,
            normals,
            indices,
        }),
        uv,
        faces: std::array::from_fn(|i| i * 6..(i + 1) * 6),
    })
}
