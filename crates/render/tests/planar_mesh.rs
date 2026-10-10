use fold_render::{
    spatial::{
        Mesh,
        planar::{Cache, Error},
    },
    vector::{Cap, Drawing, Join, Segment, Stroke},
};
use std::sync::Arc;

fn drawing() -> Drawing {
    let mut path = vec![];
    for (a, b) in [(10., 30.), (15., 25.)] {
        path.extend([
            Segment::Move([a, a]),
            Segment::Line([b, a]),
            Segment::Line([b, b]),
            Segment::Line([a, b]),
            Segment::Close,
        ]);
    }
    Drawing {
        feather: None,
        path: Arc::new(path),
        transform: [1., 0., 0., 1., 0., 0.],
        fill: Some([1.; 4]),
        even_odd: true,
        stroke: Some(Stroke {
            color: [1.; 4],
            width: 2.,
            cap: Cap::Butt,
            join: Join::Miter,
        }),
    }
}
fn contains(mesh: &Mesh, p: [f32; 2]) -> bool {
    mesh.indices.chunks_exact(3).any(|ids| {
        let t = [0, 1, 2].map(|i| mesh.positions[ids[i] as usize]);
        [0, 1, 2].into_iter().all(|i| {
            let a = t[i];
            let b = t[(i + 1) % 3];
            (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]) >= 0.
        })
    })
}
#[test]
fn planar_fill_holes_and_stroke_match_independent_rectangles() {
    let mut cache = Cache::default();
    for even_odd in [true, false] {
        let mut drawing = drawing();
        drawing.even_odd = even_odd;
        let prepared = cache.prepare_with(&drawing, 1., &mut || Ok(())).unwrap();
        let fill = prepared.fill.unwrap();
        let stroke = prepared.stroke.unwrap();
        for mesh in [&fill, &stroke] {
            assert!(mesh.normals.iter().all(|n| *n == [0., 0., 1.]));
            for ids in mesh.indices.chunks_exact(3) {
                let [a, b, c] = [0, 1, 2].map(|i| mesh.positions[ids[i] as usize]);
                assert!((b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0]) > 0.);
            }
        }
        for y in 0..40 {
            for x in 0..40 {
                let p = [x as f32 + 0.5, y as f32 + 0.5];
                let inside = |a, b| p.iter().all(|v| *v > a && *v < b);
                assert_eq!(
                    contains(&fill, p),
                    inside(10., 30.) && (!even_odd || !inside(15., 25.))
                );
                assert_eq!(
                    contains(&stroke, p),
                    (inside(9., 31.) && !inside(11., 29.))
                        || (inside(14., 26.) && !inside(16., 24.))
                );
            }
        }
    }
}
#[test]
fn planar_cache_separates_geometry_from_paint_and_placement() {
    let mut cache = Cache::default();
    let mut drawing = drawing();
    let initial = cache.prepare_with(&drawing, 1., &mut || Ok(())).unwrap();
    drawing.fill = Some([-2., 8., 0., 0.5]);
    drawing.stroke.as_mut().unwrap().color = [0., 4., 2., 0.25];
    drawing.transform = [2., 0., 0., -2., 45., 90.];
    let repainted = cache.prepare_with(&drawing, 1., &mut || Ok(())).unwrap();
    assert!(Arc::ptr_eq(
        initial.fill.as_ref().unwrap(),
        repainted.fill.as_ref().unwrap()
    ));
    assert!(Arc::ptr_eq(
        initial.stroke.as_ref().unwrap(),
        repainted.stroke.as_ref().unwrap()
    ));
    drawing.stroke.as_mut().unwrap().width = 4.;
    let wider = cache.prepare_with(&drawing, 1., &mut || Ok(())).unwrap();
    assert!(Arc::ptr_eq(
        initial.fill.as_ref().unwrap(),
        wider.fill.as_ref().unwrap()
    ));
    assert!(!Arc::ptr_eq(
        initial.stroke.as_ref().unwrap(),
        wider.stroke.as_ref().unwrap()
    ));
    drawing.fill = None;
    drawing.stroke = None;
    let empty = cache.prepare_with(&drawing, 1., &mut || Ok(())).unwrap();
    assert!(empty.fill.is_none() && empty.stroke.is_none());
    drop(cache);
    assert!(contains(initial.fill.as_ref().unwrap(), [12., 12.]));
}
#[test]
fn planar_failures_preserve_typed_stops_and_retry() {
    let drawing = drawing();
    let mut count = 0;
    Cache::default()
        .prepare_with(&drawing, 1., &mut || {
            count += 1;
            Ok(())
        })
        .unwrap();
    for at in [1, count / 2, count] {
        for error in [Error::Cancelled, Error::Budget] {
            let mut cache = Cache::default();
            let mut seen = 0;
            assert_eq!(
                cache
                    .prepare_with(&drawing, 1., &mut || {
                        seen += 1;
                        if seen == at {
                            Err(error.clone())
                        } else {
                            Ok(())
                        }
                    })
                    .unwrap_err(),
                error
            );
            assert_eq!(seen, at);
            assert!(cache.prepare_with(&drawing, 1., &mut || Ok(())).is_ok());
        }
    }
    for scale in [0., -1., f64::NAN, f64::INFINITY] {
        assert_eq!(
            Cache::default()
                .prepare_with(&drawing, scale, &mut || Ok(()))
                .unwrap_err(),
            Error::InvalidInput
        );
    }
    let mut invalid = drawing;
    invalid.stroke.as_mut().unwrap().width = -1.;
    assert!(matches!(
        Cache::default().prepare_with(&invalid, 1., &mut || Ok(())),
        Err(Error::Geometry(_))
    ));
}

#[test]
fn disjoint_paint_regions_preserve_holes_and_never_overlap() {
    for even_odd in [true, false] {
        let mut cache = Cache::default();
        let mut drawing = drawing();
        drawing.even_odd = even_odd;
        let prepared = cache.prepare_with(&drawing, 1., &mut || Ok(())).unwrap();
        let regions = cache
            .disjoint_with(prepared.clone(), &mut || Ok(()))
            .unwrap();
        for y in 0..40 {
            for x in 0..40 {
                // Avoid the rectangle diagonals as well as pixel edge coincidences.
                let p = [x as f32 + 0.37, y as f32 + 0.61];
                let fill = contains(prepared.fill.as_ref().unwrap(), p);
                let stroke = contains(prepared.stroke.as_ref().unwrap(), p);
                let present =
                    |mesh: &Option<Arc<Mesh>>| mesh.as_ref().is_some_and(|m| contains(m, p));
                assert_eq!(
                    [
                        present(&regions.fill),
                        present(&regions.stroke),
                        present(&regions.overlap)
                    ],
                    [fill && !stroke, stroke && !fill, fill && stroke],
                    "{p:?}"
                );
            }
        }
        let mut count = 0;
        Cache::default()
            .disjoint_with(prepared.clone(), &mut || {
                count += 1;
                Ok(())
            })
            .unwrap();
        for at in [1, count / 2, count] {
            let mut seen = 0;
            assert!(matches!(
                Cache::default().disjoint_with(prepared.clone(), &mut || {
                    seen += 1;
                    if seen == at {
                        Err(Error::Cancelled)
                    } else {
                        Ok(())
                    }
                }),
                Err(Error::Cancelled)
            ));
            assert_eq!(seen, at);
        }
        let mut cached_work = 0;
        let reused = cache
            .disjoint_with(prepared, &mut || {
                cached_work += 1;
                Ok(())
            })
            .unwrap();
        assert!(cached_work < count);
        for (a, b) in [
            (&regions.fill, &reused.fill),
            (&regions.stroke, &reused.stroke),
            (&regions.overlap, &reused.overlap),
        ] {
            assert!(Arc::ptr_eq(a.as_ref().unwrap(), b.as_ref().unwrap()));
        }
        drop(cache);
        assert!(contains(reused.overlap.as_ref().unwrap(), [10.5, 10.5]));
    }
}

#[test]
fn typography_partitions_reuse_repeated_glyphs_with_bounded_work() {
    let layout = fold_render::text::Service::new(
        include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
        "Noto Sans",
    )
    .unwrap()
    .layout("O B 8 O", 140., None)
    .unwrap();
    let outlines =
        fold_render::text::outlines::prepare(layout, &fold_media::Cancel::default()).unwrap();
    let drawings = outlines.drawings(
        Some([1., 0., 0., 0.5]),
        Some(Stroke {
            color: [0., 1., 0., 0.5],
            width: 4.,
            cap: Cap::Butt,
            join: Join::Miter,
        }),
    );
    let mut cache = Cache::default();
    let mut total = 0;
    let mut first = None;
    for (index, drawing) in drawings.iter().enumerate() {
        let [a, b, c, d, _, _] = drawing.transform;
        let mut work = 0;
        let mut step = || {
            work += 1;
            Ok(())
        };
        let prepared = cache
            .prepare_with(
                drawing,
                (a.abs() + c.abs()).max(b.abs() + d.abs()),
                &mut step,
            )
            .unwrap();
        let partition = cache.disjoint_with(prepared, &mut step).unwrap();
        total += work;
        eprintln!("glyph {index}: {work} preparation checkpoints");
        if index == 0 {
            first = partition.overlap;
        } else if index == 6 {
            assert!(Arc::ptr_eq(
                first.as_ref().unwrap(),
                partition.overlap.as_ref().unwrap()
            ));
        }
    }
    eprintln!("typography total: {total} preparation checkpoints");
    assert!(total < 1_000_000);
}
