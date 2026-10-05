//! Direct Bézier edits preserve attached handles; Alt breaks tangent coupling.
use fold_render::vector::Segment;
pub fn move_point(
    segments: &mut [Segment],
    index: usize,
    component: usize,
    point: [f64; 2],
    break_tangent: bool,
) {
    use Segment::*;
    let Some(segment) = segments.get(index) else {
        return;
    };
    let old = match segment {
        Move(a) | Line(a) => *a,
        Quad(a, b) => {
            if component == 0 {
                *a
            } else {
                *b
            }
        }
        Cubic(a, b, c) => match component {
            0 => *a,
            1 => *b,
            _ => *c,
        },
        Close => return,
    };
    let last = segments.len().checked_sub(2);
    let closed =
        matches!(segments.last(), Some(Close)) && matches!(segments.first(), Some(Move(_)));
    let delta = [point[0] - old[0], point[1] - old[1]];
    let endpoint = matches!(segment, Move(_) | Line(_))
        || matches!(segment, Quad(..)) && component == 1
        || matches!(segment, Cubic(..)) && component == 2;
    let shift = |p: &mut [f64; 2]| {
        p[0] += delta[0];
        p[1] += delta[1];
    };
    if endpoint {
        match &mut segments[index] {
            Move(p) | Line(p) => *p = point,
            Quad(a, b) => {
                shift(a);
                *b = point;
            }
            Cubic(_, b, c) => {
                shift(b);
                *c = point;
            }
            Close => {}
        }
        if let Some(Quad(a, _) | Cubic(a, _, _)) = segments.get_mut(index + 1) {
            shift(a);
        }
        if closed
            && index == 0
            && let Some(last) = last
        {
            match &mut segments[last] {
                Cubic(_, b, c) if *c == old => {
                    shift(b);
                    *c = point;
                }
                Quad(a, b) if *b == old => {
                    shift(a);
                    *b = point;
                }
                Line(p) if *p == old => *p = point,
                _ => {}
            }
        } else if closed
            && Some(index) == last
            && matches!(segments.first(),Some(Move(p)) if *p==old)
        {
            segments[0] = Move(point);
            if let Some(Quad(a, _) | Cubic(a, _, _)) = segments.get_mut(1) {
                shift(a);
            }
        }
    } else {
        match &mut segments[index] {
            Quad(a, _) => *a = point,
            Cubic(a, b, _) => {
                if component == 0 {
                    *a = point
                } else {
                    *b = point
                }
            }
            _ => {}
        }
        if !break_tangent {
            if component == 0 && index > 0 {
                if let Cubic(_, b, c) = &mut segments[index - 1] {
                    *b = [2. * c[0] - point[0], 2. * c[1] - point[1]];
                } else if closed
                    && index == 1
                    && let Some(last) = last
                    && let Cubic(_, b, c) = &mut segments[last]
                {
                    *b = [2. * c[0] - point[0], 2. * c[1] - point[1]];
                }
            } else if component == 1 {
                let anchor = match segments[index] {
                    Cubic(_, _, c) => Some(c),
                    _ => None,
                };
                if let (Some(c), Some(Cubic(a, _, _))) = (anchor, segments.get_mut(index + 1)) {
                    *a = [2. * c[0] - point[0], 2. * c[1] - point[1]];
                } else if closed
                    && Some(index) == last
                    && let (Some(c), Some(Cubic(a, _, _))) = (anchor, segments.get_mut(1))
                {
                    *a = [2. * c[0] - point[0], 2. * c[1] - point[1]];
                }
            }
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closed_anchor_moves_closing_endpoint_and_both_handles() {
        use Segment::*;
        let mut p = vec![
            Move([0., 0.]),
            Cubic([10., 0.], [90., 0.], [100., 0.]),
            Cubic([90., 10.], [-10., 0.], [0., 0.]),
            Close,
        ];
        move_point(&mut p, 0, 0, [5., 5.], false);
        assert_eq!(p[1], Cubic([15., 5.], [90., 0.], [100., 0.]));
        assert_eq!(p[2], Cubic([90., 10.], [-5., 5.], [5., 5.]));
        move_point(&mut p, 1, 0, [20., 10.], false);
        assert_eq!(p[2], Cubic([90., 10.], [-10., 0.], [5., 5.]));
    }
    #[test]
    fn moving_anchor_preserves_both_attached_handles() {
        let mut p = vec![
            Segment::Move([0., 0.]),
            Segment::Cubic([10., 0.], [90., 0.], [100., 0.]),
            Segment::Cubic([110., 0.], [190., 0.], [200., 0.]),
        ];
        move_point(&mut p, 1, 2, [100., 20.], false);
        assert_eq!(p[1], Segment::Cubic([10., 0.], [90., 20.], [100., 20.]));
        assert_eq!(p[2], Segment::Cubic([110., 20.], [190., 0.], [200., 0.]));
        move_point(&mut p, 2, 0, [110., 40.], false);
        assert_eq!(p[1], Segment::Cubic([10., 0.], [90., 0.], [100., 20.]));
    }
}
