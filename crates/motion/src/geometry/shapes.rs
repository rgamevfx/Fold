//! Native cubic construction. Coordinates use top-left document pixels.
use fold_render::vector::Segment as S;
pub fn ellipse(width: f64, height: f64) -> Vec<S> {
    let x = width / 2.;
    let y = height / 2.;
    let k = 0.5522847498307936;
    vec![
        S::Move([x, 0.]),
        S::Cubic([x + k * x, 0.], [width, y - k * y], [width, y]),
        S::Cubic([width, y + k * y], [x + k * x, height], [x, height]),
        S::Cubic([x - k * x, height], [0., y + k * y], [0., y]),
        S::Cubic([0., y - k * y], [x - k * x, 0.], [x, 0.]),
        S::Close,
    ]
}
pub fn rectangle(width: f64, height: f64, radius: f64) -> Vec<S> {
    let r = radius.clamp(0., width.min(height) / 2.);
    let k = r * 0.5522847498307936;
    vec![
        S::Move([r, 0.]),
        S::Line([width - r, 0.]),
        S::Cubic([width - r + k, 0.], [width, r - k], [width, r]),
        S::Line([width, height - r]),
        S::Cubic(
            [width, height - r + k],
            [width - r + k, height],
            [width - r, height],
        ),
        S::Line([r, height]),
        S::Cubic([r - k, height], [0., height - r + k], [0., height - r]),
        S::Line([0., r]),
        S::Cubic([0., r - k], [r - k, 0.], [r, 0.]),
        S::Close,
    ]
}
