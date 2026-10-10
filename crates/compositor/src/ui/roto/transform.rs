//! Selection transforms are pure geometry. All coordinates remain full-format
//! pixels; viewport pan/zoom is only used at the input/output edge.
use crate::roto::{Point, add, mul, sub};
pub(super) fn bounds(points: impl Iterator<Item = [f64; 2]>) -> Option<[f64; 4]> {
    let mut bounds = [
        f64::INFINITY,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NEG_INFINITY,
    ];
    let mut any = false;
    for p in points {
        any = true;
        for a in 0..2 {
            bounds[a] = bounds[a].min(p[a]);
            bounds[a + 2] = bounds[a + 2].max(p[a]);
        }
    }
    any.then_some(bounds)
}
pub(super) fn corners(b: [f64; 4]) -> [[f64; 2]; 4] {
    [[b[0], b[1]], [b[2], b[1]], [b[2], b[3]], [b[0], b[3]]]
}
/// Homography from the unit square to a convex quadrilateral. Reject a folded
/// quad or horizon crossing rather than producing unstable point coordinates.
pub(super) fn project(b: [f64; 4], q: [[f64; 2]; 4], p: [f64; 2]) -> Result<[f64; 2], String> {
    if b[2] - b[0] < 1e-8 || b[3] - b[1] < 1e-8 {
        return Err("Corner-pin requires a nonzero selection box".into());
    }
    let cross = |a: [f64; 2], b: [f64; 2]| a[0] * b[1] - a[1] * b[0];
    let signs: [f64; 4] = std::array::from_fn(|i| {
        cross(
            sub(q[(i + 1) % 4], q[i]),
            sub(q[(i + 2) % 4], q[(i + 1) % 4]),
        )
    });
    if signs
        .iter()
        .any(|s| s.abs() < 1e-8 || s.signum() != signs[0].signum())
    {
        return Err("Corner-pin cannot fold the selection".into());
    }
    let dx1 = q[1][0] - q[2][0];
    let dx2 = q[3][0] - q[2][0];
    let dx3 = q[0][0] - q[1][0] + q[2][0] - q[3][0];
    let dy1 = q[1][1] - q[2][1];
    let dy2 = q[3][1] - q[2][1];
    let dy3 = q[0][1] - q[1][1] + q[2][1] - q[3][1];
    let determinant = dx1 * dy2 - dx2 * dy1;
    if determinant.abs() < 1e-10 {
        return Err("Corner-pin is singular".into());
    }
    let g = (dx3 * dy2 - dx2 * dy3) / determinant;
    let h = (dx1 * dy3 - dx3 * dy1) / determinant;
    let u = (p[0] - b[0]) / (b[2] - b[0]);
    let v = (p[1] - b[1]) / (b[3] - b[1]);
    let w = g * u + h * v + 1.;
    if w <= 1e-6 {
        return Err("Corner-pin crosses the projection horizon".into());
    }
    Ok([
        ((q[1][0] - q[0][0] + g * q[1][0]) * u + (q[3][0] - q[0][0] + h * q[3][0]) * v + q[0][0])
            / w,
        ((q[1][1] - q[0][1] + g * q[1][1]) * u + (q[3][1] - q[0][1] + h * q[3][1]) * v + q[0][1])
            / w,
    ])
}
pub(super) fn point(
    p: &mut Point,
    map: impl Fn([f64; 2]) -> Result<[f64; 2], String>,
) -> Result<(), String> {
    let position = map(p.position)?;
    let incoming = sub(map(add(p.position, p.incoming))?, position);
    let outgoing = sub(map(add(p.position, p.outgoing))?, position);
    let feather = sub(map(add(p.position, p.feather))?, position);
    p.position = position;
    p.incoming = incoming;
    p.outgoing = outgoing;
    p.feather = feather;
    Ok(())
}
pub(super) fn rotate(p: [f64; 2], center: [f64; 2], angle: f64) -> [f64; 2] {
    let v = sub(p, center);
    let (s, c) = angle.sin_cos();
    add(center, [c * v[0] - s * v[1], s * v[0] + c * v[1]])
}
pub(super) fn distance(p: [f64; 2], a: [f64; 2], b: [f64; 2]) -> f64 {
    let d = sub(b, a);
    let l = d[0] * d[0] + d[1] * d[1];
    let v = sub(p, a);
    let t = if l > 0. {
        (v[0] * d[0] + v[1] * d[1]) / l
    } else {
        0.
    };
    let v = sub(p, add(a, mul(d, t.clamp(0., 1.))));
    v[0].hypot(v[1])
}
