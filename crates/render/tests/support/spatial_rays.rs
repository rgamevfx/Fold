use fold_render::gpu::spatial::{Binding, Frame, Scene};

// Independent f64 ray/triangle reference for the axis-aligned fixture cameras.
// Checks perspective-visible walls as well as both caps, without matching Bevy raster code.
pub fn check_extrusion_rays(
    scene: &Scene,
    binding: &Binding,
    frame: &mut Frame,
    cancel: &fold_media::Cancel,
) {
    fn sub(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [0, 1, 2].map(|i| a[i] - b[i])
    }
    fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
        [
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        ]
    }
    fn dot(a: [f64; 3], b: [f64; 3]) -> f64 {
        (0..3).map(|i| a[i] * b[i]).sum()
    }
    let mut triangles = vec![];
    for instance in &scene.instances {
        assert_eq!(instance.transform.rotation, [0., 0., 0., 1.]);
        let mesh = &scene.meshes[&instance.mesh];
        for t in mesh.indices.chunks_exact(3) {
            triangles.push([t[0], t[1], t[2]].map(|i| {
                [0, 1, 2].map(|axis| {
                    f64::from(mesh.positions[i as usize][axis])
                        * f64::from(instance.transform.scale[axis])
                        + f64::from(instance.transform.translation[axis])
                })
            }));
        }
    }
    let pixels = frame.readback(binding, cancel).unwrap();
    let depth = frame.read_depth(binding, cancel).unwrap();
    let origin = scene.camera.position.map(f64::from);
    let forward = if origin[2] > 0. { -1. } else { 1. };
    let fold_render::spatial::Projection::Perspective { vertical_fov } = scene.camera.projection
    else {
        panic!("perspective oracle required")
    };
    let scale = 2. * (f64::from(vertical_fov) / 2.).tan() / 220.;
    // Conservative projected bounds keep the independent ray oracle inexpensive.
    let triangles: Vec<_> = triangles
        .into_iter()
        .map(|triangle| {
            let projected = triangle.map(|p| {
                let distance = (p[2] - origin[2]) * forward;
                assert!(distance > 0.);
                [
                    320. - forward * p[0] / distance / scale,
                    110. - p[1] / distance / scale,
                ]
            });
            let min = [0, 1].map(|axis| {
                projected
                    .iter()
                    .map(|p| p[axis])
                    .fold(f64::INFINITY, f64::min)
            });
            let max = [0, 1].map(|axis| {
                projected
                    .iter()
                    .map(|p| p[axis])
                    .fold(f64::NEG_INFINITY, f64::max)
            });
            (triangle, projected, min, max)
        })
        .collect();
    let mut caps = 0;
    let mut sides = 0;
    let mut clear = 0;
    let mut edge_samples = 0;
    // Raster subpixel rounding can disagree with exact ray coverage at boundaries.
    const EDGE_BAND: f64 = 1. / 128.;
    for y in (2..218).step_by(3) {
        for x in (2..638).step_by(3) {
            let ray = [
                -forward * (x as f64 + 0.5 - 320.) * scale,
                (110. - y as f64 - 0.5) * scale,
                forward,
            ];
            let mut nearest = f64::INFINITY;
            let mut edge = false;
            let mut side = false;
            for &([a, b, c], projected, min, max) in &triangles {
                if x as f64 + 0.5 < min[0] - EDGE_BAND
                    || x as f64 + 0.5 > max[0] + EDGE_BAND
                    || y as f64 + 0.5 < min[1] - EDGE_BAND
                    || y as f64 + 0.5 > max[1] + EDGE_BAND
                {
                    continue;
                }
                let point = [x as f64 + 0.5, y as f64 + 0.5];
                for i in 0..3 {
                    let a = projected[i];
                    let b = projected[(i + 1) % 3];
                    let d = [b[0] - a[0], b[1] - a[1]];
                    let length = d[0] * d[0] + d[1] * d[1];
                    let t = if length == 0. {
                        0.
                    } else {
                        ((point[0] - a[0]) * d[0] + (point[1] - a[1]) * d[1]) / length
                    }
                    .clamp(0., 1.);
                    let distance = (point[0] - a[0] - t * d[0]).hypot(point[1] - a[1] - t * d[1]);
                    if distance < EDGE_BAND {
                        edge = true;
                        break;
                    }
                }
                if edge {
                    break;
                }
                let e1 = sub(b, a);
                let e2 = sub(c, a);
                let h = cross(ray, e2);
                let det = dot(e1, h);
                if det.abs() < 1e-12 {
                    continue;
                }
                let s = sub(origin, a);
                let u = dot(s, h) / det;
                if !(0. ..=1.).contains(&u) {
                    continue;
                }
                let q = cross(s, e1);
                let v = dot(ray, q) / det;
                if v < 0. || u + v > 1. {
                    continue;
                }
                let distance = dot(e2, q) / det;
                if distance > 0. && distance < nearest {
                    nearest = distance;
                    side = e1[2] != 0. || e2[2] != 0.;
                }
            }
            if edge {
                edge_samples += 1;
                continue;
            }
            let index = y * 640 + x;
            if nearest.is_finite() {
                assert_eq!(pixels.pixels()[index], [1.; 4], "ray coverage at {x},{y}");
                let actual = f64::from(depth.view_distance(x as u32, y as u32).unwrap());
                assert!(
                    (actual - nearest).abs() < 2e-4,
                    "ray depth at {x},{y}: {actual} vs {nearest}"
                );
                if side {
                    sides += 1;
                } else {
                    caps += 1;
                }
            } else {
                assert_eq!(pixels.pixels()[index], [0.; 4], "clear ray at {x},{y}");
                assert_eq!(depth.values()[index], 0.);
                clear += 1;
            }
        }
    }
    assert!(caps > 500 && sides > 10 && clear > 1000);
    assert!(edge_samples < 500);
    eprintln!(
        "extrusion ray reference, camera_z={}: {caps} cap, {sides} side and {clear} clear rays match color/depth (2e-4 view-unit tolerance); {edge_samples} samples excluded within 1/128 pixel of projected triangle edges",
        scene.camera.position[2]
    );
}
