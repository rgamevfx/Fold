//! Compositor-owned property paths and sampled parameter values.
use crate::{Node, Parameters};
use fold_animation::Curve;
use fold_foundation::Time;
use std::collections::BTreeMap;

pub struct Property {
    pub id: &'static str,
    pub label: &'static str,
    pub unit: &'static str,
    pub channels: &'static [&'static str],
    pub values: Vec<f64>,
    pub range: Option<[f64; 2]>,
}
impl Parameters {
    pub fn animated_properties(&self) -> Vec<Property> {
        let property = |id, label, unit, channels, values, range| Property {
            id,
            label,
            unit,
            channels,
            values,
            range,
        };
        match self {
            Self::Transform {
                translate,
                scale,
                opacity,
            } => vec![
                property(
                    "position",
                    "Position",
                    "px",
                    &["X", "Y"],
                    translate.to_vec(),
                    Some([-1e6, 1e6]),
                ),
                property(
                    "scale",
                    "Scale",
                    "",
                    &["X", "Y"],
                    scale.to_vec(),
                    Some([-1e6, 1e6]),
                ),
                property(
                    "opacity",
                    "Opacity",
                    "",
                    &["Value"],
                    vec![f64::from(*opacity)],
                    Some([0., 1.]),
                ),
            ],
            Self::Crop { rect } | Self::Mask { rect } => vec![property(
                "bounds",
                "Bounds",
                "px",
                &["Left", "Top", "Right", "Bottom"],
                rect.iter().map(|v| f64::from(*v)).collect(),
                Some([0., 16384.]),
            )],
            Self::Blur { radius } => vec![property(
                "radius",
                "Radius",
                "px",
                &["Value"],
                vec![f64::from(*radius)],
                Some([0., 64.]),
            )],
            Self::Grade { gain } => vec![property(
                "gain",
                "Gain",
                "",
                &["R", "G", "B"],
                gain.iter().map(|v| f64::from(*v)).collect(),
                Some([0., 16.]),
            )],
            // Solid's authored colors are straight, evaluated output is premultiplied.
            Self::Solid { rgba } => {
                let alpha = f64::from(rgba[3]);
                let mut values: Vec<_> = rgba.iter().map(|v| f64::from(*v)).collect();
                if alpha > 0. {
                    for v in &mut values[..3] {
                        *v /= alpha;
                    }
                }
                vec![property(
                    "color",
                    "Color",
                    "",
                    &["R", "G", "B", "A"],
                    values,
                    None,
                )]
            }
            _ => vec![],
        }
    }
    pub fn set_property(&mut self, id: &str, values: &[f64]) -> Result<(), String> {
        match (self, id, values) {
            (Self::Transform { translate, .. }, "position", [x, y]) => *translate = [*x, *y],
            (Self::Transform { scale, .. }, "scale", [x, y]) => *scale = [*x, *y],
            (Self::Transform { opacity, .. }, "opacity", [v]) => *opacity = v.clamp(0., 1.) as f32,
            (Self::Crop { rect } | Self::Mask { rect }, "bounds", [a, b, c, d]) => {
                *rect = [*a, *b, *c, *d].map(|v| v.clamp(0., f64::from(u32::MAX)).round() as u32)
            }
            (Self::Blur { radius }, "radius", [v]) => *radius = v.clamp(0., 64.).round() as u32,
            (Self::Grade { gain }, "gain", [r, g, b]) => {
                *gain = [*r, *g, *b].map(|v| v.clamp(0., 16.) as f32)
            }
            (Self::Solid { rgba }, "color", [r, g, b, a]) => {
                let a = a.clamp(0., 1.);
                *rgba = [(r * a) as f32, (g * a) as f32, (b * a) as f32, a as f32];
            }
            _ => return Err("Unknown compositor animation property".into()),
        }
        Ok(())
    }
}
pub fn path(property: &str, component: usize) -> String {
    format!("{property}.{component}")
}
impl Node {
    pub fn remove_animation_channel(&mut self, path: &str, time: Time) {
        if let Some(curve) = self.animation.remove(path)
            && let Some(value) = curve.sample(time)
            && let Some((property, component)) = path.rsplit_once('.')
            && let Ok(component) = component.parse::<usize>()
            && let Some(mut p) = self
                .parameters
                .animated_properties()
                .into_iter()
                .find(|p| p.id == property)
            && let Some(v) = p.values.get_mut(component)
        {
            *v = value;
            let _ = self.parameters.set_property(property, &p.values);
        }
    }

    pub fn evaluated_parameters(&self, time: Time) -> Result<Parameters, String> {
        let mut parameters = self.parameters.clone();
        for mut property in parameters.animated_properties() {
            let mut animated = false;
            for (i, value) in property.values.iter_mut().enumerate() {
                if let Some(curve) = self.animation.get(&path(property.id, i)) {
                    *value = curve.sample(time).ok_or("Empty animation channel")?;
                    animated = true;
                }
            }
            if animated {
                parameters.set_property(property.id, &property.values)?;
            }
        }
        parameters.validate()?;
        Ok(parameters)
    }
    pub fn validate_animation(&self) -> Result<(), String> {
        let available: BTreeMap<_, _> = self
            .parameters
            .animated_properties()
            .into_iter()
            .flat_map(|p| (0..p.values.len()).map(move |i| (path(p.id, i), p.range)))
            .collect();
        for (path, curve) in &self.animation {
            if !available.contains_key(path) {
                return Err(format!("Unknown animation channel {path}"));
            }
            curve.validate()?;
        }
        Ok(())
    }
}
pub type Channels = BTreeMap<String, Curve>;
