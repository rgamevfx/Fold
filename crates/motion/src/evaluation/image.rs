use super::{Scope, State, Value};
use crate::{
    registry::Builtin,
    types::{Alpha, Encoding, Literal, Space, Unit},
};
use fold_foundation::ObjectId;
use fold_platform::{image_sampling::Request, image_source::Outside};
use std::collections::BTreeMap;

impl State<'_> {
    pub(super) fn image_texture(
        &mut self,
        scope: &Scope,
        id: ObjectId,
        args: &BTreeMap<&str, Value>,
    ) -> Result<Value, String> {
        let node = self
            .prepared
            .construction
            .definition(scope.definition)?
            .nodes
            .iter()
            .find(|n| n.id == id)
            .ok_or("Missing Image Texture")?;
        let source = crate::images::source(node)?;
        let Literal::Duration { value: time } =
            args.get("time").ok_or("Missing source time")?.primitive()?
        else {
            return Err("Image time requires exact duration".into());
        };
        let text = |name| super::attribute_nodes::text(args, name);
        let outside = match text("outside")? {
            "hold" => Outside::Hold,
            "loop" => Outside::Loop,
            "transparent" => Outside::Black,
            "error" => Outside::Error,
            _ => return Err("Outside Range must be hold, loop, transparent or error".into()),
        };
        let color = text("color_space")?;
        if matches!(source, fold_platform::image_source::Source::Output(_)) && !color.is_empty() {
            return Err("Document outputs already have a working color space".into());
        }
        Ok(Value::Image(Request {
            source,
            time: *time,
            outside,
            color_space: (!color.is_empty()).then(|| color.into()),
        }))
    }
    pub(super) fn sample_image(
        &mut self,
        args: &BTreeMap<&str, Value>,
        builtin: Builtin,
    ) -> Result<Value, String> {
        let Some(Value::Image(request)) = args.get("image") else {
            return Err("Sample Image requires Image Texture".into());
        };
        let Literal::Vector {
            value: uv,
            unit: Unit::Unitless,
            space: Space::World,
        } = args.get("uv").ok_or("Missing UV")?.primitive()?
        else {
            return Err("UV requires a unitless two-component vector".into());
        };
        if uv.len() != 2 || uv.iter().any(|v| !v.is_finite() || v.abs() > 1e9) {
            return Err("UV must be finite and within ±1e9".into());
        }
        let filter = super::attribute_nodes::text(args, "filter")?;
        let wrap = super::attribute_nodes::text(args, "wrap")?;
        if !matches!(filter, "nearest" | "bilinear")
            || !matches!(wrap, "clamp" | "repeat" | "mirror" | "transparent")
        {
            return Err("Unsupported image filter or wrap".into());
        }
        let channel = if builtin == Builtin::SampleImageScalar {
            super::attribute_nodes::text(args, "channel")?
        } else {
            "color"
        };
        if builtin == Builtin::SampleImageScalar
            && !matches!(channel, "luminance" | "red" | "green" | "blue" | "alpha")
        {
            return Err("Unsupported image channel".into());
        }
        let frame = self
            .prepared
            .images
            .ok_or("Image sampling requires a snapshot-bound image service")?
            .frame(request);
        self.budget.step()?;
        let frame = frame?;
        let mut rgba = sample(&frame, [uv[0], uv[1]], filter, wrap);
        // Filter premultiplied pixels first, then return straight linear color.
        let alpha = rgba[3];
        if alpha > 0. {
            for c in &mut rgba[..3] {
                *c /= alpha;
            }
        } else {
            rgba[..3].fill(0.);
        }
        if frame.working_space() == fold_render::WorkingSpace::AcesCg {
            rgba = fold_platform::color::aces_to_linear_rec709(rgba);
        }
        let literal = if channel == "color" {
            let Literal::Bool { value: clamp } = args
                .get("clamp_negative")
                .ok_or("Missing color gamut policy")?
                .primitive()?
            else {
                return Err("Clamp Negative RGB requires a boolean".into());
            };
            if *clamp {
                for component in &mut rgba[..3] {
                    *component = component.max(0.);
                }
            }
            Literal::Color {
                value: rgba,
                encoding: Encoding::LinearRec709,
                alpha: Alpha::Straight,
            }
        } else {
            Literal::Scalar {
                value: match channel {
                    "red" => rgba[0],
                    "green" => rgba[1],
                    "blue" => rgba[2],
                    "alpha" => rgba[3],
                    _ => rgba[0] * 0.2126 + rgba[1] * 0.7152 + rgba[2] * 0.0722,
                },
            }
        };
        literal.validate()?;
        Ok(Value::Primitive(literal))
    }
}
fn sample(frame: &fold_render::Frame, uv: [f64; 2], filter: &str, wrap: &str) -> [f64; 4] {
    sample_pixels(frame.pixels(), frame.dimensions(), uv, filter, wrap)
}
fn sample_pixels(
    pixels: &[[f32; 4]],
    [width, height]: [u32; 2],
    uv: [f64; 2],
    filter: &str,
    wrap: &str,
) -> [f64; 4] {
    let coordinate = |v: i64, n: u32| -> Option<usize> {
        let n = i64::from(n);
        Some(match wrap {
            "repeat" => v.rem_euclid(n),
            "mirror" => {
                let t = v.rem_euclid(n * 2);
                if t < n { t } else { n * 2 - 1 - t }
            }
            "transparent" if v < 0 || v >= n => return None,
            _ => v.clamp(0, n - 1),
        } as usize)
    };
    let at = |x, y| match (coordinate(x, width), coordinate(y, height)) {
        (Some(x), Some(y)) => pixels[y * width as usize + x].map(f64::from),
        _ => [0.; 4],
    };
    let [x, y] = [
        uv[0] * f64::from(width) - 0.5,
        uv[1] * f64::from(height) - 0.5,
    ];
    if filter == "nearest" {
        return at((x + 0.5).floor() as i64, (y + 0.5).floor() as i64);
    }
    let [ix, iy] = [x.floor() as i64, y.floor() as i64];
    let [fx, fy] = [x - x.floor(), y - y.floor()];
    let [a, b, c, d] = [
        at(ix, iy),
        at(ix + 1, iy),
        at(ix, iy + 1),
        at(ix + 1, iy + 1),
    ];
    std::array::from_fn(|i| {
        (a[i] * (1. - fx) + b[i] * fx) * (1. - fy) + (c[i] * (1. - fx) + d[i] * fx) * fy
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn hdr_samples_preserve_signed_values_and_filter_premultiplied_edges() {
        let pixels = [[2., -0.5, 0.25, 0.5], [0.; 4]];
        assert_eq!(
            sample_pixels(&pixels, [2, 1], [0.5, 0.5], "bilinear", "clamp"),
            [1., -0.25, 0.125, 0.25]
        );
        let source = [4., -1., 0.5, 0.25];
        let aces = fold_platform::color::linear_rec709_to_aces(source);
        let restored = fold_platform::color::aces_to_linear_rec709(aces);
        for (actual, expected) in restored.into_iter().zip(source) {
            assert!((actual - expected).abs() < 1e-5);
        }
    }
}
