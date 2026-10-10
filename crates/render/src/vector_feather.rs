//! Corresponding native contours form a coverage ribbon. Its vertex coverage is
//! integrated analytically over each pixel by the shared vector CPU/GPU path.
//! Hard interiors and soft edges share one hierarchy, upload and drawing.
use crate::{
    vector::{Drawing, Segment},
    vector_geometry::{MAX_TRIANGLES, PreparationError, Triangle},
};
use fold_media::Cancel;
use std::sync::Arc;
#[derive(Clone, Debug, PartialEq)]
pub struct Feather {
    pub path: Arc<Vec<Segment>>,
    pub falloff: f64,
}
pub fn drawings(
    inner: Vec<Segment>,
    outer: Vec<Segment>,
    transform: [f64; 6],
    falloff: f64,
    cancel: &Cancel,
) -> Result<Vec<Drawing>, String> {
    cancel.check()?;
    if inner.len() != outer.len() {
        return Err("Feather contour topology differs".into());
    }
    let feather = (inner != outer).then(|| {
        Arc::new(Feather {
            path: Arc::new(outer),
            falloff,
        })
    });
    let drawings = vec![Drawing {
        path: Arc::new(inner),
        transform,
        fill: Some([1.; 4]),
        stroke: None,
        even_odd: false,
        feather,
    }];
    crate::vector::validate_working(&drawings, true)?;
    Ok(drawings)
}
type Point = [f64; 2];
type Cubic = [Point; 4];
fn lerp(a: Point, b: Point, t: f64) -> Point {
    [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])]
}
fn line(a: Point, b: Point) -> Cubic {
    [a, lerp(a, b, 1. / 3.), lerp(a, b, 2. / 3.), b]
}
fn split(c: Cubic) -> [Cubic; 2] {
    let a = lerp(c[0], c[1], 0.5);
    let b = lerp(c[1], c[2], 0.5);
    let d = lerp(c[2], c[3], 0.5);
    let e = lerp(a, b, 0.5);
    let f = lerp(b, d, 0.5);
    let m = lerp(e, f, 0.5);
    [[c[0], a, e, m], [m, f, d, c[3]]]
}
fn flat(c: Cubic, tolerance: f64) -> bool {
    // Control-polygon distance to the parameter-matched chord also catches
    // collinear reversals; both contours use identical subdivision parameters.
    (1..3).all(|i| {
        let p = lerp(c[0], c[3], i as f64 / 3.);
        (p[0] - c[i][0]).hypot(p[1] - c[i][1]) <= tolerance
    })
}
// Three-point Gaussian quadrature integrates the degree-four signed area
// integrand of a cubic exactly. Winding also determines whether a ribbon adds
// outside coverage or removes coverage for an inward feather.
fn orientation(path: &[Segment]) -> f64 {
    let mut current = [0.; 2];
    let mut first = current;
    let mut area = 0.;
    for segment in path {
        let c = match segment {
            Segment::Move(p) => {
                current = *p;
                first = *p;
                continue;
            }
            Segment::Line(p) => line(current, *p),
            Segment::Quad(p, q) => [
                current,
                lerp(current, *p, 2. / 3.),
                lerp(*q, *p, 2. / 3.),
                *q,
            ],
            Segment::Cubic(p, q, r) => [current, *p, *q, *r],
            Segment::Close => line(current, first),
        };
        for (t, weight) in [
            (0.5 - (0.6f64).sqrt() / 2., 5. / 18.),
            (0.5, 4. / 9.),
            (0.5 + (0.6f64).sqrt() / 2., 5. / 18.),
        ] {
            let a = lerp(c[0], c[1], t);
            let b = lerp(c[1], c[2], t);
            let d = lerp(c[2], c[3], t);
            let e = lerp(a, b, t);
            let f = lerp(b, d, t);
            let p = lerp(e, f, t);
            area += weight * 3. * (p[0] * (f[1] - e[1]) - p[1] * (f[0] - e[0]));
        }
        current = c[3];
    }
    area.signum()
}
struct Ribbon<'a> {
    triangles: &'a mut Vec<Triangle>,
    knots: Vec<(f64, f32)>,
    tolerance: f64,
    orientation: f64,
    checkpoint: &'a mut dyn FnMut() -> Result<(), String>,
    work: usize,
}
impl Ribbon<'_> {
    fn curves(&mut self, inner: Cubic, outer: Cubic, depth: usize) -> Result<(), PreparationError> {
        (self.checkpoint)().map_err(PreparationError::Stopped)?;
        self.work += 1;
        if self.work > MAX_TRIANGLES || depth > 24 {
            return Err(PreparationError::Budget(
                "Feather subdivision budget exceeded",
            ));
        }
        if !flat(inner, self.tolerance) || !flat(outer, self.tolerance) {
            let a = split(inner);
            let b = split(outer);
            self.curves(a[0], b[0], depth + 1)?;
            self.curves(a[1], b[1], depth + 1)?;
            return Ok(());
        }
        for pair in self.knots.windows(2) {
            let [(t0, a), (t1, b)] = [pair[0], pair[1]];
            let p = [
                lerp(inner[0], outer[0], t0),
                lerp(inner[3], outer[3], t0),
                lerp(inner[3], outer[3], t1),
                lerp(inner[0], outer[0], t1),
            ];
            for (indices, weights) in [([0, 1, 2], [a, a, b]), ([0, 2, 3], [a, b, b])] {
                if self.triangles.len() >= MAX_TRIANGLES {
                    return Err(PreparationError::Budget("Feather triangle budget exceeded"));
                }
                let points = indices.map(|i| p[i].map(|v| v as f32));
                let [u, v, w] = indices.map(|i| p[i]);
                let cross = (v[0] - u[0]) * (w[1] - u[1]) - (v[1] - u[1]) * (w[0] - u[0]);
                let sign = (-cross.signum() * self.orientation) as f32;
                self.triangles
                    .push(Triangle::weighted(points, weights.map(|v| v * sign)));
            }
        }
        Ok(())
    }
}
pub(crate) fn append(
    inner: &[Segment],
    feather: &Feather,
    tolerance: f64,
    triangles: &mut Vec<Triangle>,
    checkpoint: &mut dyn FnMut() -> Result<(), String>,
) -> Result<(), PreparationError> {
    fn knots(a: f64, b: f64, power: f64, depth: usize, result: &mut Vec<(f64, f32)>) {
        let f = |t: f64| (1. - t).powf(power);
        let middle = (a + b) * 0.5;
        if depth < 10 && (f(middle) - (f(a) + f(b)) * 0.5).abs() > 1. / 1024. {
            knots(a, middle, power, depth + 1, result);
            knots(middle, b, power, depth + 1, result);
        } else {
            result.push((b, f(b) as f32));
        }
    }
    let mut levels = vec![(0., 1.)];
    knots(0., 1., feather.falloff, 0, &mut levels);
    let mut ribbon = Ribbon {
        triangles,
        knots: levels,
        tolerance,
        orientation: orientation(inner),
        checkpoint,
        work: 0,
    };
    let (mut a, mut b, mut first_a, mut first_b) = ([0.; 2], [0.; 2], [0.; 2], [0.; 2]);
    for (x, y) in inner.iter().zip(feather.path.iter()) {
        (ribbon.checkpoint)().map_err(PreparationError::Stopped)?;
        let pair = match (x, y) {
            (Segment::Move(x), Segment::Move(y)) => {
                a = *x;
                b = *y;
                first_a = a;
                first_b = b;
                continue;
            }
            (Segment::Line(x), Segment::Line(y)) => [line(a, *x), line(b, *y)],
            (Segment::Quad(x, y), Segment::Quad(u, v)) => [
                [a, lerp(a, *x, 2. / 3.), lerp(*y, *x, 2. / 3.), *y],
                [b, lerp(b, *u, 2. / 3.), lerp(*v, *u, 2. / 3.), *v],
            ],
            (Segment::Cubic(x, y, z), Segment::Cubic(u, v, w)) => {
                [[a, *x, *y, *z], [b, *u, *v, *w]]
            }
            (Segment::Close, Segment::Close) => [line(a, first_a), line(b, first_b)],
            _ => {
                return Err(PreparationError::Invalid(
                    "Feather contour topology differs".into(),
                ));
            }
        };
        a = pair[0][3];
        b = pair[1][3];
        ribbon.curves(pair[0], pair[1], 0)?;
    }
    Ok(())
}
