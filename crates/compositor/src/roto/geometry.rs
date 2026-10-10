//! Spline conversion is shared by viewport observation and matte preparation.
use super::{Layer, Point, Shape, Spline};
use fold_render::{Affine, vector::Segment};
pub type Cubic = [[f64; 2]; 4];
pub fn add(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] + b[0], a[1] + b[1]]
}
pub fn sub(a: [f64; 2], b: [f64; 2]) -> [f64; 2] {
    [a[0] - b[0], a[1] - b[1]]
}
pub fn mul(a: [f64; 2], s: f64) -> [f64; 2] {
    [a[0] * s, a[1] * s]
}
pub fn map(m: Affine, p: [f64; 2]) -> [f64; 2] {
    [
        m.a * p[0] + m.c * p[1] + m.tx,
        m.b * p[0] + m.d * p[1] + m.ty,
    ]
}
pub fn compose(a: Affine, b: Affine) -> Affine {
    Affine {
        a: a.a * b.a + a.c * b.b,
        b: a.b * b.a + a.d * b.b,
        c: a.a * b.c + a.c * b.d,
        d: a.b * b.c + a.d * b.d,
        tx: a.a * b.tx + a.c * b.ty + a.tx,
        ty: a.b * b.tx + a.d * b.ty + a.ty,
    }
}
impl Shape {
    pub fn feather_positions(&self) -> Vec<[f64; 2]> {
        let n = self.points.len();
        let area: f64 = (0..n)
            .map(|i| {
                let a = self.points[i].position;
                let b = self.points[(i + 1) % n].position;
                a[0] * b[1] - b[0] * a[1]
            })
            .sum();
        (0..n)
            .map(|i| {
                let p = &self.points[i];
                let t = sub(
                    self.points[(i + 1) % n].position,
                    self.points[(i + n - 1) % n].position,
                );
                let length = t[0].hypot(t[1]);
                let normal = if length > 1e-9 {
                    mul([t[1], -t[0]], area.signum() / length)
                } else {
                    [0.; 2]
                };
                add(add(p.position, mul(normal, self.feather)), p.feather)
            })
            .collect()
    }
    pub fn cubics(&self, feather: bool) -> Vec<Cubic> {
        let n = self.points.len();
        if n < 2 {
            return vec![];
        }
        let p = if feather {
            self.feather_positions()
        } else {
            self.points.iter().map(|p| p.position).collect()
        };
        let count = if self.closed { n } else { n - 1 };
        (0..count)
            .map(|i| {
                let j = (i + 1) % n;
                if self.spline == Spline::Bezier {
                    [
                        p[i],
                        add(p[i], self.points[i].outgoing),
                        add(p[j], self.points[j].incoming),
                        p[j],
                    ]
                } else {
                    let a = p[if i == 0 {
                        if self.closed { n - 1 } else { 0 }
                    } else {
                        i - 1
                    }];
                    let b = p[i];
                    let c = p[j];
                    let d = p[if i + 2 < n {
                        i + 2
                    } else if self.closed {
                        (i + 2) % n
                    } else {
                        n - 1
                    }];
                    [
                        mul(add(add(a, mul(b, 4.)), c), 1. / 6.),
                        mul(add(mul(b, 2.), c), 1. / 3.),
                        mul(add(b, mul(c, 2.)), 1. / 3.),
                        mul(add(add(b, mul(c, 4.)), d), 1. / 6.),
                    ]
                }
            })
            .collect()
    }
    pub fn path(&self, feather: bool) -> Vec<Segment> {
        let curves = self.cubics(feather);
        let Some(first) = curves.first() else {
            return vec![];
        };
        let mut path = vec![Segment::Move(first[0])];
        path.extend(curves.iter().map(|c| Segment::Cubic(c[1], c[2], c[3])));
        if self.closed {
            path.push(Segment::Close);
        }
        path
    }
}
impl Layer {
    pub fn rectangle(a: [f64; 2], b: [f64; 2], ellipse: bool) -> Self {
        let [x, y] = [a[0].min(b[0]), a[1].min(b[1])];
        let [r, d] = [a[0].max(b[0]), a[1].max(b[1])];
        let mut layer = if ellipse {
            let (cx, cy, rx, ry) = ((x + r) / 2., (y + d) / 2., (r - x) / 2., (d - y) / 2.);
            let k = 0.5522847498307936;
            let mut points: Vec<_> = [[r, cy], [cx, d], [x, cy], [cx, y]]
                .into_iter()
                .map(Point::new)
                .collect();
            for (p, t) in
                points
                    .iter_mut()
                    .zip([[0., ry * k], [-rx * k, 0.], [0., -ry * k], [rx * k, 0.]])
            {
                p.outgoing = t;
                p.incoming = mul(t, -1.);
            }
            Self::curve(Spline::Bezier, points)
        } else {
            Self::curve(
                Spline::Bezier,
                [[x, y], [r, y], [r, d], [x, d]]
                    .into_iter()
                    .map(Point::new)
                    .collect(),
            )
        };
        layer.name = if ellipse { "Ellipse" } else { "Rectangle" }.into();
        layer
    }
}
impl Shape {
    /// Split a Bézier at the picked parameter without changing its main contour.
    /// A B-spline inserts a new control point at the picked curve position.
    pub fn insert_point(
        &mut self,
        segment: usize,
        t: f64,
    ) -> Result<fold_foundation::ObjectId, String> {
        if self.points.len() < 2 || segment >= self.cubics(false).len() || !(0. ..=1.).contains(&t)
        {
            return Err("Invalid curve insertion".into());
        }
        let next = (segment + 1) % self.points.len();
        let c = self.cubics(false)[segment];
        let lerp = |a, b| add(a, mul(sub(b, a), t));
        let a = lerp(c[0], c[1]);
        let b = lerp(c[1], c[2]);
        let d = lerp(c[2], c[3]);
        let e = lerp(a, b);
        let f = lerp(b, d);
        let middle = lerp(e, f);
        let mut point = Point::new(middle);
        if self.spline == Spline::Bezier {
            self.points[segment].outgoing = sub(a, c[0]);
            self.points[next].incoming = sub(d, c[3]);
            point.incoming = sub(e, middle);
            point.outgoing = sub(f, middle);
        }
        let id = point.id;
        self.points.insert(segment + 1, point);
        Ok(id)
    }
}
