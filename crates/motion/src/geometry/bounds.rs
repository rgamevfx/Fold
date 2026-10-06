//! Analytic bounds of transformed native path centerlines/outlines. Stroke,
//! masks, clipping and image effects are deliberately outside this measurement.
use super::{Content, Geometry, IDENTITY, multiply};
use crate::fields::Budget;
use fold_render::vector::Segment;
#[derive(Default)]
pub struct Bounds(pub Option<[f64; 4]>);
impl Bounds {
    pub fn point(&mut self, p: [f64; 2]) {
        match &mut self.0 {
            Some(b) => {
                b[0] = b[0].min(p[0]);
                b[1] = b[1].min(p[1]);
                b[2] = b[2].max(p[0]);
                b[3] = b[3].max(p[1]);
            }
            None => self.0 = Some([p[0], p[1], p[0], p[1]]),
        }
    }
    fn curve(&mut self, p: [[f64; 2]; 4]) {
        self.point(p[0]);
        self.point(p[3]);
        for axis in 0..2 {
            let [p0, p1, p2, p3] = p.map(|v| v[axis]);
            let a = -p0 + 3. * p1 - 3. * p2 + p3;
            let b = 2. * (p0 - 2. * p1 + p2);
            let c = p1 - p0;
            let mut roots = vec![];
            if a.abs() < 1e-12 {
                if b.abs() > 1e-12 {
                    roots.push(-c / b);
                }
            } else {
                let d = b * b - 4. * a * c;
                if d >= 0. {
                    roots.push((-b + d.sqrt()) / (2. * a));
                    roots.push((-b - d.sqrt()) / (2. * a));
                }
            }
            for t in roots.into_iter().filter(|t| *t > 0. && *t < 1.) {
                let u = 1. - t;
                self.point(std::array::from_fn(|i| {
                    u * u * u * p[0][i]
                        + 3. * u * u * t * p[1][i]
                        + 3. * u * t * t * p[2][i]
                        + t * t * t * p[3][i]
                }));
            }
        }
    }
    pub fn path(
        &mut self,
        path: &[Segment],
        m: [f64; 6],
        budget: &mut Budget,
    ) -> Result<(), String> {
        let map = |p: [f64; 2]| {
            [
                m[0] * p[0] + m[2] * p[1] + m[4],
                m[1] * p[0] + m[3] * p[1] + m[5],
            ]
        };
        let mut current = map([0., 0.]);
        let mut start = current;
        for segment in path {
            budget.spend()?;
            match *segment {
                Segment::Move(p) => {
                    current = map(p);
                    start = current;
                }
                Segment::Line(p) => {
                    self.point(current);
                    current = map(p);
                    self.point(current);
                }
                Segment::Quad(control, end) => {
                    let q = map(control);
                    let end = map(end);
                    self.curve([
                        current,
                        std::array::from_fn(|i| current[i] + (q[i] - current[i]) * 2. / 3.),
                        std::array::from_fn(|i| end[i] + (q[i] - end[i]) * 2. / 3.),
                        end,
                    ]);
                    current = end;
                }
                Segment::Cubic(a, b, end) => {
                    let end = map(end);
                    self.curve([current, map(a), map(b), end]);
                    current = end;
                }
                Segment::Close => {
                    self.point(current);
                    self.point(start);
                    current = start;
                }
            }
        }
        Ok(())
    }
}
pub fn content(content: &Content, cancel: &fold_media::Cancel) -> Result<Option<[f64; 4]>, String> {
    content_with_budget(content, &mut Budget::cancellable(cancel))
}
pub fn content_with_budget(
    content: &Content,
    budget: &mut Budget,
) -> Result<Option<[f64; 4]>, String> {
    fn walk(
        c: &Content,
        m: [f64; 6],
        b: &mut Bounds,
        budget: &mut Budget,
        depth: usize,
    ) -> Result<(), String> {
        if depth > 64 {
            return Err("bounds nesting exceeds 64".into());
        }
        for e in c.iter() {
            budget.spend()?;
            let m = multiply(m, e.transform);
            match &e.geometry {
                Geometry::Path(path) | Geometry::Glyph { path, .. } => b.path(path, m, budget)?,
                Geometry::Point => b.point([m[4], m[5]]),
                Geometry::Group(children) | Geometry::Instance(children) => {
                    walk(children, m, b, budget, depth + 1)?
                }
            }
        }
        Ok(())
    }
    let mut bounds = Bounds::default();
    walk(content, IDENTITY, &mut bounds, budget, 0)?;
    Ok(bounds.0)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quadratic_extremum_is_not_control_hull() {
        let mut b = Bounds::default();
        b.path(
            &[Segment::Move([0., 0.]), Segment::Quad([1., 2.], [2., 0.])],
            [2., 0., 0., 3., 10., 20.],
            &mut Budget::default(),
        )
        .unwrap();
        assert_eq!(b.0, Some([10., 20., 14., 23.]));
    }
}
