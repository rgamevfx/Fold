//! Column-major affine transforms. Composition retains shear without TRS decomposition.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Affine(pub [[f64; 3]; 4]);
impl Affine {
    pub const IDENTITY: Self = Self([[1., 0., 0.], [0., 1., 0.], [0., 0., 1.], [0., 0., 0.]]);
    pub fn trs(position: [f64; 3], rotation: [f64; 4], scale: [f64; 3]) -> Self {
        let [x, y, z, w] = rotation;
        let mut columns = [
            [
                1. - 2. * (y * y + z * z),
                2. * (x * y + z * w),
                2. * (x * z - y * w),
            ],
            [
                2. * (x * y - z * w),
                1. - 2. * (x * x + z * z),
                2. * (y * z + x * w),
            ],
            [
                2. * (x * z + y * w),
                2. * (y * z - x * w),
                1. - 2. * (x * x + y * y),
            ],
            position,
        ];
        for i in 0..3 {
            columns[i] = columns[i].map(|v| v * scale[i]);
        }
        Self(columns)
    }
    pub fn vector(self, v: [f64; 3]) -> [f64; 3] {
        std::array::from_fn(|i| self.0[0][i] * v[0] + self.0[1][i] * v[1] + self.0[2][i] * v[2])
    }
    pub fn point(self, p: [f64; 3]) -> [f64; 3] {
        let p = self.vector(p);
        std::array::from_fn(|i| p[i] + self.0[3][i])
    }
    pub fn compose(self, local: Self) -> Self {
        Self([
            self.vector(local.0[0]),
            self.vector(local.0[1]),
            self.vector(local.0[2]),
            self.point(local.0[3]),
        ])
    }
    pub fn validate(self) -> Result<(), String> {
        let [a, b, c, _] = self.0;
        let det = a[0] * (b[1] * c[2] - b[2] * c[1]) - b[0] * (a[1] * c[2] - a[2] * c[1])
            + c[0] * (a[1] * b[2] - a[2] * b[1]);
        if self
            .0
            .iter()
            .flatten()
            .any(|v| !v.is_finite() || v.abs() > 1e6)
            || !det.is_finite()
            || det <= 0.
        {
            return Err(
                "Affine transform must be finite, bounded and orientation preserving".into(),
            );
        }
        // The GPU uses f32 matrices and an inverse-transpose normal matrix.
        let lengths = [a, b, c].map(|v| v.iter().map(|x| x * x).sum::<f64>().sqrt());
        if lengths.iter().any(|v| !(1e-6..=1e6).contains(v))
            || det / lengths.iter().product::<f64>() < 1e-6
        {
            return Err("Affine transform is singular or too ill-conditioned for rendering".into());
        }
        Ok(())
    }
    pub fn matrix_f32(self) -> [[f32; 4]; 4] {
        std::array::from_fn(|i| {
            [
                self.0[i][0] as f32,
                self.0[i][1] as f32,
                self.0[i][2] as f32,
                if i == 3 { 1. } else { 0. },
            ]
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn nonuniform_parent_and_rotated_child_preserve_shear() {
        let parent = Affine::trs([2., 1., 0.], [0., 0., 0., 1.], [2., 1., 3.]);
        let angle = std::f64::consts::FRAC_PI_8;
        let child = Affine::trs([1., 0., 0.], [0., 0., angle.sin(), angle.cos()], [1.; 3]);
        let combined = parent.compose(child);
        combined.validate().unwrap();
        for p in [[0.; 3], [1., 2., 3.], [-2., 4., 1.]] {
            let expected = parent.point(child.point(p));
            let actual = combined.point(p);
            assert!(
                actual
                    .into_iter()
                    .zip(expected)
                    .all(|(a, b)| (a - b).abs() < 1e-12)
            );
        }
        assert!(
            combined.0[0]
                .iter()
                .zip(combined.0[1])
                .map(|(a, b)| a * b)
                .sum::<f64>()
                .abs()
                > 0.1
        );
        assert!(
            Affine::trs([0.; 3], [0., 0., 0., 1.], [0., 1., 1.])
                .validate()
                .is_err()
        );
    }
}
