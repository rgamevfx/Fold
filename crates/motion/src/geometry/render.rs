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
            || matches!(e.geometry, Geometry::Image(_))
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
    compile_images(content, width, height, scale, cancel, &|_| {
        Err("image inputs require a project render context".into())
    })
}
pub fn compile_images(
    content: &Content,
    width: u32,
    height: u32,
    scale: [f64; 2],
    cancel: &fold_media::Cancel,
    resolve: &dyn Fn(&fold_platform::image_source::Request) -> Result<RenderGraph, String>,
) -> Result<RenderGraph, String> {
    compile_images_mapped(
        content,
        width,
        height,
        [scale[0], 0., 0., scale[1], 0., 0.],
        cancel,
        resolve,
    )
}
pub fn compile_images_mapped(
    content: &Content,
    width: u32,
    height: u32,
    matrix: [f64; 6],
    cancel: &fold_media::Cancel,
    resolve: &dyn Fn(&fold_platform::image_source::Request) -> Result<RenderGraph, String>,
) -> Result<RenderGraph, String> {
    let scale = [matrix[0].hypot(matrix[1]), matrix[2].hypot(matrix[3])];
    struct Builder<'a> {
        nodes: Vec<ImageOp>,
        dimensions: [u32; 2],
        sources: Vec<(fold_platform::image_source::Request, usize)>,
        atlases: Vec<(Content, usize)>,
        resolve: &'a dyn Fn(&fold_platform::image_source::Request) -> Result<RenderGraph, String>,
        scale: [f64; 2],
        cancel: &'a fold_media::Cancel,
    }
    impl Builder<'_> {
        fn source(
            &mut self,
            request: &fold_platform::image_source::Request,
        ) -> Result<usize, String> {
            if let Some((_, id)) = self.sources.iter().find(|(r, _)| r == request) {
                return Ok(*id);
            }
            let fragment = (self.resolve)(request)?;
            let mut graph = RenderGraph {
                width: self.dimensions[0],
                height: self.dimensions[1],
                nodes: std::mem::take(&mut self.nodes),
                output: 0,
            };
            let id = graph.append(fragment)?;
            self.nodes = graph.nodes;
            self.sources.push((request.clone(), id));
            Ok(id)
        }
        fn image(
            &mut self,
            image: &crate::nodes::image::ImageGeometry,
            transform: [f64; 6],
            opacity: f64,
        ) -> Result<usize, String> {
            use crate::nodes::image::Texture;
            let input = match &image.texture {
                Texture::Source(request) => self.source(request)?,
                Texture::Atlas(content) => {
                    if let Some((_, id)) =
                        self.atlases.iter().find(|(c, _)| Arc::ptr_eq(c, content))
                    {
                        *id
                    } else {
                        let drawings = drawings_with(content, self.scale, self.cancel)?;
                        let id = self.push(ImageOp::Vector(Arc::new(drawings)))?;
                        self.atlases.push((content.clone(), id));
                        id
                    }
                }
            };
            let driver = if let Some(request) = &image.driver {
                self.source(request)?
            } else {
                input
            };
            let mut draw = image.draw.clone();
            let m = transform.map(|v| v as f32);
            draw.instances = vec![fold_render::textured::Instance {
                matrix: [m[0], m[1], m[2], m[3]],
                offset: [m[4], m[5], opacity as f32, 0.],
            }];
            draw.validate()?;
            if let ImageOp::Baked(texture) = &self.nodes[input] {
                return self.push(ImageOp::BakedTextured {
                    batch: Arc::new(fold_render::baked::Batch {
                        textures: vec![texture.clone()],
                        draw,
                    }),
                    driver: image.driver.as_ref().map(|_| driver),
                });
            }
            self.push(ImageOp::Textured {
                input,
                driver,
                draw: Arc::new(draw),
            })
        }
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
            if let Some(background) = back {
                if let (
                    ImageOp::BakedTextured {
                        batch: a,
                        driver: ad,
                    },
                    ImageOp::BakedTextured {
                        batch: b,
                        driver: bd,
                    },
                ) = (&self.nodes[background], &self.nodes[front])
                {
                    let x = &a.textures[0];
                    let y = &b.textures[0];
                    if ad == bd
                        && a.draw.compatible(&b.draw)
                        && a.draw.instances.len() + b.draw.instances.len() <= 16384
                        && a.textures.len() + b.textures.len() <= 64
                        && x.dimensions() == y.dimensions()
                        && x.format == y.format
                        && x.aces == y.aces
                        && x.config == y.config
                    {
                        let other = b.clone();
                        if let ImageOp::BakedTextured { batch, .. } = &mut self.nodes[background] {
                            let batch = Arc::make_mut(batch);
                            let mut mapping = vec![];
                            for texture in &other.textures {
                                let index = batch
                                    .textures
                                    .iter()
                                    .position(|t| t.identity == texture.identity)
                                    .unwrap_or_else(|| {
                                        batch.textures.push(texture.clone());
                                        batch.textures.len() - 1
                                    });
                                mapping.push(index);
                            }
                            for instance in &other.draw.instances {
                                let mut instance = *instance;
                                instance.offset[3] = mapping[instance.offset[3] as usize] as f32;
                                batch.draw.instances.push(instance);
                            }
                        }
                        if front + 1 == self.nodes.len() {
                            self.nodes.pop();
                        }
                        return Ok(Some(background));
                    }
                }
            }
            if let Some(background) = back {
                if let (
                    ImageOp::Textured {
                        input: a,
                        driver: b,
                        draw: x,
                    },
                    ImageOp::Textured {
                        input: c,
                        driver: d,
                        draw: y,
                    },
                ) = (&self.nodes[background], &self.nodes[front])
                {
                    if a == c
                        && b == d
                        && x.compatible(y)
                        && x.instances.len() + y.instances.len() <= 16384
                    {
                        let instances = y.instances.clone();
                        if let ImageOp::Textured { draw, .. } = &mut self.nodes[background] {
                            Arc::make_mut(draw).instances.extend(instances);
                        }
                        if front + 1 == self.nodes.len() {
                            self.nodes.pop();
                        }
                        return Ok(Some(background));
                    }
                }
            }
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
                if item.effects.is_none() && !nested && !matches!(item.geometry, Geometry::Image(_))
                {
                    pending.push(item);
                    continue;
                }
                if !pending.is_empty() {
                    let id = self.plain(std::mem::take(&mut pending))?;
                    result = self.over(result, id)?;
                }
                let style = item.effects.take();
                let mut id = if let Geometry::Image(image) = &item.geometry {
                    self.image(image, item.transform, item.opacity)?
                } else if nested {
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
        dimensions: [width, height],
        sources: vec![],
        atlases: vec![],
        resolve,
        scale,
        cancel,
    };
    let output = b.content(content, matrix, 0)?;
    Ok(RenderGraph {
        width,
        height,
        nodes: b.nodes,
        output,
    })
}
