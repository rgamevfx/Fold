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
            unpremultiply: true,
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
        Ok(())
    }
    pub(crate) fn apply(&self, pixel: [f32; 4]) -> [f32; 4] {
        if self == &Self::default() {
            return pixel;
        }
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
            let value = ((value - self.black[c]) / (self.white[c] - self.black[c])
                * (self.gain[c] - self.lift[c])
                + self.lift[c])
                * self.multiply[c]
                + self.offset[c];
            let value = value.signum() * value.abs().powf(1. / self.gamma[c]);
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
) -> [f32; 4] {
    let [width, height] = dimensions;
    if point.iter().any(|v| !v.is_finite())
        || point[0] < -2.
        || point[1] < -2.
        || point[0] > f64::from(width) + 2.
        || point[1] > f64::from(height) + 2.
    {
        return [0.; 4];
    }
    let read = |x: i64, y: i64| {
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
        return Err("Gaussian size must be finite and in 0..256 pixels".into());
    }
    Ok(())
}
pub(crate) fn gaussian_weights(sigma: f32) -> Vec<f32> {
    let radius = (3. * sigma).ceil() as i32;
    (-radius..=radius)
        .map(|k| {
            if sigma == 0. {
                1.
            } else {
                (-0.5 * (k as f32 / sigma).powi(2)).exp()
            }
        })
        .collect()
}
pub(crate) fn gaussian(
    pixels: &[[f32; 4]],
    result: &mut Vec<[f32; 4]>,
    dimensions: [u32; 2],
    size: [f32; 2],
    edges: Edges,
    cancel: &fold_media::Cancel,
) -> Result<(), String> {
    validate_blur(size)?;
    let [width, height] = dimensions;
    let _scratch = fold_media::budget::reserve_working(pixels.len() as u64 * 16)?;
    let mut scratch = vec![[0.; 4]; pixels.len()];
    result.resize(pixels.len(), [0.; 4]);
    for (axis, sigma) in size.into_iter().enumerate() {
        let radius = (3. * sigma).ceil() as i32;
        let weights = gaussian_weights(sigma);
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
