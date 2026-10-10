//! Lower evaluated shapes to a feature-neutral scalar coverage program.
use super::{Combine, Layer, Roto, Shape, geometry::compose};
use fold_foundation::Time;
use fold_media::Cancel;
use fold_render::{
    Affine, ImageId, ImageOp, RenderGraph,
    channels::{Channel, Channels, Interpretation, RGBA, Value},
    coverage::{Op, Program},
    vector::Drawing,
};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};
thread_local! {
    // Worker-local geometry only. No project, GPU resource or frame is retained.
    static PATHS: RefCell<VecDeque<(Shape, Drawing)>> = const { RefCell::new(VecDeque::new()) };
}
fn drawing(
    shape: &Shape,
    matrix: Affine,
    opacity: f64,
    cancel: &Cancel,
) -> Result<Drawing, String> {
    cancel.check()?;
    let mut drawing = PATHS.with(|cache| {
        let mut cache = cache.borrow_mut();
        if let Some(i) = cache.iter().position(|(s, _)| s == shape) {
            let entry = cache.remove(i).unwrap();
            let result = entry.1.clone();
            cache.push_back(entry);
            return Ok(result);
        }
        let result = fold_render::vector_feather::drawings(
            shape.path(false),
            shape.path(true),
            [1., 0., 0., 1., 0., 0.],
            shape.falloff,
            cancel,
        )?
        .remove(0);
        // Bound retained control/segment counts as well as entry count.
        while !cache.is_empty()
            && (cache.len() >= 64
                || cache.iter().map(|(s, _)| s.points.len()).sum::<usize>() + shape.points.len()
                    > 16384)
        {
            cache.pop_front();
        }
        if shape.points.len() <= 16384 {
            cache.push_back((shape.clone(), result.clone()));
        }
        Ok::<_, String>(result)
    })?;
    drawing.transform = [matrix.a, matrix.b, matrix.c, matrix.d, matrix.tx, matrix.ty];
    drawing.fill = Some([1., 1., 1., opacity]);
    Ok(drawing)
}
struct Builder<'a> {
    program: Program,
    cancel: &'a Cancel,
}
impl Builder<'_> {
    fn push(&mut self, op: Op) -> usize {
        let id = self.program.nodes.len();
        self.program.nodes.push(op);
        id
    }
    fn combine(&mut self, first: usize, second: usize, mode: Combine) -> usize {
        if mode == Combine::Replace {
            return first;
        }
        if matches!(self.program.nodes[second], Op::Constant(0.)) {
            return match mode {
                Combine::Over | Combine::Add => first,
                _ => second,
            };
        }
        let mode = match mode {
            Combine::Over => fold_render::coverage::Combine::Over,
            Combine::Add => fold_render::coverage::Combine::Add,
            Combine::Subtract => fold_render::coverage::Combine::Subtract,
            Combine::Intersect => fold_render::coverage::Combine::Intersect,
            Combine::Replace => unreachable!(),
        };
        self.push(Op::Combine {
            first,
            second,
            mode,
        })
    }
    fn flush(&mut self, batch: &mut Vec<Drawing>, result: usize) -> usize {
        if batch.is_empty() {
            return result;
        }
        let first = self.push(Op::Draw(Arc::new(std::mem::take(batch))));
        self.combine(first, result, Combine::Over)
    }
    fn layer(&mut self, l: &Layer, parent: Affine, time: Time) -> Result<usize, String> {
        self.cancel.check()?;
        if self.program.nodes.len() > 4000 {
            return Err("Roto coverage budget exceeded".into());
        }
        if !l.enabled || l.lifetime.is_some_and(|[a, b]| time < a || time >= b) {
            return Ok(self.push(Op::Constant(0.)));
        }
        let matrix = compose(parent, l.matrix());
        let mut result = if let Some(shape) = &l.shape {
            if shape.points.len() < 3 || !shape.closed {
                self.push(Op::Constant(0.))
            } else {
                let d = drawing(shape, matrix, 1., self.cancel)?;
                self.push(Op::Draw(Arc::new(vec![d])))
            }
        } else {
            let mut result = self.push(Op::Constant(0.));
            let mut batch = Vec::new();
            for child in &l.children {
                if !child.enabled || child.lifetime.is_some_and(|[a, b]| time < a || time >= b) {
                    continue;
                }
                if child.combine == Combine::Over && !child.invert {
                    if let Some(shape) = &child.shape {
                        if shape.closed && shape.points.len() >= 3 {
                            batch.push(drawing(
                                shape,
                                compose(matrix, child.matrix()),
                                child.opacity,
                                self.cancel,
                            )?);
                        }
                        continue;
                    }
                }
                result = self.flush(&mut batch, result);
                let first = self.layer(child, matrix, time)?;
                result = self.combine(first, result, child.combine);
            }
            self.flush(&mut batch, result)
        };
        if l.invert || l.opacity != 1. {
            result = self.push(Op::Adjust {
                input: result,
                invert: l.invert,
                opacity: l.opacity as f32,
            });
        }
        Ok(result)
    }
}
fn push(graph: &mut RenderGraph, op: ImageOp) -> ImageId {
    let id = graph.nodes.len();
    graph.nodes.push(op);
    id
}
pub(crate) fn compile(
    graph: &mut RenderGraph,
    samples: &[(Time, Roto)],
    input: Option<&Channels>,
    scale: [f64; 2],
    cancel: &Cancel,
) -> Result<Channels, String> {
    let settings = &samples.first().ok_or("Roto requires a time sample")?.1;
    let matrix = Affine {
        a: scale[0],
        d: scale[1],
        ..Affine::IDENTITY
    };
    let mut builder = Builder {
        program: Program {
            nodes: vec![],
            output: 0,
        },
        cancel,
    };
    let mut matte = None;
    for (i, (time, roto)) in samples.iter().enumerate() {
        let next = builder.layer(&roto.root, matrix, *time)?;
        matte = Some(if let Some(first) = matte {
            builder.push(Op::Mix {
                first,
                second: next,
                amount: 1. / (i + 1) as f32,
            })
        } else {
            next
        });
    }
    builder.program.output = matte.unwrap();
    let matte = push(graph, ImageOp::Coverage(Arc::new(builder.program)));
    let mut channels = if let Some(input) = input {
        input.clone()
    } else {
        Channels::rgba(push(graph, ImageOp::Solid { rgba: [0.; 4] }))
    };
    for name in &settings.channels {
        channels.insert(
            name.clone(),
            Channel {
                value: Value::Component {
                    image: matte,
                    component: 0,
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
            ImageOp::Unary {
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
