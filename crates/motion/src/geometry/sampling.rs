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
