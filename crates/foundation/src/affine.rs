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
    pub fn inverse(self) -> Result<Self, String> {
        self.validate()?;
        let [a, b, c, t] = self.0;
        let cross = |u: [f64; 3], v: [f64; 3]| {
            [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ]
        };
        let rows = [cross(b, c), cross(c, a), cross(a, b)];
        let det = a.iter().zip(rows[0]).map(|(a, b)| a * b).sum::<f64>();
        let mut inverse = Self([
            std::array::from_fn(|i| rows[i][0] / det),
            std::array::from_fn(|i| rows[i][1] / det),
            std::array::from_fn(|i| rows[i][2] / det),
            [0.; 3],
        ]);
        inverse.0[3] = inverse.vector(t).map(|v| -v);
        inverse.validate()?;
        Ok(inverse)
    }
    /// Exact TRS representation within floating-point tolerance; shear is rejected.
    pub fn trs_components(self) -> Result<([f64; 3], [f64; 4], [f64; 3]), String> {
        self.validate()?;
        let scale = std::array::from_fn(|i| self.0[i].iter().map(|v| v * v).sum::<f64>().sqrt());
        let r: [[f64; 3]; 3] = std::array::from_fn(|i| self.0[i].map(|v| v / scale[i]));
        for (a, b) in [(0, 1), (0, 2), (1, 2)] {
            if r[a].iter().zip(r[b]).map(|(a, b)| a * b).sum::<f64>().abs() > 1e-10 {
                return Err("World-preserving move requires shear, which the native local transform cannot represent".into());
            }
        }
        let trace = r[0][0] + r[1][1] + r[2][2];
        let q = if trace > 0. {
            let s = (trace + 1.).sqrt() * 2.;
            [
                (r[1][2] - r[2][1]) / s,
                (r[2][0] - r[0][2]) / s,
                (r[0][1] - r[1][0]) / s,
                s / 4.,
            ]
        } else {
            let i = if r[0][0] > r[1][1] && r[0][0] > r[2][2] {
                0
            } else if r[1][1] > r[2][2] {
                1
            } else {
                2
            };
            let j = (i + 1) % 3;
            let k = (i + 2) % 3;
            let s = (1. + r[i][i] - r[j][j] - r[k][k]).max(0.).sqrt() * 2.;
            let mut q = [0.; 4];
            q[i] = s / 4.;
            q[j] = (r[i][j] + r[j][i]) / s;
            q[k] = (r[i][k] + r[k][i]) / s;
            q[3] = (r[j][k] - r[k][j]) / s;
            q
        };
        let length = q.iter().map(|v| v * v).sum::<f64>().sqrt();
        let q = q.map(|v| v / length);
        let rebuilt = Self::trs(self.0[3], q, scale);
        if rebuilt
            .0
            .iter()
            .flatten()
            .zip(self.0.iter().flatten())
            .any(|(a, b)| (a - b).abs() > 1e-9 * b.abs().max(1.))
        {
            return Err("World-preserving move cannot retain this transform exactly".into());
        }
        Ok((self.0[3], q, scale))
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
    fn inverse_and_trs_round_trip_rotations_and_reject_shear() {
        for axis in 0..3 {
            for angle in [0., 0.8_f64, std::f64::consts::PI, 4.] {
                let mut q = [0., 0., 0., (angle / 2.).cos()];
                q[axis] = (angle / 2.).sin();
                let matrix = Affine::trs([3., -4., 2.], q, [2., 3., 4.]);
                let (p, r, s) = matrix.trs_components().unwrap();
                let restored = Affine::trs(p, r, s);
                for point in [[0.; 3], [2., -3., 5.]] {
                    let a = restored.point(point);
                    let b = matrix.point(point);
                    assert!(a.iter().zip(b).all(|(a, b)| (a - b).abs() < 1e-10));
                    let inverted = matrix.inverse().unwrap().point(b);
                    assert!(
                        inverted
                            .iter()
                            .zip(point)
                            .all(|(a, b)| (a - b).abs() < 1e-10)
                    );
                }
            }
        }
        let mut sheared = Affine::IDENTITY;
        sheared.0[1][0] = 0.3;
        assert!(sheared.trs_components().is_err());
    }
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
