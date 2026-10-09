//! Shared unit-quaternion interpolation for evaluation and curve display.
fn dot(a: [f64; 4], b: [f64; 4]) -> f64 {
    a.into_iter().zip(b).map(|(a, b)| a * b).sum()
}
pub fn normalize(q: [f64; 4]) -> [f64; 4] {
    let length = dot(q, q).sqrt();
    q.map(|v| v / length)
}
pub fn slerp(a: [f64; 4], mut b: [f64; 4], t: f64) -> [f64; 4] {
    let mut cosine = dot(a, b);
    if cosine < 0. {
        b = b.map(|v| -v);
        cosine = -cosine;
    }
    let cosine = cosine.clamp(-1., 1.);
    if cosine > 0.9995 {
        return normalize(std::array::from_fn(|i| a[i] * (1. - t) + b[i] * t));
    }
    let theta = cosine.acos();
    let scale = theta.sin();
    normalize(std::array::from_fn(|i| {
        (a[i] * ((1. - t) * theta).sin() + b[i] * (t * theta).sin()) / scale
    }))
}
