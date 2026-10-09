//! Compositor-owned property paths and sampled parameter values.
use crate::{Node, OperationConfig};
mod bindings;
pub use bindings::Channels;
use fold_foundation::Time;
use std::collections::BTreeMap;

pub struct Property {
    pub id: String,
    pub label: String,
    pub unit: &'static str,
    pub channels: &'static [&'static str],
    pub values: Vec<f64>,
    pub range: Option<[f64; 2]>,
}
impl OperationConfig {
    /// Native static record owning an animatable control's authored base.
    pub(crate) fn parameter_for_property<'a>(&self, property: &'a str) -> &'a str {
        match self {
            Self::Shader { .. }
            | Self::RelativeTransform { .. }
            | Self::TransformImage { .. }
            | Self::ColorGrade { .. } => "settings",
            Self::Unary { .. } => "operation",
            Self::Solid { .. } => "rgba",
            Self::Crop { .. } | Self::Mask { .. } => "rect",
            _ => property,
        }
    }
    pub fn animated_properties(&self) -> Vec<Property> {
        let property = |id: &str, label: &str, unit, channels, values, range| Property {
            id: id.into(),
            label: label.into(),
            unit,
            channels,
            values,
            range,
        };
        match self {
            Self::Shader { settings } => settings
                .controls
                .iter()
                .map(|c| Property {
                    id: c.id.clone(),
                    label: c.label.clone(),
                    unit: "",
                    channels: match c.default.len() {
                        1 => &["Value"],
                        2 => &["X", "Y"],
                        3 => &["R", "G", "B"],
                        _ => &["R", "G", "B", "A"],
                    },
                    values: c.default.clone(),
                    range: c.range,
                })
                .collect(),
            Self::RelativeTransform { settings } => vec![
                property(
                    "position",
                    "Position",
                    "%",
                    &["X", "Y"],
                    settings.position.to_vec(),
                    None,
                ),
                property(
                    "size",
                    "Size",
                    "%",
                    &["Value"],
                    vec![settings.size],
                    Some([0.001, 10000.]),
                ),
            ],
            Self::Shape { bounds, .. } => vec![property(
                "bounds",
                "Bounds",
                "px",
                &["Left", "Top", "Right", "Bottom"],
                bounds.to_vec(),
                None,
            )],
            Self::Unary { operation } => match operation {
                fold_render::operations::Unary::Exposure { stops } => vec![property(
                    "stops",
                    "Exposure",
                    "stops",
                    &["Value"],
                    vec![f64::from(*stops)],
                    Some([-32., 32.]),
                )],
                fold_render::operations::Unary::Clamp { minimum, maximum } => vec![property(
                    "limits",
                    "Range",
                    "",
                    &["Min", "Max"],
                    vec![f64::from(*minimum), f64::from(*maximum)],
                    None,
                )],
                _ => vec![],
            },
            Self::GaussianBlur { size, .. } => vec![property(
                "size",
                "Size",
                "px",
                &["X", "Y"],
                size.map(f64::from).into(),
                Some([0., 256.]),
            )],
            Self::TransformImage { settings } => vec![
                property(
                    "position",
                    "Position",
                    "px",
                    &["X", "Y"],
                    settings.translate.into(),
                    None,
                ),
                property(
                    "rotate",
                    "Rotate",
                    "°",
                    &["Value"],
                    vec![settings.rotate],
                    None,
                ),
                property(
                    "scale",
                    "Scale",
                    "",
                    &["X", "Y"],
                    settings.scale.into(),
                    None,
                ),
                property(
                    "pivot",
                    "Pivot",
                    "px",
                    &["X", "Y"],
                    settings.pivot.into(),
                    None,
                ),
                property("skew", "Skew", "", &["Value"], vec![settings.skew], None),
            ],
            Self::ColorGrade { settings } => [
                ("black", "Black point", settings.black),
                ("white", "White point", settings.white),
                ("gain", "Gain", settings.gain),
                ("gamma", "Gamma", settings.gamma),
                ("offset", "Offset", settings.offset),
                ("lift", "Lift", settings.lift),
                ("multiply", "Multiply", settings.multiply),
            ]
            .into_iter()
            .map(|(id, label, values)| {
                property(
                    id,
                    label,
                    "",
                    &["R", "G", "B"],
                    values.map(f64::from).into(),
                    if id == "gamma" {
                        Some([0.001, 100.])
                    } else {
                        None
                    },
                )
            })
            .collect(),
            Self::Crop { rect } | Self::Mask { rect } => vec![property(
                "bounds",
                "Bounds",
                "px",
                &["Left", "Top", "Right", "Bottom"],
                rect.iter().map(|v| f64::from(*v)).collect(),
                Some([0., 16384.]),
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
            (Self::Shader { settings }, id, values) => {
                let control = settings
                    .controls
                    .iter_mut()
                    .find(|c| c.id == id)
                    .ok_or("Missing shader control")?;
                if values.len() != control.default.len() || values.iter().any(|v| !v.is_finite()) {
                    return Err("Shader control type mismatch".into());
                }
                control.default = values.to_vec();
            }
            (Self::RelativeTransform { settings }, "position", [x, y]) => {
                settings.position = [*x, *y]
            }
            (Self::RelativeTransform { settings }, "size", [v]) => settings.size = *v,
            (Self::Shape { bounds, .. }, "bounds", [a, b, c, d]) => *bounds = [*a, *b, *c, *d],
            (
                Self::Unary {
                    operation: fold_render::operations::Unary::Exposure { stops },
                },
                "stops",
                [value],
            ) => *stops = *value as f32,
            (
                Self::Unary {
                    operation: fold_render::operations::Unary::Clamp { minimum, maximum },
                },
                "limits",
                [a, b],
            ) => {
                *minimum = *a as f32;
                *maximum = *b as f32;
            }
            (Self::GaussianBlur { size, .. }, "size", [x, y]) => *size = [*x as f32, *y as f32],
            (Self::TransformImage { settings }, "position", [x, y]) => {
                settings.translate = [*x, *y]
            }
            (Self::TransformImage { settings }, "scale", [x, y]) => settings.scale = [*x, *y],
            (Self::TransformImage { settings }, "pivot", [x, y]) => settings.pivot = [*x, *y],
            (Self::TransformImage { settings }, "rotate", [value]) => settings.rotate = *value,
            (Self::TransformImage { settings }, "skew", [value]) => settings.skew = *value,
            (Self::ColorGrade { settings }, id, [r, g, b]) => {
                let target = match id {
                    "black" => &mut settings.black,
                    "white" => &mut settings.white,
                    "lift" => &mut settings.lift,
                    "gain" => &mut settings.gain,
                    "multiply" => &mut settings.multiply,
                    "offset" => &mut settings.offset,
                    "gamma" => &mut settings.gamma,
                    _ => return Err("Unknown grade property".into()),
                };
                *target = [*r as f32, *g as f32, *b as f32];
            }
            (Self::Crop { rect } | Self::Mask { rect }, "bounds", [a, b, c, d]) => {
                *rect = [*a, *b, *c, *d].map(|v| v.clamp(0., f64::from(u32::MAX)).round() as u32)
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
    pub fn animated_properties(&self) -> Vec<Property> {
        let mut properties = self.configuration().animated_properties();
        if self.configuration().operator().supports_effect() {
            properties.push(Property {
                id: "mix".into(),
                label: "Mix".into(),
                unit: "",
                channels: &["Value"],
                values: vec![self.effect.mix as f64],
                range: Some([0., 1.]),
            });
        }
        properties
    }
    pub fn set_property(&mut self, id: &str, values: &[f64]) -> Result<(), String> {
        if id == "mix" && self.configuration().operator().supports_effect() {
            if values.len() != 1 || !values[0].is_finite() || !(0.0..=1.).contains(&values[0]) {
                return Err("Mix must be finite and in 0..1".into());
            }
            self.effect.mix = values[0] as f32;
            Ok(())
        } else {
            let mut config = self.configuration();
            config.set_property(id, values)?;
            self.set_configuration(config)
        }
    }
    pub fn evaluated_effect(&self, time: Time) -> Result<crate::Effect, String> {
        let mut effect = self.effect.clone();
        if let Some(value) = self.animation.sample("mix.0", time)? {
            effect.mix = value as f32;
        }
        effect.validate()?;
        Ok(effect)
    }

    pub fn remove_animation_channel(&mut self, path: &str, time: Time) -> Result<(), String> {
        let value = self.animation.sample(path, time)?;
        self.animation.remove(path)?;
        if let Some(value) = value
            && let Some((property, component)) = path.rsplit_once('.')
            && let Ok(component) = component.parse::<usize>()
            && let Some(mut p) = self
                .animated_properties()
                .into_iter()
                .find(|p| p.id == property)
            && let Some(v) = p.values.get_mut(component)
        {
            *v = value;
            self.set_property(property, &p.values)?;
        }
        Ok(())
    }

    pub fn evaluated_parameters(&self, time: Time) -> Result<OperationConfig, String> {
        let mut parameters = self.try_configuration()?;
        for mut property in parameters.animated_properties() {
            let mut animated = false;
            for (i, value) in property.values.iter_mut().enumerate() {
                if let Some(sample) = self.animation.sample(&path(&property.id, i), time)? {
                    *value = sample;
                    animated = true;
                }
            }
            if animated {
                parameters.set_property(&property.id, &property.values)?;
            }
        }
        parameters.validate()?;
        Ok(parameters)
    }
    pub fn validate_animation(&self) -> Result<(), String> {
        let available: BTreeMap<_, _> = self
            .animated_properties()
            .into_iter()
            .flat_map(|p| (0..p.values.len()).map(move |i| (path(&p.id, i), p.range)))
            .collect();
        self.animation.validate()?;
        for path in self.animation.bindings().keys() {
            if !available.contains_key(path) {
                return Err(format!("Unknown animation channel {path}"));
            }
        }
        Ok(())
    }
}
