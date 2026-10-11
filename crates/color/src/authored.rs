//! Pinned sRGB picker ↔ straight ACEScg conversion, independent of display views.
//! Matrices are OCIO 2.4.2 Studio 2.2.0's linear Rec.709/ACEScg transforms.
const TO_ACES: [[f64; 3]; 3] = [
    [0.6130974293, 0.3395231366, 0.0473794527],
    [0.0701937228, 0.9163538814, 0.0134523986],
    [0.0206155926, 0.1095697731, 0.8698146343],
];
const FROM_ACES: [[f64; 3]; 3] = [
    [1.7050509453, -0.6217921376, -0.0832588747],
    [-0.1302564144, 1.1408047676, -0.0105483187],
    [-0.0240033567, -0.128968969, 1.1529723406],
];
fn matrix(v: [f64; 4], m: [[f64; 3]; 3]) -> [f64; 4] {
    [
        m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
        m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
        m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
        v[3],
    ]
}
/// Straight scene-linear Rec.709 to ACEScg. Preserves alpha and signed HDR;
/// no transfer function, display transform or premultiplication is applied.
pub fn linear_rec709_to_aces(v: [f64; 4]) -> [f64; 4] {
    matrix(v, TO_ACES)
}
/// ACEScg to straight scene-linear Rec.709; preserves signed HDR and alpha.
pub fn aces_to_linear_rec709(v: [f64; 4]) -> [f64; 4] {
    matrix(v, FROM_ACES)
}
pub fn from_picker(mut v: [f64; 4], aces: bool) -> [f64; 4] {
    for c in &mut v[..3] {
        *c = if *c <= 0.04045 {
            *c / 12.92
        } else {
            ((*c + 0.055) / 1.055).powf(2.4)
        };
    }
    if aces { linear_rec709_to_aces(v) } else { v }
}
pub fn to_picker(v: [f64; 4], aces: bool) -> [f64; 4] {
    let mut v = if aces { matrix(v, FROM_ACES) } else { v };
    for c in &mut v[..3] {
        *c = if *c <= 0.0031308 {
            12.92 * *c
        } else {
            1.055 * c.powf(1. / 2.4) - 0.055
        };
    }
    v
}
#[cfg(test)]
mod tests {
    #[test]
    fn linear_rec709_conversion_preserves_hdr_alpha_without_transfer_or_clamping() {
        let source = [-0.25, 2., 0.18, 0.3];
        let converted = super::linear_rec709_to_aces(source);
        assert_eq!(converted[3], source[3]);
        assert!(converted[1] > 1.);
        let restored = super::matrix(converted, super::FROM_ACES);
        for (actual, expected) in restored.into_iter().zip(source) {
            assert!((actual - expected).abs() < 0.00002);
        }
        let gray = super::linear_rec709_to_aces([0.18, 0.18, 0.18, 0.5]);
        assert!((gray[0] - 0.18).abs() < 1e-6);
    }

    #[test]
    fn picker_roundtrip_and_hdr_are_not_clamped() {
        for source in [[1., 0., 0., 0.25], [-0.2, 2., 0.18, 1.]] {
            let result = super::to_picker(super::from_picker(source, true), true);
            for (a, b) in source.into_iter().zip(result) {
                assert!((a - b).abs() < 0.00002, "{a} != {b}");
            }
        }
    }
}
