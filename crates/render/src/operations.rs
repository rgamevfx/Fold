//! Versioned image mathematics shared by authoring and the CPU reference.
use serde::{Deserialize, Serialize};
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum MergeMode {
    #[default]
    Over,
    Plus,
    Multiply,
    Screen,
    Difference,
    Min,
    Max,
    In,
    Out,
    Atop,
    Xor,
    Copy,
}
impl MergeMode {
    pub const ALL: [Self; 12] = [
        Self::Over,
        Self::Plus,
        Self::Multiply,
        Self::Screen,
        Self::Difference,
        Self::Min,
        Self::Max,
        Self::In,
        Self::Out,
        Self::Atop,
        Self::Xor,
        Self::Copy,
    ];
    pub fn label(self) -> &'static str {
        match self {
            Self::Over => "Over",
            Self::Plus => "Plus",
            Self::Multiply => "Multiply",
            Self::Screen => "Screen",
            Self::Difference => "Difference",
            Self::Min => "Min",
            Self::Max => "Max",
            Self::In => "In",
            Self::Out => "Out",
            Self::Atop => "Atop",
            Self::Xor => "Xor",
            Self::Copy => "Copy",
        }
    }
    pub(crate) fn apply(self, a: [f32; 4], b: [f32; 4]) -> [f32; 4] {
        let mut result = std::array::from_fn(|c| match self {
            Self::Over => a[c] + b[c] * (1. - a[3]),
            Self::Plus => a[c] + b[c],
            Self::Multiply => a[c] * b[c],
            Self::Screen => a[c] + b[c] - a[c] * b[c],
            Self::Difference => (a[c] - b[c]).abs(),
            Self::Min => a[c].min(b[c]),
            Self::Max => a[c].max(b[c]),
            Self::In => a[c] * b[3],
            Self::Out => a[c] * (1. - b[3]),
            Self::Atop => a[c] * b[3] + b[c] * (1. - a[3]),
            Self::Xor => a[c] * (1. - b[3]) + b[c] * (1. - a[3]),
            Self::Copy => a[c],
        });
        // Coverage remains bounded while RGB can be signed and above one.
        result[3] = result[3].clamp(0., 1.);
        result
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grade {
    pub black: [f32; 3],
    pub white: [f32; 3],
    pub lift: [f32; 3],
    pub gain: [f32; 3],
    pub multiply: [f32; 3],
    pub offset: [f32; 3],
    pub gamma: [f32; 3],
    pub unpremultiply: bool,
    pub black_clamp: bool,
    pub white_clamp: bool,
    pub invert: bool,
}
impl Default for Grade {
    fn default() -> Self {
        Self {
            black: [0.; 3],
            white: [1.; 3],
            lift: [0.; 3],
            gain: [1.; 3],
            multiply: [1.; 3],
            offset: [0.; 3],
            gamma: [1.; 3],
            unpremultiply: false,
            black_clamp: true,
            white_clamp: false,
            invert: false,
        }
    }
}
impl Grade {
    pub fn validate(&self) -> Result<(), String> {
        if [
            self.black,
            self.white,
            self.lift,
            self.gain,
            self.multiply,
            self.offset,
            self.gamma,
        ]
        .into_iter()
        .flatten()
        .any(|v| !v.is_finite())
            || self.gamma.iter().any(|&v| v <= 0.)
            || (0..3).any(|c| {
                self.white[c] <= self.black[c] || !(self.white[c] - self.black[c]).is_finite()
            })
        {
            return Err(
                "Grade requires finite values, positive gamma and white point above black point"
                    .into(),
            );
        }
        if self.invert && (0..3).any(|c| self.gain[c] == self.lift[c] || self.multiply[c] == 0.) {
            return Err("Cannot invert a Grade with zero gain range or multiply".into());
        }
        Ok(())
    }
    pub(crate) fn apply(&self, pixel: [f32; 4]) -> [f32; 4] {
        if self.unpremultiply && pixel[3] == 0. {
            return [0.; 4];
        }
        let mut result = pixel;
        for c in 0..3 {
            let value = if self.unpremultiply {
                pixel[c] / pixel[3]
            } else {
                pixel[c]
            };
            let slope =
                (self.gain[c] - self.lift[c]) * self.multiply[c] / (self.white[c] - self.black[c]);
            let intercept =
                self.offset[c] + self.lift[c] * self.multiply[c] - slope * self.black[c];
            let mut value = if self.invert {
                (value.signum() * value.abs().powf(self.gamma[c]) - intercept) / slope
            } else {
                let value = value * slope + intercept;
                value.signum() * value.abs().powf(1. / self.gamma[c])
            };
            if self.black_clamp {
                value = value.max(0.);
            }
            if self.white_clamp {
                value = value.min(1.);
            }
            result[c] = if self.unpremultiply {
                value * pixel[3]
            } else {
                value
            };
        }
        result
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum Filter {
    Nearest,
    #[default]
    Linear,
    Cubic,
}
impl Filter {
    pub const ALL: [Self; 3] = [Self::Nearest, Self::Linear, Self::Cubic];
    pub fn label(self) -> &'static str {
        match self {
            Self::Nearest => "Nearest",
            Self::Linear => "Linear",
            Self::Cubic => "Cubic",
        }
    }
}
pub(crate) fn sample(
    pixels: &[[f32; 4]],
    dimensions: [u32; 2],
    point: [f64; 2],
    filter: Filter,
    edges: Edges,
    clamp: bool,
) -> [f32; 4] {
    let [width, height] = dimensions;
    let point = if edges == Edges::Clamp {
        [
            point[0].clamp(0.5, f64::from(width) - 0.5),
            point[1].clamp(0.5, f64::from(height) - 0.5),
        ]
    } else {
        point
    };
    if point.iter().any(|v| !v.is_finite())
        || point[0] < -2.
        || point[1] < -2.
        || point[0] > f64::from(width) + 2.
        || point[1] > f64::from(height) + 2.
    {
        return [0.; 4];
    }
    let read = |x: i64, y: i64| {
        let (x, y) = if edges == Edges::Clamp {
            (
                x.clamp(0, i64::from(width) - 1),
                y.clamp(0, i64::from(height) - 1),
            )
        } else {
            (x, y)
        };
        if x >= 0 && y >= 0 && x < i64::from(width) && y < i64::from(height) {
            pixels[(y * i64::from(width) + x) as usize]
        } else {
            [0.; 4]
        }
    };
    if filter == Filter::Nearest {
        return read(point[0].floor() as i64, point[1].floor() as i64);
    }
    let point = [point[0] - 0.5, point[1] - 0.5];
    let base = point.map(|v| v.floor() as i64);
    let fraction = [
        (point[0] - base[0] as f64) as f32,
        (point[1] - base[1] as f64) as f32,
    ];
    let mut result = [0.; 4];
    let (first, last) = if filter == Filter::Cubic {
        (-1, 2)
    } else {
        (0, 1)
    };
    for y in first..=last {
        for x in first..=last {
            let weight = if filter == Filter::Cubic {
                cubic(x as f32 - fraction[0]) * cubic(y as f32 - fraction[1])
            } else {
                (if x == 0 {
                    1. - fraction[0]
                } else {
                    fraction[0]
                }) * (if y == 0 {
                    1. - fraction[1]
                } else {
                    fraction[1]
                })
            };
            let pixel = read(base[0] + x, base[1] + y);
            for c in 0..4 {
                result[c] += pixel[c] * weight;
            }
        }
    }
    if clamp {
        for c in 0..3 {
            let samples = [
                read(base[0], base[1])[c],
                read(base[0] + 1, base[1])[c],
                read(base[0], base[1] + 1)[c],
                read(base[0] + 1, base[1] + 1)[c],
            ];
            result[c] = result[c].clamp(
                samples.into_iter().fold(f32::INFINITY, f32::min),
                samples.into_iter().fold(f32::NEG_INFINITY, f32::max),
            );
        }
    }
    result[3] = result[3].clamp(0., 1.);
    result
}
fn cubic(value: f32) -> f32 {
    let x = value.abs();
    if x < 1. {
        (1.5 * x - 2.5) * x * x + 1.
    } else if x < 2. {
        ((-0.5 * x + 2.5) * x - 4.) * x + 2.
    } else {
        0.
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[repr(u32)]
pub enum Edges {
    #[default]
    Transparent,
    Clamp,
}
pub fn validate_blur(size: [f32; 2]) -> Result<(), String> {
    if size
        .iter()
        .any(|v| !v.is_finite() || !(0.0..=256.).contains(v))
    {
        return Err("Blur size must be finite and in 0..256 pixels".into());
    }
    Ok(())
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub enum BlurFilter {
    #[default]
    Gaussian,
    Box,
}
impl BlurFilter {
    pub fn label(self) -> &'static str {
        match self {
            Self::Gaussian => "Gaussian",
            Self::Box => "Box",
        }
    }
    pub(crate) fn weights(self, size: f32) -> Vec<f32> {
        let radius = match self {
            Self::Gaussian => (3. * size).ceil(),
            Self::Box => size.ceil(),
        } as i32;
        (-radius..=radius)
            .map(|k| match self {
                Self::Gaussian if size > 0. => (-0.5 * (k as f32 / size).powi(2)).exp(),
                Self::Gaussian => 1.,
                Self::Box => (size + 1. - k.abs() as f32).clamp(0., 1.),
            })
            .collect()
    }
}
pub(crate) fn filtered_blur(
    pixels: &[[f32; 4]],
    result: &mut Vec<[f32; 4]>,
    dimensions: [u32; 2],
    size: [f32; 2],
    filter: BlurFilter,
    edges: Edges,
    cancel: &fold_media::Cancel,
) -> Result<(), fold_foundation::Failure> {
    validate_blur(size)?;
    let [width, height] = dimensions;
    let _scratch = fold_media::budget::reserve_working(pixels.len() as u64 * 16)?;
    let mut scratch = vec![[0.; 4]; pixels.len()];
    result.resize(pixels.len(), [0.; 4]);
    for (axis, sigma) in size.into_iter().enumerate() {
        let weights = filter.weights(sigma);
        let radius = (weights.len() / 2) as i32;
        let divisor: f32 = weights.iter().sum();
        let (input, output) = if axis == 0 {
            (pixels, scratch.as_mut_slice())
        } else {
            (scratch.as_slice(), result.as_mut_slice())
        };
        for y in 0..height as i32 {
            cancel.check()?;
            for x in 0..width as i32 {
                let mut value = [0.; 4];
                for (i, &weight) in weights.iter().enumerate() {
                    let mut sx = x + if axis == 0 { i as i32 - radius } else { 0 };
                    let mut sy = y + if axis == 1 { i as i32 - radius } else { 0 };
                    if edges == Edges::Clamp {
                        sx = sx.clamp(0, width as i32 - 1);
                        sy = sy.clamp(0, height as i32 - 1);
                    }
                    if sx >= 0 && sy >= 0 && sx < width as i32 && sy < height as i32 {
                        let pixel = input[sy as usize * width as usize + sx as usize];
                        for c in 0..4 {
                            value[c] += pixel[c] * weight;
                        }
                    }
                }
                value = value.map(|v| v / divisor);
                value[3] = value[3].clamp(0., 1.);
                output[y as usize * width as usize + x as usize] = value;
            }
        }
    }
    Ok(())
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Unary {
    Exposure { stops: f32 },
    Invert,
    Clamp { minimum: f32, maximum: f32 },
    Premult,
    Unpremult,
}
impl Unary {
    pub fn label(&self) -> &'static str {
        match self {
            Self::Exposure { .. } => "Exposure",
            Self::Invert => "Invert",
            Self::Clamp { .. } => "Clamp",
            Self::Premult => "Premult",
            Self::Unpremult => "Unpremult",
        }
    }
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Exposure { stops } if !stops.is_finite() || stops.abs() > 32. => {
                Err("Exposure must be finite and within ±32 stops".into())
            }
            Self::Clamp { minimum, maximum }
                if !minimum.is_finite() || !maximum.is_finite() || minimum > maximum =>
            {
                Err("Clamp requires finite minimum ≤ maximum".into())
            }
            _ => Ok(()),
        }
    }
    pub(crate) fn apply(&self, pixel: [f32; 4]) -> [f32; 4] {
        let mut output = pixel;
        for c in 0..3 {
            output[c] = match *self {
                Self::Exposure { stops } => pixel[c] * stops.exp2(),
                Self::Invert => 1. - pixel[c],
                Self::Clamp { minimum, maximum } => pixel[c].clamp(minimum, maximum),
                Self::Premult => pixel[c] * pixel[3],
                Self::Unpremult => {
                    if pixel[3] == 0. {
                        0.
                    } else {
                        pixel[c] / pixel[3]
                    }
                }
            };
        }
        output
    }
}

#[cfg(test)]
mod node_tests {
    use super::*;
    #[test]
    fn grade_clamps_and_inverse_are_explicit() {
        let grade = Grade {
            black_clamp: false,
            gain: [2.; 3],
            offset: [-0.5; 3],
            gamma: [2.; 3],
            ..Default::default()
        };
        let pixel = [0.1, 0.5, 1.5, 0.4];
        let result = grade.apply(pixel);
        let inverse = Grade {
            invert: true,
            ..grade.clone()
        };
        for (actual, expected) in inverse.apply(result).into_iter().zip(pixel) {
            assert!((actual - expected).abs() < 1e-6);
        }
        let clipped = Grade {
            black_clamp: true,
            white_clamp: true,
            ..grade
        }
        .apply(pixel);
        assert_eq!(clipped[0], 0.);
        assert_eq!(clipped[2], 1.);
        assert_eq!(clipped[3], pixel[3]);
        assert!(
            Grade {
                invert: true,
                multiply: [0.; 3],
                ..Default::default()
            }
            .validate()
            .is_err()
        );
    }
    #[test]
    fn fractional_box_blur_has_expected_coverage_and_zero_is_identity() {
        let source = [[0., 0., 0., 0.], [1., 1., 1., 1.], [0., 0., 0., 0.]];
        let mut result = vec![];
        filtered_blur(
            &source,
            &mut result,
            [3, 1],
            [0.5, 0.],
            BlurFilter::Box,
            Edges::Transparent,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(result, [[0.25; 4], [0.5; 4], [0.25; 4]]);
        filtered_blur(
            &source,
            &mut result,
            [3, 1],
            [0., 0.],
            BlurFilter::Box,
            Edges::Transparent,
            &Default::default(),
        )
        .unwrap();
        assert_eq!(result, source);
    }
}
