//! Bounded local-space mesh preparation, independent of document and runtime state.
use fold_render::spatial::Mesh;
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

/// A fixed-resolution UV sphere. Radius edits preserve topology slots; seam
/// vertices carry separate UVs and identical smooth normals. Pole triangles are
/// emitted once, avoiding zero-area faces. Maximum work is 561 vertices/960 faces.
pub fn sphere_mesh(radius: f32) -> Result<(Arc<Mesh>, Vec<[f32; 2]>), Error> {
    if !radius.is_finite() || !(1e-6..=1e6).contains(&radius) {
        return Err(Error::InvalidSize);
    }
    const LONGITUDES: usize = 32;
    const LATITUDES: usize = 16;
    let mut positions = Vec::with_capacity((LONGITUDES + 1) * (LATITUDES + 1));
    let mut normals = Vec::with_capacity(positions.capacity());
    let mut uv = Vec::with_capacity(positions.capacity());
    let mut indices = Vec::with_capacity(6 * LONGITUDES * (LATITUDES - 1));
    for latitude in 0..=LATITUDES {
        let v = latitude as f32 / LATITUDES as f32;
        let theta = v * std::f32::consts::PI;
        for longitude in 0..=LONGITUDES {
            let u = longitude as f32 / LONGITUDES as f32;
            let phi = if longitude == LONGITUDES {
                0.
            } else {
                u * std::f32::consts::TAU
            };
            let normal = match latitude {
                0 => [0., 1., 0.],
                LATITUDES => [0., -1., 0.],
                _ => [
                    theta.sin() * phi.cos(),
                    theta.cos(),
                    theta.sin() * phi.sin(),
                ],
            };
            positions.push(normal.map(|value| value * radius));
            normals.push(normal);
            uv.push([u, 1. - v]);
        }
    }
    for latitude in 0..LATITUDES {
        for longitude in 0..LONGITUDES {
            let a = (latitude * (LONGITUDES + 1) + longitude) as u32;
            let b = a + LONGITUDES as u32 + 1;
            if latitude != 0 {
                indices.extend([a, a + 1, b + 1]);
            }
            if latitude != LATITUDES - 1 {
                indices.extend([a, b + 1, b]);
            }
        }
    }
    Ok((
        Arc::new(Mesh {
            positions,
            normals,
            indices,
        }),
        uv,
    ))
}

#[cfg(test)]
mod sphere_tests {
    use super::*;
    #[test]
    fn sphere_has_smooth_unit_normals_outward_faces_and_stable_radius_topology() {
        let (mesh, uv) = sphere_mesh(2.).unwrap();
        let (resized, resized_uv) = sphere_mesh(3.).unwrap();
        assert_eq!(mesh.positions.len(), 561);
        assert_eq!(mesh.indices.len(), 2880);
        assert_eq!(mesh.indices, resized.indices);
        assert_eq!(mesh.normals, resized.normals);
        assert_eq!(uv, resized_uv);
        for (position, normal) in mesh.positions.iter().zip(&mesh.normals) {
            assert!((normal.iter().map(|x| x * x).sum::<f32>() - 1.).abs() < 1e-6);
            assert_eq!(*position, normal.map(|x| 2. * x));
        }
        for triangle in mesh.indices.chunks_exact(3) {
            let [a, b, c] =
                std::array::from_fn::<_, 3, _>(|i| mesh.positions[triangle[i] as usize]);
            let u: [f32; 3] = std::array::from_fn(|i| b[i] - a[i]);
            let v: [f32; 3] = std::array::from_fn(|i| c[i] - a[i]);
            let cross = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            assert!(cross.iter().zip(a).map(|(n, p)| n * p).sum::<f32>() > 0.);
        }
        assert!(sphere_mesh(0.).is_err());
        assert!(sphere_mesh(f32::NAN).is_err());
    }
}
