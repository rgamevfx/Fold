//! Image operation lowering and shared channel/mask behavior.
use crate::{Effect, Parameters as P};
use fold_render::{
    Affine, ImageId, ImageOp as Op, RenderGraph,
    channels::{self, Channel, Channels, Value},
};
use std::collections::BTreeMap;

fn operation(
    graph: &mut RenderGraph,
    p: &P,
    inputs: &[ImageId],
    scale: [f64; 2],
) -> Result<ImageId, String> {
    let op = match p {
        P::Unary { operation } => Op::Unary {
            input: inputs[0],
            operation: operation.clone(),
        },
        P::GaussianBlur { size, edges } => Op::Gaussian {
            input: inputs[0],
            size: [size[0] * scale[0] as f32, size[1] * scale[1] as f32],
            edges: *edges,
        },
        P::TransformImage { settings } => Op::Resample {
            input: inputs[0],
            transform: settings.matrix(scale)?,
            filter: settings.filter,
        },
        P::ColorGrade { settings } => Op::ColorGrade {
            input: inputs[0],
            settings: settings.clone(),
        },
        P::Composite { mode } => Op::Merge {
            first: inputs[0],
            second: inputs[1],
            mode: *mode,
        },
        P::Transform {
            translate,
            scale: size,
            opacity,
        } => {
            let input = graph.nodes.len();
            graph.nodes.push(Op::Transform {
                input: inputs[0],
                transform: Affine {
                    a: size[0],
                    d: size[1],
                    tx: translate[0] * scale[0],
                    ty: translate[1] * scale[1],
                    ..Affine::IDENTITY
                },
            });
            Op::Opacity {
                input,
                opacity: *opacity,
            }
        }
        P::Crop { rect } => Op::Crop {
            input: inputs[0],
            rect: std::array::from_fn(|i| (f64::from(rect[i]) * scale[i % 2]).round() as u32),
        },
        P::Blur { radius } => {
            let radius = (f64::from(*radius) * scale[0].max(scale[1])).round() as u32;
            if radius > 64 {
                return Err("Scaled legacy blur radius exceeds 64 pixels".into());
            }
            Op::Blur {
                input: inputs[0],
                radius,
            }
        }
        P::Grade { gain } => Op::Grade {
            input: inputs[0],
            gain: *gain,
        },
        P::Merge => Op::Over {
            foreground: inputs[0],
            background: inputs[1],
        },
        _ => return Err("Unsupported image effect".into()),
    };
    let result = graph.nodes.len();
    graph.nodes.push(op);
    Ok(result)
}

/// Channel references outside the selection are preserved without a copy. RGB
/// layers are processed together (with their alpha); arbitrary scalar channels
/// use the red component of the operation's controls and carry no color transform.
pub(super) fn effect(
    graph: &mut RenderGraph,
    parameters: &P,
    effect: &Effect,
    inputs: &[&Channels],
    mask: Option<&Channels>,
    scale: [f64; 2],
) -> Result<Channels, String> {
    let original_slot = usize::from(matches!(parameters, P::Merge | P::Composite { .. }));
    let mut result = inputs[original_slot].clone();
    if effect.mix == 0. {
        return Ok(result);
    }
    let mut groups = BTreeMap::<(String, bool), Vec<(channels::ChannelName, u8)>>::new();
    for name in &effect.channels {
        let (layer, component) = name.as_str().rsplit_once('.').unwrap();
        let index = match component {
            "red" => Some(0),
            "green" => Some(1),
            "blue" => Some(2),
            "alpha" => Some(3),
            _ => None,
        };
        if matches!(
            parameters,
            P::Unary { .. } | P::Grade { .. } | P::ColorGrade { .. }
        ) && index == Some(3)
        {
            continue;
        }
        let key = if index.is_some() {
            layer.to_owned()
        } else {
            name.to_string()
        };
        groups
            .entry((key, index.is_none()))
            .or_default()
            .push((name.clone(), index.unwrap_or(0)));
    }
    let mask = mask
        .map(|mask| {
            let value = mask.get(effect.mask_channel.as_str())?.value;
            channels::pack(graph, [value, Value::Zero, Value::Zero, Value::One])
        })
        .transpose()?;
    for ((group, scalar), selected) in groups {
        if graph.nodes.len() > 4096 {
            return Err("Composite exceeds 4096 render operations".into());
        }
        let mut packed = Vec::new();
        for input in inputs {
            let values = if scalar {
                let value = input
                    .get(selected[0].0.as_str())
                    .map(|c| c.value)
                    .unwrap_or(Value::Zero);
                [value, Value::Zero, Value::Zero, Value::One]
            } else {
                ["red", "green", "blue", "alpha"].map(|component| {
                    input
                        .get(&format!("{group}.{component}"))
                        .map(|c| c.value)
                        .unwrap_or(if component == "alpha" {
                            Value::One
                        } else {
                            Value::Zero
                        })
                })
            };
            packed.push(channels::pack(graph, values)?);
        }
        let mut processed = operation(graph, parameters, &packed, scale)?;
        if mask.is_some() || effect.mix != 1. {
            let id = graph.nodes.len();
            graph.nodes.push(Op::Mix {
                original: packed[original_slot],
                processed,
                mask,
                mask_channel: 0,
                invert: effect.invert,
                amount: effect.mix,
                channels: [true; 4],
            });
            processed = id;
        }
        for (name, component) in selected {
            let previous = inputs[original_slot]
                .get(name.as_str())
                .or_else(|_| inputs[0].get(name.as_str()))?;
            result.insert(
                name,
                Channel {
                    value: Value::Component {
                        image: processed,
                        component,
                    },
                    interpretation: previous.interpretation,
                },
            );
        }
    }
    Ok(result)
}
