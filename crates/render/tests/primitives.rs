use fold_render::spatial::primitives::{Error, box_mesh};
use std::collections::BTreeMap;

#[test]
fn box_is_closed_outward_and_retains_face_uv_topology_across_size_edits() {
    let reference = box_mesh([0.06; 3]).unwrap();
    for size in [[0.06; 3], [2., 3., 5.], [1e-6, 1e6, 1.]] {
        let prepared = box_mesh(size).unwrap();
        let mesh = &prepared.mesh;
        assert_eq!(mesh.positions.len(), 24);
        assert_eq!(mesh.indices, reference.mesh.indices);
        assert_eq!(mesh.normals, reference.mesh.normals);
        assert_eq!(prepared.faces, reference.faces);
        assert_eq!(prepared.uv, reference.uv);
        let mut edges = BTreeMap::new();
        let mut volume = 0.;
        for triangle in mesh.indices.chunks_exact(3) {
            let p = [triangle[0], triangle[1], triangle[2]]
                .map(|i| mesh.positions[i as usize].map(f64::from));
            let a = [0, 1, 2].map(|i| p[1][i] - p[0][i]);
            let b = [0, 1, 2].map(|i| p[2][i] - p[0][i]);
            let cross = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            let normal = mesh.normals[triangle[0] as usize].map(f64::from);
            assert!((0..3).map(|i| cross[i] * normal[i]).sum::<f64>() > 0.);
            volume += (0..3).map(|i| p[0][i] * cross[i]).sum::<f64>() / 6.;
            for (a, b) in [(0, 1), (1, 2), (2, 0)] {
                let a = mesh.positions[triangle[a] as usize].map(f32::to_bits);
                let b = mesh.positions[triangle[b] as usize].map(f32::to_bits);
                let entry = edges.entry((a.min(b), a.max(b))).or_insert((0, 0));
                entry.0 += 1;
                entry.1 += if a < b { 1 } else { -1 };
            }
        }
        assert!(edges.values().all(|v| *v == (2, 0)));
        let expected = size.into_iter().map(f64::from).product::<f64>();
        assert!((volume - expected).abs() <= expected * 1e-6);
        for (face, range) in prepared.faces.iter().enumerate() {
            assert_eq!(range.len(), 6);
            for &index in &mesh.indices[range.clone()] {
                assert_eq!(index as usize / 4, face);
                let p = mesh.positions[index as usize];
                assert_eq!(p.map(f32::abs), size.map(|v| v * 0.5));
            }
            assert_eq!(
                &prepared.uv[face * 4..face * 4 + 4],
                &[[0., 0.], [1., 0.], [1., 1.], [0., 1.]]
            );
        }
    }
}

#[test]
fn box_rejects_degenerate_nonfinite_and_out_of_range_sizes() {
    for invalid in [
        0.,
        -1.,
        f32::NAN,
        f32::INFINITY,
        f32::NEG_INFINITY,
        1e-7,
        1e7,
    ] {
        for axis in 0..3 {
            let mut size = [0.06; 3];
            size[axis] = invalid;
            assert_eq!(box_mesh(size), Err(Error::InvalidSize));
        }
    }
}
