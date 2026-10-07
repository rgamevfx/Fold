//! GPU image-driven responses. Native paths/glyphs retain editable authoring;
//! repeated local shapes share a raster atlas and compact instance transforms.
use super::*;
pub(super) fn definition() -> Definition {
    let mut def = super::definition(
        "fold.motion.image_response",
        "Image Response",
        "Image",
        vec![
            Socket::required("content", Kind::Content),
            Socket::required("sample", Kind::ImageField),
            vector("displacement", [0.; 2], "units"),
            scalar("scale", 0., "factor"),
            scalar("rotation", 0., "degrees"),
            scalar("color", 0., "factor"),
            scalar("opacity", 0., "factor"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let c = e.content(n, "content")?;
            let Value::ImageField(sample) = e.input(n, "sample")? else {
                return Err("connect Sample Image".into());
            };
            let mut response = Response {
                sample: &sample,
                displacement: e.vector(n, "displacement")?.map(|v| v as f32),
                scale: e.scalar(n, "scale")? as f32,
                rotation: e.scalar(n, "rotation")? as f32,
                color: e.scalar(n, "color")? as f32,
                alpha: e.scalar(n, "opacity")? as f32,
                dimensions: [e.canvas_size()?[0], e.canvas_size()?[1]],
                cache: Default::default(),
            };
            Ok(content(response.apply(&c, &mut e.budget, 0)?))
        },
    );
    def.description = "GPU image-driven displacement, scale, color and opacity. Images/grids sample stored UVs; paths/glyphs sample at each instance center in document coordinates and share raster atlases. Geometry remains editable upstream. Does not expose pixels to CPU fields. Combine amounts in one response.";
    def
}
struct Response<'a> {
    sample: &'a Samples,
    displacement: [f32; 2],
    scale: f32,
    rotation: f32,
    color: f32,
    alpha: f32,
    dimensions: [f64; 2],
    cache: std::collections::HashMap<usize, crate::geometry::Content>,
}
impl Response<'_> {
    fn apply(
        &mut self,
        c: &crate::geometry::Content,
        budget: &mut crate::fields::Budget,
        depth: usize,
    ) -> Result<crate::geometry::Content, String> {
        if depth > 64 {
            return Err("image response nesting exceeds 64".into());
        }
        let key = Arc::as_ptr(c) as usize;
        if let Some(result) = self.cache.get(&key) {
            return Ok(result.clone());
        }
        let mut out = Vec::new();
        for item in c.iter() {
            budget.spend()?;
            let mut item = item.clone();
            match &item.geometry {
                Geometry::Group(children) => {
                    item.geometry = Geometry::Group(self.apply(children, budget, depth + 1)?);
                    out.push(item);
                    continue;
                }
                Geometry::Instance(children) => {
                    item.geometry = Geometry::Instance(self.apply(children, budget, depth + 1)?);
                    out.push(item);
                    continue;
                }
                Geometry::Path(_) | Geometry::Glyph { .. } => {
                    let mut local = item.clone();
                    local.transform = crate::geometry::IDENTITY;
                    local.opacity = 1.;
                    local.effects = None;
                    let Some(mut bounds) = crate::geometry::bounds::content_with_budget(
                        &Arc::new(vec![local.clone()]),
                        budget,
                    )?
                    else {
                        continue;
                    };
                    let padding = local.stroke.as_ref().map_or(1., |s| s.width * 4. + 1.);
                    bounds[0] -= padding;
                    bounds[1] -= padding;
                    bounds[2] += padding;
                    bounds[3] += padding;
                    let size = [bounds[2] - bounds[0], bounds[3] - bounds[1]];
                    if size[0] <= 0. || size[1] <= 0. {
                        continue;
                    }
                    let scale = [self.dimensions[0] / size[0], self.dimensions[1] / size[1]];
                    local.transform = [
                        scale[0],
                        0.,
                        0.,
                        scale[1],
                        -bounds[0] * scale[0],
                        -bounds[1] * scale[1],
                    ];
                    let center = [(bounds[0] + bounds[2]) * 0.5, (bounds[1] + bounds[3]) * 0.5];
                    let mut mesh = quad(size);
                    for v in &mut Arc::make_mut(&mut mesh).vertices {
                        for i in 0..2 {
                            v.position[i] += center[i] as f32;
                            v.center[i] = center[i] as f32;
                        }
                    }
                    let mut draw = Draw::new(mesh);
                    draw.document_samples = true;
                    item.geometry = Geometry::Image(Arc::new(ImageGeometry {
                        texture: Texture::Atlas(Arc::new(vec![local])),
                        driver: None,
                        draw,
                    }));
                }
                Geometry::Image(_) => {}
                Geometry::Point => {
                    return Err("instance content on points before applying Image Response".into());
                }
            }
            let Geometry::Image(image) = &item.geometry else {
                unreachable!()
            };
            let mut image = image.as_ref().clone();
            if image.driver.is_some() {
                return Err("combine image response amounts in one Image Response".into());
            }
            image.driver = Some(self.sample.image.clone());
            image.draw.channel = self.sample.channel;
            image.draw.remap = self.sample.remap;
            image.draw.displacement = self.displacement;
            image.draw.scale = self.scale;
            image.draw.rotation = self.rotation;
            image.draw.color = self.color;
            image.draw.alpha = self.alpha;
            image.draw.validate()?;
            item.geometry = Geometry::Image(Arc::new(image));
            out.push(item);
        }
        let result = Arc::new(out);
        self.cache.insert(key, result.clone());
        Ok(result)
    }
}
