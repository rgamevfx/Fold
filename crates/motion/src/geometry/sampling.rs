//! Bounded curve sampling for geometric cell occupancy and convex profiles.
use crate::fields::Budget;
use fold_render::vector::Segment;
pub fn contours(
    segments: &[Segment],
    tolerance: f64,
    budget: &mut Budget,
) -> Result<Vec<Vec<[f64; 2]>>, String> {
    let mut result = Vec::new();
    let mut points = Vec::new();
    let mut current = [0.; 2];
    for segment in segments {
        budget.spend()?;
        match *segment {
            Segment::Move(p) => {
                if !points.is_empty() {
                    result.push(std::mem::take(&mut points));
                }
                points.push(p);
                current = p;
            }
            Segment::Line(p) => {
                points.push(p);
                current = p;
            }
            Segment::Close => {
                if !points.is_empty() {
                    result.push(std::mem::take(&mut points));
                }
            }
            Segment::Quad(a, b) => {
                let c1 = std::array::from_fn(|i| current[i] + (a[i] - current[i]) * 2. / 3.);
                let c2 = std::array::from_fn(|i| b[i] + (a[i] - b[i]) * 2. / 3.);
                cubic(current, c1, c2, b, tolerance, 0, &mut points, budget)?;
                current = b;
            }
            Segment::Cubic(a, b, c) => {
                cubic(current, a, b, c, tolerance, 0, &mut points, budget)?;
                current = c;
            }
        }
    }
    if !points.is_empty() {
        result.push(points);
    }
    if result
        .iter()
        .flatten()
        .flatten()
        .any(|v| !v.is_finite() || v.abs() > 1e9)
    {
        return Err("invalid geometry coordinate".into());
    }
    Ok(result)
}
fn cubic(
    a: [f64; 2],
    b: [f64; 2],
    c: [f64; 2],
    d: [f64; 2],
    tolerance: f64,
    depth: usize,
    out: &mut Vec<[f64; 2]>,
    budget: &mut Budget,
) -> Result<(), String> {
    budget.spend()?;
    let distance = |p: [f64; 2]| {
        let dx = d[0] - a[0];
        let dy = d[1] - a[1];
        let length = dx.hypot(dy);
        if length < 1e-12 {
            (p[0] - a[0]).hypot(p[1] - a[1])
        } else {
            let projection =
                (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (length * length)).clamp(0., 1.);
            (p[0] - a[0] - projection * dx).hypot(p[1] - a[1] - projection * dy)
        }
    };
    if depth >= 12 || distance(b).max(distance(c)) <= tolerance {
        out.push(d);
        return Ok(());
    }
    let mid = |a: [f64; 2], b: [f64; 2]| [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
    let ab = mid(a, b);
    let bc = mid(b, c);
    let cd = mid(c, d);
    let abc = mid(ab, bc);
    let bcd = mid(bc, cd);
    let center = mid(abc, bcd);
    cubic(a, ab, abc, center, tolerance, depth + 1, out, budget)?;
    cubic(center, bcd, cd, d, tolerance, depth + 1, out, budget)
}
pub fn contains(
    contours: &[Vec<[f64; 2]>],
    p: [f64; 2],
    even_odd: bool,
    budget: &mut Budget,
) -> Result<bool, String> {
    let mut winding = 0i32;
    for contour in contours {
        if contour.len() < 3 {
            continue;
        }
        for (a, b) in contour
            .iter()
            .zip(contour.iter().cycle().skip(1))
            .take(contour.len())
        {
            budget.spend()?;
            let cross = (b[0] - a[0]) * (p[1] - a[1]) - (p[0] - a[0]) * (b[1] - a[1]);
            if a[1] <= p[1] {
                if b[1] > p[1] && cross > 0. {
                    winding += 1;
                }
            } else if b[1] <= p[1] && cross < 0. {
                winding -= 1;
            }
        }
    }
    Ok(if even_odd {
        winding % 2 != 0
    } else {
        winding != 0
    })
}
pub fn hull(mut points: Vec<[f64; 2]>) -> Vec<[f64; 2]> {
    points.sort_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    points.dedup();
    if points.len() < 3 {
        return points;
    }
    let cross = |a: [f64; 2], b: [f64; 2], c: [f64; 2]| {
        (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
    };
    let mut lower: Vec<[f64; 2]> = Vec::new();
    let mut upper: Vec<[f64; 2]> = Vec::new();
    for p in &points {
        while lower.len() >= 2 && cross(lower[lower.len() - 2], lower[lower.len() - 1], *p) <= 0. {
            lower.pop();
        }
        lower.push(*p);
    }
    for p in points.iter().rev() {
        while upper.len() >= 2 && cross(upper[upper.len() - 2], upper[upper.len() - 1], *p) <= 0. {
            upper.pop();
        }
        upper.push(*p);
    }
    lower.pop();
    upper.pop();
    lower.extend(upper);
    lower
}

/// Reuse the edges crossing one grid row. Keep the original cross-product test
/// (rather than rounded x intersections) so boundary coverage stays identical.
pub struct Scanline {
    y: f64,
    edges: Vec<([f64; 2], [f64; 2], i32)>,
}
impl Scanline {
    pub fn new(contours: &[Vec<[f64; 2]>], y: f64, budget: &mut Budget) -> Result<Self, String> {
        let mut edges = Vec::new();
        for contour in contours.iter().filter(|c| c.len() >= 3) {
            for (a, b) in contour
                .iter()
                .zip(contour.iter().cycle().skip(1))
                .take(contour.len())
            {
                budget.spend()?;
                if a[1] <= y && b[1] > y {
                    edges.push((*a, *b, 1));
                } else if a[1] > y && b[1] <= y {
                    edges.push((*a, *b, -1));
                }
            }
        }
        Ok(Self { y, edges })
    }
    pub fn contains(&self, x: f64, even_odd: bool, budget: &mut Budget) -> Result<bool, String> {
        budget.spend()?;
        let mut winding = 0;
        for (a, b, direction) in &self.edges {
            budget.spend()?;
            let cross = (b[0] - a[0]) * (self.y - a[1]) - (x - a[0]) * (b[1] - a[1]);
            if (*direction == 1 && cross > 0.) || (*direction == -1 && cross < 0.) {
                winding += direction;
            }
        }
        Ok(if even_odd {
            winding % 2 != 0
        } else {
            winding != 0
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn grid_scanlines_match_point_winding_including_holes_and_boundaries() {
        let outlines = vec![
            vec![[0., 0.], [10., 0.], [10., 10.], [0., 10.]],
            vec![[2., 2.], [2., 8.], [8., 8.], [8., 2.]],
            vec![[0., 0.], [10., 10.], [0., 10.], [10., 0.]],
        ];
        for reflect in [-1., 1.] {
            let contours: Vec<Vec<_>> = outlines
                .iter()
                .map(|c| c.iter().map(|p| [p[0] * reflect, p[1]]).collect())
                .collect();
            for even_odd in [false, true] {
                for row in -2..=22 {
                    let y = row as f64 / 2.;
                    let scan = Scanline::new(&contours, y, &mut Budget::default()).unwrap();
                    for column in -22..=22 {
                        let x = column as f64 / 2.;
                        assert_eq!(
                            scan.contains(x, even_odd, &mut Budget::default()).unwrap(),
                            contains(&contours, [x, y], even_odd, &mut Budget::default()).unwrap()
                        );
                    }
                }
            }
        }
    }
    #[test]
    fn grid_scanline_checks_cancellation_even_without_crossing_edges() {
        let cancel = fold_media::Cancel::default();
        let scan = Scanline::new(&[], 0., &mut Budget::default()).unwrap();
        cancel.cancel();
        assert!(
            scan.contains(0., false, &mut Budget::cancellable(&cancel))
                .is_err()
        );
    }
}

/// Filled outlines in input coordinates, with upstream affine transforms applied.
/// Paint, strokes and image effects do not define occupancy.
pub struct FilledShape {
    pub contours: Vec<Vec<[f64; 2]>>,
    pub bounds: [f64; 4],
    pub even_odd: bool,
}
pub fn filled_shapes(
    content: &super::Content,
    budget: &mut Budget,
) -> Result<Vec<FilledShape>, String> {
    let drawings = super::drawings_with(content, [1., 1.], budget.cancel())?;
    let mut shapes = vec![];
    for drawing in drawings {
        budget.spend()?;
        let m = drawing.transform;
        let scale = m[0].hypot(m[1]).max(m[2].hypot(m[3])).max(1.);
        let mut contours = contours(&drawing.path, 0.25 / scale, budget)?;
        let mut bounds = [
            f64::INFINITY,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f64::NEG_INFINITY,
        ];
        for p in contours.iter_mut().flatten() {
            *p = [
                m[0] * p[0] + m[2] * p[1] + m[4],
                m[1] * p[0] + m[3] * p[1] + m[5],
            ];
            for axis in 0..2 {
                if !p[axis].is_finite() || p[axis].abs() > 1e9 {
                    return Err("filled geometry exceeds coordinate bounds".into());
                }
                bounds[axis] = bounds[axis].min(p[axis]);
                bounds[axis + 2] = bounds[axis + 2].max(p[axis]);
            }
        }
        if bounds[0] < bounds[2] && bounds[1] < bounds[3] {
            shapes.push(FilledShape {
                contours,
                bounds,
                even_odd: drawing.even_odd,
            });
        }
    }
    Ok(shapes)
}
