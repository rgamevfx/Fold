//! Lower isolated groups and ordered mask stacks into the shared image plan.
use super::*;
use fold_render::{
    ImageOp, RenderGraph,
    operations::{Edges, MergeMode},
};
#[derive(Clone, Debug)]
pub struct Coverage {
    pub content: Content,
    pub operation: crate::scene::MaskOperation,
    pub invert: bool,
    pub opacity: f64,
    pub feather: f64,
}
#[derive(Clone, Debug)]
pub struct Effects {
    pub opacity: f64,
    pub masks: Vec<Coverage>,
}
fn effects(content: &Content) -> bool {
    content.iter().any(|e| {
        e.effects.is_some()
            || match &e.geometry {
                Geometry::Group(c) | Geometry::Instance(c) => effects(c),
                _ => false,
            }
    })
}
pub fn compile(
    content: &Content,
    width: u32,
    height: u32,
    scale: [f64; 2],
    cancel: &fold_media::Cancel,
) -> Result<RenderGraph, String> {
    struct Builder<'a> {
        nodes: Vec<ImageOp>,
        scale: [f64; 2],
        cancel: &'a fold_media::Cancel,
    }
    impl Builder<'_> {
        fn push(&mut self, op: ImageOp) -> Result<usize, String> {
            self.cancel.check()?;
            if self.nodes.len() >= 16384 {
                return Err("scene image-operation budget exceeded".into());
            }
            let id = self.nodes.len();
            self.nodes.push(op);
            Ok(id)
        }
        fn over(&mut self, back: Option<usize>, front: usize) -> Result<Option<usize>, String> {
            Ok(Some(if let Some(background) = back {
                self.push(ImageOp::Over {
                    foreground: front,
                    background,
                })?
            } else {
                front
            }))
        }
        fn plain(&mut self, content: Vec<Element>) -> Result<usize, String> {
            let drawings = drawings_with(&Arc::new(content), [1.; 2], self.cancel)?;
            self.push(ImageOp::Vector(Arc::new(drawings)))
        }
        fn content(
            &mut self,
            source: &Content,
            parent: [f64; 6],
            depth: usize,
        ) -> Result<usize, String> {
            if depth > 64 {
                return Err("render nesting limit".into());
            }
            let mut pending = vec![];
            let mut result = None;
            for e in source.iter() {
                self.cancel.check()?;
                let mut item = e.clone();
                item.transform = multiply(parent, e.transform);
                let nested = match &item.geometry {
                    Geometry::Group(c) | Geometry::Instance(c) => effects(c),
                    _ => false,
                };
                if item.effects.is_none() && !nested {
                    pending.push(item);
                    continue;
                }
                if !pending.is_empty() {
                    let id = self.plain(std::mem::take(&mut pending))?;
                    result = self.over(result, id)?;
                }
                let style = item.effects.take();
                let mut id = if nested {
                    let children = match &item.geometry {
                        Geometry::Group(c) | Geometry::Instance(c) => c,
                        _ => unreachable!(),
                    };
                    let children = Arc::new(
                        children
                            .iter()
                            .map(|c| {
                                let mut c = c.clone();
                                c.fill = item.fill.or(c.fill);
                                c.stroke = item.stroke.clone().or(c.stroke);
                                if let Some(effects) = &mut c.effects {
                                    effects.opacity *= item.opacity;
                                } else {
                                    c.opacity *= item.opacity;
                                }
                                c
                            })
                            .collect(),
                    );
                    self.content(&children, item.transform, depth + 1)?
                } else {
                    self.plain(vec![item])?
                };
                if let Some(style) = style {
                    if style.opacity != 1. {
                        id = self.push(ImageOp::Opacity {
                            input: id,
                            opacity: style.opacity as f32,
                        })?;
                    }
                    let mut coverage = None;
                    for mask in style.masks {
                        let mut mask_id = self.content(&mask.content, parent, depth + 1)?;
                        if mask.feather > 0. {
                            mask_id = self.push(ImageOp::Gaussian {
                                input: mask_id,
                                size: [
                                    (mask.feather * self.scale[0]) as f32,
                                    (mask.feather * self.scale[1]) as f32,
                                ],
                                edges: Edges::Transparent,
                            })?;
                        }
                        let white = self.push(ImageOp::Solid { rgba: [1.; 4] })?;
                        if mask.invert {
                            mask_id = self.push(ImageOp::Merge {
                                first: white,
                                second: mask_id,
                                mode: MergeMode::Out,
                            })?;
                        }
                        let intersect = mask.operation == crate::scene::MaskOperation::Intersect;
                        let neutral = if intersect {
                            white
                        } else {
                            self.push(ImageOp::Solid { rgba: [0.; 4] })?
                        };
                        mask_id = self.push(ImageOp::Mix {
                            original: neutral,
                            processed: mask_id,
                            mask: None,
                            mask_channel: 3,
                            invert: false,
                            amount: mask.opacity as f32,
                            channels: [true; 4],
                        })?;
                        let base = if let Some(base) = coverage {
                            base
                        } else if mask.operation == crate::scene::MaskOperation::Add {
                            neutral
                        } else {
                            white
                        };
                        coverage = Some(self.push(ImageOp::Merge {
                            first: base,
                            second: mask_id,
                            mode: match mask.operation {
                                crate::scene::MaskOperation::Intersect => MergeMode::In,
                                crate::scene::MaskOperation::Add => MergeMode::Max,
                                crate::scene::MaskOperation::Subtract => MergeMode::Out,
                            },
                        })?);
                    }
                    if let Some(mask) = coverage {
                        id = self.push(ImageOp::Mask { input: id, mask })?;
                    }
                }
                result = self.over(result, id)?;
            }
            if !pending.is_empty() {
                let id = self.plain(pending)?;
                result = self.over(result, id)?;
            }
            match result {
                Some(id) => Ok(id),
                None => self.push(ImageOp::Solid { rgba: [0.; 4] }),
            }
        }
    }
    let mut b = Builder {
        nodes: vec![],
        scale,
        cancel,
    };
    let output = b.content(content, [scale[0], 0., 0., scale[1], 0., 0.], 0)?;
    Ok(RenderGraph {
        width,
        height,
        nodes: b.nodes,
        output,
    })
}
