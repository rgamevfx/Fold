//! Curve geometry for pointer selection, never artwork rasterization. Flattening
//! uses screen-space error so zoom and ancestor transforms do not weaken picking.
use super::overlay::View;
use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::{
    model::{Cap, Contour, Join, Paint},
    tools::Transform,
};

type Position = [f64; 2];
fn projection(p: Position, a: Position, b: Position) -> (f64, f64) {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    let t = if length > 0. {
        ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length
    } else {
        0.
    };
    let t = t.clamp(0., 1.);
    ((p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1]), t)
}
fn distance(p: Position, a: Position, b: Position) -> f64 {
    projection(p, a, b).0
}
fn triangle(p: Position, vertices: [Position; 3]) -> bool {
    let cross =
        |a: Position, b: Position| (b[0] - a[0]) * (p[1] - a[1]) - (b[1] - a[1]) * (p[0] - a[0]);
    let [a, b, c] = vertices;
    let signs = [cross(a, b), cross(b, c), cross(c, a)];
    signs.iter().all(|v| *v >= 0.) || signs.iter().all(|v| *v <= 0.)
}
fn join(contour: &Contour, local: Position, half: f64, kind: Join, limit: f64) -> bool {
    let points = &contour.points;
    if points.len() < 2 || half <= 0. || !half.is_finite() {
        return false;
    }
    let unit = |v: Position| {
        let length = v[0].hypot(v[1]);
        (length > 0. && length.is_finite()).then(|| [v[0] / length, v[1] / length])
    };
    let difference = |a: Position, b: Position| [a[0] - b[0], a[1] - b[1]];
    for (index, point) in points.iter().enumerate() {
        if !contour.closed && (index == 0 || index + 1 == points.len()) {
            continue;
        }
        let previous = &points[(index + points.len() - 1) % points.len()];
        let next = &points[(index + 1) % points.len()];
        // First nonzero endpoint derivative, including collapsed Bézier handles.
        let incoming = unit(point.incoming.map(|v| -v))
            .or_else(|| {
                unit(difference(
                    point.position,
                    [
                        previous.position[0] + previous.outgoing[0],
                        previous.position[1] + previous.outgoing[1],
                    ],
                ))
            })
            .or_else(|| unit(difference(point.position, previous.position)));
        let outgoing = unit(point.outgoing)
            .or_else(|| {
                unit(difference(
                    [
                        next.position[0] + next.incoming[0],
                        next.position[1] + next.incoming[1],
                    ],
                    point.position,
                ))
            })
            .or_else(|| unit(difference(next.position, point.position)));
        let (Some(a), Some(b)) = (incoming, outgoing) else {
            continue;
        };
        let turn = a[0] * b[1] - a[1] * b[0];
        if turn.abs() < 1e-12 {
            continue;
        }
        let side = turn.signum();
        let offset = |direction: Position| {
            [
                point.position[0] + side * direction[1] * half,
                point.position[1] - side * direction[0] * half,
            ]
        };
        let start = offset(a);
        let end = offset(b);
        let delta = difference(end, start);
        let t = (delta[0] * b[1] - delta[1] * b[0]) / turn;
        let tip = [start[0] + t * a[0], start[1] + t * a[1]];
        if kind == Join::Round {
            if (local[0] - point.position[0]).hypot(local[1] - point.position[1]) <= half {
                return true;
            }
        } else if kind == Join::Miter
            && tip.iter().all(|v| v.is_finite())
            && (tip[0] - point.position[0]).hypot(tip[1] - point.position[1]) <= half * limit
        {
            if triangle(local, [point.position, start, tip])
                || triangle(local, [point.position, tip, end])
            {
                return true;
            }
        } else if triangle(local, [point.position, start, end]) {
            return true;
        }
    }
    false
}
fn caps(contour: &Contour, local: Position, half: f64, cap: Cap) -> bool {
    if contour.closed || contour.points.len() < 2 || half <= 0. {
        return false;
    }
    let points = &contour.points;
    for (point, neighbor, handle, neighbor_handle) in [
        (
            &points[0],
            &points[1],
            points[0].outgoing,
            points[1].incoming,
        ),
        (
            &points[points.len() - 1],
            &points[points.len() - 2],
            points[points.len() - 1].incoming,
            points[points.len() - 2].outgoing,
        ),
    ] {
        let delta = [local[0] - point.position[0], local[1] - point.position[1]];
        if cap == Cap::Round && delta[0].hypot(delta[1]) <= half {
            return true;
        }
        if cap != Cap::Square {
            continue;
        }
        let direction = [
            handle,
            [
                neighbor.position[0] + neighbor_handle[0] - point.position[0],
                neighbor.position[1] + neighbor_handle[1] - point.position[1],
            ],
            [
                neighbor.position[0] - point.position[0],
                neighbor.position[1] - point.position[1],
            ],
        ]
        .into_iter()
        .find(|v| v[0].hypot(v[1]) > 0.);
        let Some(direction) = direction else {
            continue;
        };
        let length = direction[0].hypot(direction[1]);
        let along = (delta[0] * direction[0] + delta[1] * direction[1]) / length;
        let across = (delta[0] * direction[1] - delta[1] * direction[0]) / length;
        if (-half..=0.).contains(&along) && across.abs() <= half {
            return true;
        }
    }
    false
}
fn strip(p: Position, a: Position, b: Position, half: f64) -> bool {
    let d = [b[0] - a[0], b[1] - a[1]];
    let length = d[0] * d[0] + d[1] * d[1];
    if length <= 0. {
        return false;
    }
    let along = ((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1]) / length;
    (0. ..=1.).contains(&along) && distance(p, a, b) <= half
}
fn flatten(
    curve: [Position; 4],
    screen: &impl Fn(Position) -> Position,
    depth: u8,
    range: [f64; 2],
    edge: &mut impl FnMut(Position, Position, [f64; 2]),
) {
    let projected = curve.map(screen);
    if depth == 12
        || distance(projected[1], projected[0], projected[3]) <= 0.25
            && distance(projected[2], projected[0], projected[3]) <= 0.25
    {
        edge(curve[0], curve[3], range);
        return;
    }
    let midpoint = |a: Position, b: Position| [a[0] / 2. + b[0] / 2., a[1] / 2. + b[1] / 2.];
    let a = midpoint(curve[0], curve[1]);
    let b = midpoint(curve[1], curve[2]);
    let c = midpoint(curve[2], curve[3]);
    let d = midpoint(a, b);
    let e = midpoint(b, c);
    let f = midpoint(d, e);
    let middle = (range[0] + range[1]) / 2.;
    flatten(
        [curve[0], a, d, f],
        screen,
        depth + 1,
        [range[0], middle],
        edge,
    );
    flatten(
        [f, e, c, curve[3]],
        screen,
        depth + 1,
        [middle, range[1]],
        edge,
    );
}
/// Find an interior curve position for the Pen tool. This is screen-space
/// pointer geometry only; the document operation splits the original cubic.
pub(super) fn segment(
    contours: &[Contour],
    transform: Transform,
    position: Position,
    view: View,
) -> Option<(ObjectId, ObjectId, f64)> {
    let screen = |point| view.screen(transform.point(point)).map(f64::from);
    let mouse = view.screen(position).map(f64::from);
    // Anchors already have a direct editing target. Do not create near-duplicate
    // anchors when clicking an endpoint or a very short segment.
    if contours
        .iter()
        .flat_map(|c| &c.points)
        .any(|p| distance(mouse, screen(p.position), screen(p.position)) <= 9.)
    {
        return None;
    }
    let mut closest = (6., None);
    for contour in contours {
        for (index, a) in contour.points.iter().enumerate() {
            let Some(b) = contour
                .points
                .get(index + 1)
                .or_else(|| contour.closed.then(|| contour.points.first()).flatten())
            else {
                continue;
            };
            let curve = [
                a.position,
                [a.position[0] + a.outgoing[0], a.position[1] + a.outgoing[1]],
                [b.position[0] + b.incoming[0], b.position[1] + b.incoming[1]],
                b.position,
            ];
            flatten(curve, &screen, 0, [0., 1.], &mut |a, b, range| {
                if distance(mouse, screen(a), screen(b)) >= closest.0 {
                    return;
                }
                let straight = curve[1] == curve[0] && curve[2] == curve[3];
                let at = |t: f64| {
                    if straight {
                        return [
                            curve[0][0] * (1. - t) + curve[3][0] * t,
                            curve[0][1] * (1. - t) + curve[3][1] * t,
                        ];
                    }
                    let u = 1. - t;
                    std::array::from_fn(|i| {
                        u * u * u * curve[0][i]
                            + 3. * u * u * t * curve[1][i]
                            + 3. * u * t * t * curve[2][i]
                            + t * t * t * curve[3][i]
                    })
                };
                let error = |t| distance(mouse, screen(at(t)), screen(at(t)));
                // Refine on the original cubic: collinear control handles may
                // have a nonlinear parameter even though the flattened edge is straight.
                let [mut low, mut high] = range;
                for _ in 0..24 {
                    let left = low + (high - low) / 3.;
                    let right = high - (high - low) / 3.;
                    if error(left) < error(right) {
                        high = right;
                    } else {
                        low = left;
                    }
                }
                let t = (low + high) / 2.;
                let distance = error(t);
                if distance < closest.0 && t > 0. && t < 1. {
                    closest = (distance, Some((contour.id, contour.points[index].id, t)));
                }
            });
        }
    }
    closest.1
}

pub(super) fn path(
    contours: &[Contour],
    paint: &Paint,
    transform: Transform,
    position: Position,
    view: View,
    time: Time,
) -> bool {
    let fill = paint.fill.as_ref().is_some_and(|source| source.visible());
    let width = paint.stroke_width.at(time);
    let stroke = width > 0. && paint.stroke.as_ref().is_some_and(|source| source.visible());
    if !fill && !stroke {
        return false;
    }
    let Ok(inverse) = transform.inverse() else {
        return false;
    };
    let local = inverse.point(position);
    if stroke
        && contours.iter().any(|contour| {
            join(contour, local, width / 2., paint.join, paint.miter_limit)
                || caps(contour, local, width / 2., paint.cap)
        })
    {
        return true;
    }
    let screen = |point| view.screen(transform.point(point)).map(f64::from);
    let mouse = view.screen(position).map(f64::from);
    let mut winding = 0_i64;
    let mut near = false;
    let mut edge = |a: Position, b: Position, stroked: bool| {
        // Minimum pointer target is five screen pixels. Authored stroke width
        // is evaluated in local space, including nonuniform group transforms.
        if stroke
            && stroked
            && (distance(mouse, screen(a), screen(b)) <= 5. || strip(local, a, b, width / 2.))
        {
            near = true;
        }
        if fill {
            let cross = (b[0] - a[0]) * (local[1] - a[1]) - (local[0] - a[0]) * (b[1] - a[1]);
            if a[1] <= local[1] && b[1] > local[1] && cross > 0. {
                winding += 1;
            }
            if a[1] > local[1] && b[1] <= local[1] && cross < 0. {
                winding -= 1;
            }
            if distance(mouse, screen(a), screen(b)) <= 0.5 {
                near = true;
            }
        }
    };
    for contour in contours {
        let Some(first) = contour.points.first() else {
            continue;
        };
        for index in 0..contour.points.len() {
            let a = &contour.points[index];
            let b = contour
                .points
                .get(index + 1)
                .or_else(|| contour.closed.then_some(first));
            if let Some(b) = b {
                let curve = [
                    a.position,
                    [a.position[0] + a.outgoing[0], a.position[1] + a.outgoing[1]],
                    [b.position[0] + b.incoming[0], b.position[1] + b.incoming[1]],
                    b.position,
                ];
                flatten(curve, &screen, 0, [0., 1.], &mut |a, b, _| edge(a, b, true));
            } else if fill {
                // SVG closes open contours for fill, while stroke remains open.
                edge(a.position, first.position, false);
            }
        }
    }
    near || if paint.even_odd {
        winding % 2 != 0
    } else {
        winding != 0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_foundation::ObjectId;
    use fold_motion_canvas::model::{Content, Object, Point};
    use fold_ui::sdk::ViewerRect;

    fn contour(positions: &[[f64; 2]], closed: bool) -> Contour {
        Contour {
            id: ObjectId::new(),
            closed,
            extensions: Default::default(),
            points: positions
                .iter()
                .map(|position| Point {
                    position_track: None,
                    incoming_track: None,
                    outgoing_track: None,
                    id: ObjectId::new(),
                    position: *position,
                    incoming: [0.; 2],
                    outgoing: [0.; 2],
                    extensions: Default::default(),
                })
                .collect(),
        }
    }
    fn view(scale: f32) -> View {
        View {
            rect: ViewerRect {
                editable: true,
                image_current: true,
                canvas_origin: [20., 30.],
                canvas_size: [100. * scale, 100. * scale],
                dimensions: [100, 100],
                origin: [20., 30.],
                size: [100. * scale, 100. * scale],
            },
            dimensions: [100, 100],
        }
    }
    #[test]
    fn miter_tips_follow_authored_limits_and_transformed_endpoint_tangents() {
        let mut shape = contour(&[[-20., 60.], [0., 0.], [20., 60.]], false);
        let mut paint = Paint::default();
        paint.fill = None;
        paint.stroke = Some([1.; 4].into());
        paint.stroke_width.value = 10.;
        paint.join = Join::Miter;
        paint.miter_limit = 10.;
        for transform in [
            Transform::IDENTITY,
            Transform([-2., 0.3, 0.7, 0.5, 100., 100.]),
        ] {
            assert!(path(
                &[shape.clone()],
                &paint,
                transform,
                transform.point([0., -14.]),
                view(10.),
                Time::ZERO
            ));
            assert!(!path(
                &[shape.clone()],
                &paint,
                transform,
                transform.point([0., -20.]),
                view(10.),
                Time::ZERO
            ));
            paint.miter_limit = 1.;
            assert!(!path(
                &[shape.clone()],
                &paint,
                transform,
                transform.point([0., -14.]),
                view(10.),
                Time::ZERO
            ));
            paint.miter_limit = 10.;
        }
        shape.points[1].incoming = [-2., 6.];
        shape.points[1].outgoing = [2., 6.];
        assert!(join(&shape, [0., -14.], 5., Join::Miter, 10.));
        let closed = contour(&[[0., 0.], [20., 60.], [-20., 60.]], true);
        assert!(join(&closed, [0., -14.], 5., Join::Miter, 10.));
        let straight = contour(&[[0., 0.], [20., 0.], [40., 0.]], false);
        assert!(!join(&straight, [100., 100.], 5., Join::Miter, 10.));
        assert!(!join(&shape, [100., 100.], 0., Join::Miter, 10.));
    }
    #[test]
    fn stroke_caps_and_join_fallback_follow_authored_geometry() {
        let line = contour(&[[0., 0.], [100., 0.]], false);
        let mut paint = Paint::default();
        paint.fill = None;
        paint.stroke = Some([1.; 4].into());
        paint.stroke_width.value = 20.;
        for transform in [
            Transform::IDENTITY,
            Transform([-2., 0.3, 0.7, 0.5, 100., 100.]),
        ] {
            let hit = |paint: &Paint, p| {
                path(
                    &[line.clone()],
                    paint,
                    transform,
                    transform.point(p),
                    view(20.),
                    Time::ZERO,
                )
            };
            paint.cap = Cap::Butt;
            assert!(!hit(&paint, [-8., 0.]));
            assert!(hit(&paint, [50., 9.]));
            paint.cap = Cap::Round;
            assert!(hit(&paint, [-8., 0.]));
            assert!(!hit(&paint, [-8., 8.]));
            paint.cap = Cap::Square;
            assert!(hit(&paint, [-8., 8.]));
            assert!(hit(&paint, [108., 8.]));
            assert!(!hit(&paint, [-12., 0.]));
        }
        let corner = contour(&[[-20., 60.], [0., 0.], [20., 60.]], false);
        assert!(join(&corner, [0., -1.], 5., Join::Bevel, 10.));
        assert!(!join(&corner, [0., -4.], 5., Join::Bevel, 10.));
        assert!(join(&corner, [0., -4.], 5., Join::Round, 10.));
        // SVG miter limit falls back to bevel.
        assert!(join(&corner, [0., -1.], 5., Join::Miter, 1.));
        assert!(!join(&corner, [0., -4.], 5., Join::Miter, 1.));
        let mut closed = line;
        closed.closed = true;
        assert!(!caps(&closed, [-8., 0.], 10., Cap::Square));
    }

    #[test]
    fn insertion_target_retains_curve_parameters_and_closed_edges() {
        let mut curve = contour(&[[0., 0.], [100., 0.]], false);
        // The curve is collinear but its parameter is not linear in position.
        curve.points[0].outgoing = [5., 0.];
        curve.points[1].incoming = [-70., 0.];
        let transform = Transform([2., 0., 0., 0.5, 10., 20.]);
        let position = [0.125 * 0. + 0.375 * 5. + 0.375 * 30. + 0.125 * 100., 0.];
        let (id, start, t) = segment(
            &[curve.clone()],
            transform,
            transform.point(position),
            view(2.),
        )
        .unwrap();
        assert_eq!(id, curve.id);
        assert_eq!(start, curve.points[0].id);
        assert!((t - 0.5).abs() < 0.0001);
        assert!(segment(&[curve], transform, transform.point([0., 0.]), view(2.)).is_none());
        let closed = contour(&[[0., 0.], [100., 0.], [100., 100.]], true);
        let (_, start, t) =
            segment(&[closed.clone()], Transform::IDENTITY, [50., 50.], view(1.)).unwrap();
        assert_eq!(start, closed.points[2].id);
        assert!((t - 0.5).abs() < 0.0001);
        let mut open = closed;
        open.closed = false;
        assert!(segment(&[open], Transform::IDENTITY, [50., 50.], view(1.)).is_none());
    }

    #[test]
    fn picks_curves_and_fills_with_holes_at_screen_scale() {
        let mut paint = Object::new(
            "Path",
            Content::Path {
                contours: vec![],
                extensions: Default::default(),
            },
        )
        .paint;
        paint.stroke = None;
        let outer = contour(&[[0., 0.], [100., 0.], [100., 100.], [0., 100.]], true);
        let inner = contour(&[[25., 25.], [75., 25.], [75., 75.], [25., 75.]], true);
        let contours = [outer.clone(), inner.clone()];
        assert!(path(
            &contours,
            &paint,
            Transform::IDENTITY,
            [50., 50.],
            view(1.),
            Time::ZERO
        ));
        paint.even_odd = true;
        assert!(!path(
            &contours,
            &paint,
            Transform::IDENTITY,
            [50., 50.],
            view(1.),
            Time::ZERO
        ));
        assert!(path(
            &contours,
            &paint,
            Transform::IDENTITY,
            [10., 50.],
            view(1.),
            Time::ZERO
        ));
        paint.even_odd = false;
        let mut reversed = inner;
        reversed.points.reverse();
        assert!(!path(
            &[outer, reversed],
            &paint,
            Transform::IDENTITY,
            [50., 50.],
            view(1.),
            Time::ZERO
        ));

        let open = contour(&[[0., 0.], [100., 0.], [0., 100.]], false);
        assert!(path(
            &[open.clone()],
            &paint,
            Transform::IDENTITY,
            [20., 20.],
            view(1.),
            Time::ZERO
        ));
        paint.fill = None;
        paint.stroke = Some([1.; 4].into());
        paint.stroke_width.value = 1.;
        assert!(!path(
            &[open],
            &paint,
            Transform::IDENTITY,
            [0., 50.],
            view(1.),
            Time::ZERO
        ));

        let mut curve = contour(&[[0., 0.], [100., 0.]], false);
        curve.points[0].outgoing = [0., 100.];
        curve.points[1].incoming = [0., 100.];
        let transform = Transform([2., 0., 0., 0.5, 10., 20.]);
        assert!(path(
            &[curve.clone()],
            &paint,
            transform,
            transform.point([50., 75.]),
            view(1.),
            Time::ZERO
        ));
        assert!(!path(
            &[curve.clone()],
            &paint,
            transform,
            transform.point([50., 20.]),
            view(1.),
            Time::ZERO
        ));
        let line = contour(&[[0., 0.], [100., 0.]], false);
        assert!(path(
            &[line.clone()],
            &paint,
            Transform::IDENTITY,
            [50., 4.],
            view(1.),
            Time::ZERO
        ));
        assert!(!path(
            &[line],
            &paint,
            Transform::IDENTITY,
            [50., 4.],
            view(10.),
            Time::ZERO
        ));
        paint.stroke = Some([1., 1., 1., 0.].into());
        assert!(!path(
            &[curve],
            &paint,
            transform,
            transform.point([50., 75.]),
            view(1.),
            Time::ZERO
        ));
    }
}
