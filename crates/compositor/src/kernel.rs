//! Transient, validated inputs to image-processing kernels. Never stored on authored nodes.
use crate::{OPERATORS, Operator, Source};
use fold_foundation::Time;
use fold_graph::types::Type;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum OperationConfig {
    Shader {
        settings: crate::shader::Shader,
    },
    RelativeTransform {
        settings: crate::RelativeTransform,
    },
    Input {
        id: String,
    },
    Instance {
        instance: fold_platform::effects::Instance,
        images: Vec<String>,
    },
    Dissolve {
        curve: fold_animation::timing::Curve,
    },
    Read {
        source: Source,
        start: Time,
        source_start: Time,
        duration: Time,
    },
    Solid {
        rgba: [f32; 4],
    },
    /// Translation and scale in document pixels, plus opacity.
    Transform {
        translate: [f64; 2],
        scale: [f64; 2],
        opacity: f32,
    },
    Crop {
        rect: [u32; 4],
    },
    Blur {
        radius: u32,
    },
    Grade {
        gain: [f32; 3],
    },
    Mask {
        rect: [u32; 4],
    },
    Unary {
        operation: fold_render::operations::Unary,
    },
    Shape {
        shape: crate::Shape,
        bounds: [f64; 4],
    },
    GaussianBlur {
        size: [f32; 2],
        edges: fold_render::operations::Edges,
    },
    TransformImage {
        settings: crate::Transform,
    },
    ColorGrade {
        settings: fold_render::operations::Grade,
    },
    Composite {
        mode: fold_render::operations::MergeMode,
    },
    ApplyMask,
    Shuffle {
        mappings: Vec<crate::ChannelMapping>,
    },
    RemoveChannels {
        channels: Vec<fold_render::channels::ChannelName>,
        keep: bool,
    },
    /// Input 0 is foreground, input 1 background.
    Merge,
    Output,
}
impl OperationConfig {
    pub fn input_count(&self) -> usize {
        match self {
            Self::Shader { settings } => settings.images.len(),
            Self::Instance { images, .. } => images.len(),
            _ => self.operator().inputs.len(),
        }
    }
    pub fn input_key(&self, slot: usize) -> &str {
        match self {
            Self::Shader { settings } => &settings.images[slot],
            Self::Instance { images, .. } => &images[slot],
            Self::Dissolve { .. } => {
                if slot == 0 {
                    "outgoing"
                } else {
                    "incoming"
                }
            }
            _ => self.operator().input_key(slot),
        }
    }
    pub fn input_type(&self, slot: usize) -> Type {
        match self {
            Self::Shader { .. } | Self::Instance { .. } => Type::Image,
            _ => self.operator().inputs[slot].clone(),
        }
    }

    pub fn operator(&self) -> &'static Operator {
        &OPERATORS[match self {
            Self::Shader { .. } => 27,
            Self::RelativeTransform { .. } => 26,
            Self::Input { .. } => 23,
            Self::Instance { .. } => 24,
            Self::Dissolve { .. } => 25,
            Self::Read { .. } => 0,
            Self::Solid { .. } => 1,
            Self::Transform { .. } => 2,
            Self::Crop { .. } => 3,
            Self::Blur { .. } => 4,
            Self::Grade { .. } => 5,
            Self::Mask { .. } => 6,
            Self::ApplyMask => 7,
            Self::Merge => 8,
            Self::Output => 9,
            Self::Shuffle { .. } => 10,
            Self::RemoveChannels { .. } => 11,
            Self::ColorGrade { .. } => 12,
            Self::Composite { .. } => 13,
            Self::TransformImage { .. } => 14,
            Self::GaussianBlur { .. } => 15,
            Self::Unary { operation } => {
                16 + match operation {
                    fold_render::operations::Unary::Exposure { .. } => 0,
                    fold_render::operations::Unary::Invert => 1,
                    fold_render::operations::Unary::Clamp { .. } => 2,
                    fold_render::operations::Unary::Premult => 3,
                    fold_render::operations::Unary::Unpremult => 4,
                }
            }
            Self::Shape { shape, .. } => {
                if *shape == crate::Shape::Rectangle {
                    21
                } else {
                    22
                }
            }
        }]
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Shader { settings } => settings.validate()?,
            Self::RelativeTransform { settings } => {
                settings.matrix(
                    fold_platform::format::Format::square([1920, 1080]),
                    [1920, 1080],
                )?;
            }
            Self::Input { id } => {
                if id.is_empty() || id.len() > 128 {
                    return Err("Input requires a stable port identity".into());
                }
            }
            Self::Instance { instance, images } => {
                instance.validate()?;
                let unique: BTreeSet<_> = images.iter().collect();
                if images.len() > 16
                    || unique.len() != images.len()
                    || images.iter().any(|i| i.is_empty() || i.len() > 128)
                {
                    return Err("Invalid effect image interface".into());
                }
            }
            Self::Dissolve { curve } => curve.validate()?,
            Self::Unary { operation } => operation.validate()?,
            Self::Shape { shape, bounds } => {
                crate::shapes::drawing(*shape, *bounds, [1.; 2])?;
            }
            Self::GaussianBlur { size, .. } => fold_render::operations::validate_blur(*size)?,
            Self::TransformImage { settings } => {
                settings.matrix([1.; 2])?;
            }
            Self::ColorGrade { settings } => settings.validate()?,
            Self::Shuffle { mappings } => {
                let unique: BTreeSet<_> = mappings.iter().map(|m| &m.destination).collect();
                if mappings.len() > 256 || unique.len() != mappings.len() {
                    return Err(
                        "Shuffle requires unique destinations and at most 256 mappings".into(),
                    );
                }
            }
            Self::RemoveChannels { channels, .. } if channels.len() > 256 => {
                return Err("Select at most 256 channels".into());
            }
            Self::Read {
                source,
                start,
                source_start,
                duration,
            } => {
                let info = source.info();
                info.validate()?;
                if let Source::Exr {
                    image,
                    color_layers,
                    sequence,
                    ..
                } = source
                {
                    if color_layers.len() > 256
                        || color_layers.iter().any(|layer| {
                            !["red", "green", "blue"].iter().all(|component| {
                                image
                                    .channels
                                    .iter()
                                    .any(|c| c.name == format!("{layer}.{component}"))
                            })
                        })
                    {
                        return Err("EXR color assignments require complete RGB layers".into());
                    }
                    image.validate()?;
                    if image.sequence.is_some() {
                        return Err(
                            "Read stores a sequence summary; manifests belong to imported assets"
                                .into(),
                        );
                    }
                    if let Some(sequence) = sequence {
                        sequence.validate()?;
                        if sequence.last >= info.frames {
                            return Err("EXR sequence exceeds source frame range".into());
                        }
                    }
                    if image.dimensions != [info.width, info.height] {
                        return Err("EXR source dimensions disagree".into());
                    }
                }
                if *source_start < Time::ZERO
                    || *duration <= Time::ZERO
                    || source_start
                        .checked_add(*duration)
                        .map_err(|e| e.to_string())?
                        > info.time(info.frames)?
                {
                    return Err("Read requires a valid half-open source range".into());
                }
                start.checked_add(*duration).map_err(|e| e.to_string())?;
                if let Source::Document { source, .. } = source
                    && source.output.is_empty()
                {
                    return Err("Read requires a named video output port".into());
                }
            }
            Self::Solid { rgba }
                if rgba.iter().any(|v| !v.is_finite()) || !(0.0..=1.0).contains(&rgba[3]) =>
            {
                return Err("Solid requires finite RGB and alpha in 0..1".into());
            }
            Self::Transform {
                translate,
                scale,
                opacity,
            } if translate
                .iter()
                .chain(scale)
                .any(|v| !v.is_finite() || v.abs() > 1e6)
                || scale.iter().any(|v| v.abs() < 1e-6)
                || !opacity.is_finite()
                || !(0.0..=1.0).contains(opacity) =>
            {
                return Err("Transform requires finite invertible scale and opacity 0..1".into());
            }
            Self::Crop { rect } | Self::Mask { rect } if rect[0] > rect[2] || rect[1] > rect[3] => {
                return Err("invalid rectangle".into());
            }
            Self::Blur { radius } if *radius > 64 => return Err("blur radius exceeds 64".into()),
            Self::Grade { gain }
                if gain
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=16.0).contains(v)) =>
            {
                return Err("gain must be finite in 0..16".into());
            }
            _ => {}
        }
        Ok(())
    }
}
