use fold_media::Cancel;
use fold_render::spatial::extrude::{Error, prepare};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};

fn caps() -> fold_render::text::mesh::Prepared {
    let layout = fold_render::text::Service::new(
        include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
        "Noto Sans",
    )
    .unwrap()
    .layout("OB8", 140., None)
    .unwrap();
    let outlines =
        Arc::new(fold_render::text::outlines::prepare(layout, &Cancel::default()).unwrap());
    fold_render::text::mesh::Cache::default()
        .prepare(outlines, 1., &Cancel::default())
        .unwrap()
}
fn key(p: [f32; 3]) -> [u32; 3] {
    p.map(|v| if v == 0. { 0 } else { v.to_bits() })
}

#[test]
fn ob8_extrusions_are_closed_oriented_solids_with_expected_holes_and_volume() {
    let prepared = caps();
    for (glyph, euler) in prepared.glyphs.iter().zip([0, -2, -2]) {
        let cap = glyph.mesh.as_ref().unwrap();
        for segments in [1, 3] {
            let extrusion = prepare(cap, 0.15, segments, &Cancel::default()).unwrap();
            let mesh = &extrusion.mesh;
            assert_eq!(extrusion.source_vertices.len(), mesh.positions.len());
            for (position, source) in mesh.positions.iter().zip(&extrusion.source_vertices) {
                assert_eq!(&position[..2], &cap.positions[*source as usize][..2]);
            }
            assert_eq!(extrusion.regions.front, 0..cap.indices.len());
            assert_eq!(
                extrusion.regions.back,
                cap.indices.len()..cap.indices.len() * 2
            );
            assert_eq!(extrusion.regions.sides.end, mesh.indices.len());
            let mut edges = BTreeMap::<([u32; 3], [u32; 3]), (usize, i32)>::new();
            let mut vertices = BTreeSet::new();
            let mut volume = 0.;
            for triangle in mesh.indices.chunks_exact(3) {
                let p = [triangle[0], triangle[1], triangle[2]]
                    .map(|i| mesh.positions[i as usize].map(f64::from));
                let ab = [0, 1, 2].map(|i| p[1][i] - p[0][i]);
                let ac = [0, 1, 2].map(|i| p[2][i] - p[0][i]);
                let cross = [
                    ab[1] * ac[2] - ab[2] * ac[1],
                    ab[2] * ac[0] - ab[0] * ac[2],
                    ab[0] * ac[1] - ab[1] * ac[0],
                ];
                let normal = mesh.normals[triangle[0] as usize].map(f64::from);
                assert!((0..3).map(|i| cross[i] * normal[i]).sum::<f64>() > 0.);
                volume += (0..3).map(|i| p[0][i] * cross[i]).sum::<f64>() / 6.;
                for (a, b) in [
                    (triangle[0], triangle[1]),
                    (triangle[1], triangle[2]),
                    (triangle[2], triangle[0]),
                ] {
                    let a = key(mesh.positions[a as usize]);
                    let b = key(mesh.positions[b as usize]);
                    vertices.insert(a);
                    vertices.insert(b);
                    let entry = edges.entry((a.min(b), a.max(b))).or_default();
                    entry.0 += 1;
                    entry.1 += if a < b { 1 } else { -1 };
                }
            }
            assert!(
                edges.values().all(|e| *e == (2, 0)),
                "every solid edge has two opposite faces"
            );
            assert_eq!(
                vertices.len() as i64 - edges.len() as i64 + (mesh.indices.len() / 3) as i64,
                euler
            );
            let area: f64 = cap
                .indices
                .chunks_exact(3)
                .map(|t| {
                    let [a, b, c] =
                        [t[0], t[1], t[2]].map(|i| cap.positions[i as usize].map(f64::from));
                    ((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])) / 2.
                })
                .sum();
            assert!((volume - area * f64::from(0.15f32)).abs() < area * 1e-6);
        }
    }
}

#[test]
fn extrusion_rejects_invalid_topology_depth_budget_and_cancel() {
    let cap = caps().glyphs[0].mesh.clone().unwrap();
    for depth in [0., -1., f32::NAN, f32::INFINITY] {
        assert!(matches!(
            prepare(&cap, depth, 1, &Cancel::default()),
            Err(Error::InvalidInput)
        ));
    }
    assert!(matches!(
        prepare(&cap, 1., 0, &Cancel::default()),
        Err(Error::InvalidInput)
    ));
    assert!(matches!(
        prepare(&cap, 1., 4096, &Cancel::default()),
        Err(Error::Budget)
    ));
    let mut duplicate = (*cap).clone();
    duplicate.indices.extend_from_within(0..3);
    assert!(matches!(
        prepare(&duplicate, 1., 1, &Cancel::default()),
        Err(Error::NonManifold)
    ));
    let mut invalid = (*cap).clone();
    invalid.positions[0][2] = 1.;
    assert!(matches!(
        prepare(&invalid, 1., 1, &Cancel::default()),
        Err(Error::InvalidInput)
    ));
    let cancel = Cancel::default();
    cancel.cancel();
    assert!(matches!(
        prepare(&cap, 1., 1, &cancel),
        Err(Error::Cancelled)
    ));
}
