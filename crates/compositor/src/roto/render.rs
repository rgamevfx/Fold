use super::{Combine, Layer, Roto, geometry::compose};
use fold_foundation::Time;
use fold_media::Cancel;
use fold_render::{
    Affine, ImageId, ImageOp as Op, RenderGraph,
    channels::{Channel, Channels, Interpretation, RGBA, Value},
    operations::MergeMode,
};
use std::sync::Arc;
fn push(graph: &mut RenderGraph, op: Op) -> ImageId {
    let id = graph.nodes.len();
    graph.nodes.push(op);
    id
}
fn layer(
    graph: &mut RenderGraph,
    l: &Layer,
    parent: Affine,
    time: Time,
    cancel: &Cancel,
) -> Result<ImageId, String> {
    cancel.check()?;
    if graph.nodes.len() > 4000 {
        return Err("Roto render graph budget exceeded".into());
    }
    if !l.enabled || l.lifetime.is_some_and(|[a, b]| time < a || time >= b) {
        return Ok(push(graph, Op::Solid { rgba: [0.; 4] }));
    }
    let m = compose(parent, l.matrix());
    let mut result = if let Some(s) = &l.shape {
        if s.points.len() < 3 || !s.closed {
            push(graph, Op::Solid { rgba: [0.; 4] })
        } else {
            let drawings = fold_render::vector_feather::drawings(
                s.path(false),
                s.path(true),
                [m.a, m.b, m.c, m.d, m.tx, m.ty],
                s.falloff,
                cancel,
            )?;
            push(graph, Op::Vector(Arc::new(drawings)))
        }
    } else {
        let mut result = push(graph, Op::Solid { rgba: [0.; 4] });
        for child in &l.children {
            if !child.enabled || child.lifetime.is_some_and(|[a, b]| time < a || time >= b) {
                continue;
            }
            let shape = layer(graph, child, m, time, cancel)?;
            let (a, b, mode) = match child.combine {
                Combine::Over => (shape, result, MergeMode::Over),
                Combine::Add => (shape, result, MergeMode::Plus),
                Combine::Subtract => (result, shape, MergeMode::Out),
                Combine::Intersect => (result, shape, MergeMode::In),
                Combine::Replace => (shape, result, MergeMode::Copy),
            };
            result = push(
                graph,
                Op::Merge {
                    first: a,
                    second: b,
                    mode,
                },
            );
        }
        result
    };
    if l.invert {
        let white = push(graph, Op::Solid { rgba: [1.; 4] });
        result = push(
            graph,
            Op::Merge {
                first: white,
                second: result,
                mode: MergeMode::Out,
            },
        );
    }
    if l.opacity != 1. {
        result = push(
            graph,
            Op::Opacity {
                input: result,
                opacity: l.opacity as f32,
            },
        );
    }
    Ok(result)
}
pub(crate) fn compile(
    graph: &mut RenderGraph,
    samples: &[(Time, Roto)],
    input: Option<&Channels>,
    scale: [f64; 2],
    cancel: &Cancel,
) -> Result<Channels, String> {
    let settings = &samples.first().ok_or("Roto requires a time sample")?.1;
    let m = Affine {
        a: scale[0],
        d: scale[1],
        ..Affine::IDENTITY
    };
    let mut matte = None;
    for (i, (time, roto)) in samples.iter().enumerate() {
        let image = layer(graph, &roto.root, m, *time, cancel)?;
        matte = Some(if let Some(original) = matte {
            push(
                graph,
                Op::Mix {
                    original,
                    processed: image,
                    mask: None,
                    mask_channel: 3,
                    invert: false,
                    amount: 1. / (i + 1) as f32,
                    channels: [true; 4],
                },
            )
        } else {
            image
        });
    }
    let matte = matte.unwrap();
    // Read scalar coverage from alpha, never convert it through working color.
    let mut channels = if let Some(input) = input {
        input.clone()
    } else {
        let zero = push(graph, Op::Solid { rgba: [0.; 4] });
        Channels::rgba(zero)
    };
    for name in &settings.channels {
        channels.insert(
            name.clone(),
            Channel {
                value: Value::Component {
                    image: matte,
                    component: 3,
                },
                interpretation: if name.as_str().ends_with(".alpha") {
                    Interpretation::Alpha
                } else {
                    Interpretation::Data
                },
            },
        );
    }
    if settings.premultiply {
        let image = channels.pack(graph, RGBA)?;
        let result = push(
            graph,
            Op::Unary {
                input: image,
                operation: fold_render::operations::Unary::Premult,
            },
        );
        for (name, channel) in Channels::rgba(result).iter() {
            channels.insert(name.clone(), *channel);
        }
    }
    Ok(channels)
}
