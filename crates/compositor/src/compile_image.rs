//! Image operation lowering and shared channel/mask behavior.
use crate::{Effect, OperationConfig as P};
use fold_render::{
    ImageId, ImageOp as Op, RenderGraph,
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
        P::Blur {
            size,
            edges,
            filter,
        } => Op::FilteredBlur {
            filter: *filter,
            input: inputs[0],
            size: [size[0] * scale[0] as f32, size[1] * scale[1] as f32],
            edges: *edges,
        },
        P::TransformImage { settings } => Op::Resample {
            edges: settings.edges,
            clamp: settings.clamp,
            input: inputs[0],
            transform: settings.matrix(scale)?,
            filter: settings.filter,
        },
        P::ColorGrade { settings } => Op::ColorGrade {
            input: inputs[0],
            settings: settings.clone(),
        },
        P::Composite { mode, .. } => Op::Merge {
            first: inputs[0],
            second: inputs[1],
            mode: *mode,
        },
        P::Crop { rect } => Op::Crop {
            input: inputs[0],
            rect: std::array::from_fn(|i| (f64::from(rect[i]) * scale[i % 2]).round() as u32),
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
    samples: &[P],
    effect: &Effect,
    inputs: &[&Channels],
    mask: Option<&Channels>,
    scale: [f64; 2],
) -> Result<Channels, String> {
    let parameters = &samples[0];
    let original_slot = usize::from(matches!(parameters, P::Composite { .. }));
    let mut result = inputs[original_slot].clone();
    if effect.mix == 0. {
        return Ok(result);
    }
    let mut groups = BTreeMap::<(String, bool), Vec<(channels::ChannelName, u8)>>::new();
    let routing = match parameters {
        P::Composite { channels, .. } => Some(channels),
        _ => None,
    };
    let mut selected_channels = effect.channels.clone();
    if routing.is_some_and(|channels| channels.other) {
        selected_channels.extend(
            inputs
                .iter()
                .flat_map(|input| input.iter())
                .map(|(name, _)| name.clone())
                .filter(|name| !name.as_str().starts_with("rgba.")),
        );
        selected_channels.sort();
        selected_channels.dedup();
    }
    for name in &selected_channels {
        let (layer, component) = name.as_str().rsplit_once('.').unwrap();
        let index = match component {
            "red" => Some(0),
            "green" => Some(1),
            "blue" => Some(2),
            "alpha" => Some(3),
            _ => None,
        };
        if matches!(parameters, P::Unary { .. }) && index == Some(3) {
            continue;
        }
        let index = if matches!(parameters, P::ColorGrade { .. }) && index == Some(3) {
            None
        } else {
            index
        };
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
        for (slot, input) in inputs.iter().enumerate() {
            let included = |name: &str| {
                routing.is_none_or(|channels| {
                    (channels.other && !name.starts_with("rgba."))
                        || (if slot == 0 { &channels.a } else { &channels.b })
                            .iter()
                            .any(|selected| selected.as_str() == name)
                })
            };
            let values = if scalar {
                let name = selected[0].0.as_str();
                let value = if included(name) {
                    input.get(name).map(|c| c.value).unwrap_or(Value::Zero)
                } else {
                    Value::Zero
                };
                [
                    value,
                    Value::Zero,
                    Value::Zero,
                    if routing.is_some() && (!included(name) || input.get(name).is_err()) {
                        Value::Zero
                    } else {
                        Value::One
                    },
                ]
            } else {
                ["red", "green", "blue", "alpha"].map(|component| {
                    let name = format!("{group}.{component}");
                    if !included(&name) {
                        return Value::Zero;
                    }
                    input.get(&name).map(|c| c.value).unwrap_or(
                        if component == "alpha"
                            && (routing.is_none()
                                || ["red", "green", "blue"]
                                    .iter()
                                    .any(|c| input.get(&format!("{group}.{c}")).is_ok()))
                        {
                            Value::One
                        } else {
                            Value::Zero
                        },
                    )
                })
            };
            packed.push(channels::pack(graph, values)?);
        }
        let mut processed = operation(graph, parameters, &packed, scale)?;
        for (index, sample) in samples.iter().enumerate().skip(1) {
            let next = operation(graph, sample, &packed, scale)?;
            let id = graph.nodes.len();
            graph.nodes.push(Op::Mix {
                original: processed,
                processed: next,
                mask: None,
                mask_channel: 0,
                invert: false,
                amount: 1. / (index + 1) as f32,
                channels: [true; 4],
            });
            processed = id;
        }
        if mask.is_some() || effect.mix != 1. {
            let original = if routing.is_some() {
                let input = inputs[original_slot];
                let values = if scalar {
                    [
                        input
                            .get(selected[0].0.as_str())
                            .map(|c| c.value)
                            .unwrap_or(Value::Zero),
                        Value::Zero,
                        Value::Zero,
                        Value::One,
                    ]
                } else {
                    ["red", "green", "blue", "alpha"].map(|component| {
                        input
                            .get(&format!("{group}.{component}"))
                            .map(|c| c.value)
                            .unwrap_or(Value::Zero)
                    })
                };
                channels::pack(graph, values)?
            } else {
                packed[original_slot]
            };
            let id = graph.nodes.len();
            graph.nodes.push(Op::Mix {
                original,
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
