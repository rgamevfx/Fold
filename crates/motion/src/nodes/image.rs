//! Image values carry immutable requests, never decoded pixels or GPU handles.
use super::*;
use crate::{
    evaluation::{Value, content, output},
    fields::Sample,
    geometry::{Element, Geometry, derived_id},
};
use fold_platform::image_source::{Outside, Request, Source};
use fold_render::textured::{Channel, Draw, Mesh, Vertex};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
fn zero() -> fold_foundation::Time {
    fold_foundation::Time::ZERO
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Entry {
    pub source: Source,
    pub info: fold_media::VideoInfo,
    #[serde(default = "zero")]
    pub offset: fold_foundation::Time,
    #[serde(flatten)]
    pub extensions: std::collections::BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub sources: Vec<Entry>,
    #[serde(default)]
    pub outside: Outside,
    #[serde(default)]
    pub color_space: Option<String>,
}
#[derive(Clone, Debug)]
pub struct Samples {
    pub image: Request,
    pub channel: Channel,
    pub remap: [f32; 2],
}
#[derive(Clone, Debug)]
pub enum Texture {
    Source(Request),
    Atlas(crate::geometry::Content),
}
#[derive(Clone, Debug)]
pub struct ImageGeometry {
    pub texture: Texture,
    pub driver: Option<Request>,
    pub draw: Draw,
}
fn images(e: &mut Evaluator<'_>, n: &Node) -> Result<Arc<Vec<Request>>, String> {
    if let Value::Image(v) = e.input(n, "image")? {
        Ok(v)
    } else {
        Err("connect an image source".into())
    }
}
fn choose(images: &[Request], index: f64) -> Result<Request, String> {
    if images.is_empty() || !index.is_finite() || index.abs() > 1e9 {
        return Err("image selection requires sources and a finite index".into());
    }
    Ok(images[(index.floor() as i64).rem_euclid(images.len() as i64) as usize].clone())
}
pub fn quad(size: [f64; 2]) -> Arc<Mesh> {
    Arc::new(Mesh {
        vertices: [[0., 0.], [1., 0.], [1., 1.], [0., 1.]]
            .map(|uv| Vertex {
                position: [
                    ((uv[0] - 0.5) * size[0]) as f32,
                    ((uv[1] - 0.5) * size[1]) as f32,
                ],
                uv: uv.map(|v| v as f32),
                sample: [0.5; 2],
                center: [0.; 2],
            })
            .to_vec(),
        indices: vec![0, 1, 2, 0, 2, 3],
    })
}
#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize)]
enum Fit {
    #[default]
    Contain,
    Cover,
    Stretch,
}
fn card_mesh(size: [f64; 2], source: &Request, node: &Node) -> Result<Arc<Mesh>, String> {
    if size.iter().any(|v| !v.is_finite() || *v <= 0.) {
        return Err("card size must be positive".into());
    }
    let fit: Fit = serde_json::from_value(
        node.settings
            .get("fit")
            .cloned()
            .unwrap_or(serde_json::json!("Contain")),
    )
    .map_err(|e| e.to_string())?;
    let aspect = source.info.width as f64 / source.info.height as f64;
    let target = size[0] / size[1];
    let mut actual = size;
    let mut uv = [1.; 2];
    match fit {
        Fit::Contain => {
            if aspect > target {
                actual[1] = size[0] / aspect;
            } else {
                actual[0] = size[1] * aspect;
            }
        }
        Fit::Cover => {
            if aspect > target {
                uv[0] = target / aspect;
            } else {
                uv[1] = aspect / target;
            }
        }
        Fit::Stretch => {}
    }
    let mut mesh = quad(actual);
    for v in &mut Arc::make_mut(&mut mesh).vertices {
        for i in 0..2 {
            v.uv[i] = 0.5 + (v.uv[i] - 0.5) * uv[i] as f32;
        }
    }
    Ok(mesh)
}

pub fn definitions() -> Vec<Definition> {
    let mut source = definition(
        "fold.motion.image",
        "Media Texture",
        "Image",
        vec![scalar("time_offset", 0., "seconds")],
        vec![("image", Kind::Image)],
        |e, n| {
            let settings = n.settings::<Settings>()?;
            let offset = e.scalar(n, "time_offset")?;
            if !offset.is_finite() || offset.abs() > 1e8 {
                return Err("invalid image time offset".into());
            }
            let offset =
                fold_foundation::Time::new((offset * 1_000_000.).round() as i64, 1_000_000)
                    .map_err(|e| e.to_string())?;
            let mut requests = Vec::new();
            for entry in settings.sources {
                requests.push(Request {
                    bake: None,
                    source: entry.source,
                    info: entry.info,
                    time: e
                        .time
                        .checked_add(entry.offset)
                        .and_then(|t| t.checked_add(offset))
                        .map_err(|e| e.to_string())?,
                    outside: settings.outside,
                    color_space: settings.color_space.clone(),
                });
            }
            Ok(output("image", Value::Image(Arc::new(requests))))
        },
    );
    source.atomic_settings = &["sources"];
    source.defaults = || serde_json::to_value(Settings::default()).unwrap();
    source.validate = |n| {
        let s = n.settings::<Settings>()?;
        if s.sources.len() > 64 {
            return Err("image collection exceeds 64 sources".into());
        }
        for v in s.sources {
            v.info.validate()?;
        }
        Ok(())
    };
    source.description = "Image/video or document output collection. Each settings.sources entry contains source (Asset ID or Output DocumentRef), info (width/height/rate/frames), and an optional exact rational offset. Source frames are resolved on render workers and shared by exact source/time. Index wraps. Offset is added to document time; Hold, Loop, Black and Error define out-of-range behavior.";
    let mut card = definition(
        "fold.motion.image_card",
        "Image Card",
        "Image",
        vec![
            Socket::required("image", Kind::Image),
            scalar("index", 0., "integer"),
            vector("size", [320., 180.], "px"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let sources = images(e, n)?;
            let index = e.scalar(n, "index")?;
            let size = e.vector(n, "size")?;
            let request = choose(&sources, index)?;
            let mesh = card_mesh(size, &request, n)?;
            let image = ImageGeometry {
                texture: Texture::Source(request),
                driver: None,
                draw: Draw::new(mesh),
            };
            image.draw.validate()?;
            Ok(content(Arc::new(vec![Element::new(
                n.seed(),
                Geometry::Image(Arc::new(image)),
            )])))
        },
    );
    let mut cards = definition(
        "fold.motion.image_cards",
        "Image Cards on Points",
        "Image",
        vec![
            Socket::required("image", Kind::Image),
            Socket::required("points", Kind::Points),
            scalar("index", 0., "integer"),
            vector("size", [320., 180.], "px"),
            scalar("time_offset", 0., "seconds"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let sources = images(e, n)?;
            let points = e.content(n, "points")?;
            let indices = e.field(n, "index")?;
            let offsets = e.field(n, "time_offset")?;
            let size = e.vector(n, "size")?;
            let meshes = sources
                .iter()
                .map(|r| card_mesh(size, r, n))
                .collect::<Result<Vec<_>, _>>()?;
            let mut result = Vec::new();
            for (i, p) in points.iter().enumerate() {
                e.budget.spend()?;
                let sample = Sample {
                    index: i,
                    count: points.len(),
                    id: p.id,
                    position: [p.transform[4], p.transform[5]],
                    ..Default::default()
                };
                let index = indices.sample(&sample, &mut e.budget)?.scalar()?;
                let mut request = choose(&sources, index)?;
                let mesh =
                    meshes[(index.floor() as i64).rem_euclid(meshes.len() as i64) as usize].clone();
                let seconds = offsets.sample(&sample, &mut e.budget)?.scalar()?;
                if !seconds.is_finite() || seconds.abs() > 1e8 {
                    return Err("invalid per-card time offset".into());
                }
                request.time = request
                    .time
                    .checked_add(
                        fold_foundation::Time::new((seconds * 1e6).round() as i64, 1_000_000)
                            .map_err(|e| e.to_string())?,
                    )
                    .map_err(|e| e.to_string())?;
                let image = ImageGeometry {
                    texture: Texture::Source(request),
                    driver: None,
                    draw: Draw::new(mesh.clone()),
                };
                image.draw.validate()?;
                let prototype = Element::new(n.seed(), Geometry::Image(Arc::new(image)));
                let mut item = Element::new(
                    derived_id(n.seed(), p.id),
                    Geometry::Instance(Arc::new(vec![prototype])),
                );
                item.transform = p.transform;
                item.opacity = p.opacity;
                result.push(item);
            }
            Ok(content(Arc::new(result)))
        },
    );
    card.defaults = || serde_json::json!({"fit":"Contain"});
    cards.defaults = card.defaults;
    card.validate = |n| {
        serde_json::from_value::<Fit>(
            n.settings
                .get("fit")
                .cloned()
                .unwrap_or(serde_json::json!("Contain")),
        )
        .map(|_| ())
        .map_err(|e| e.to_string())
    };
    cards.validate = card.validate;
    let grid = definition(
        "fold.motion.image_grid",
        "Textured Grid",
        "Image",
        vec![
            Socket::required("image", Kind::Image),
            scalar("index", 0., "integer"),
            vector("size", [640., 360.], "px"),
            scalar("columns", 16., "integer"),
            scalar("rows", 9., "integer"),
            boolean("per_cell", false),
            vector("offset", [0.; 2], "px"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let sources = images(e, n)?;
            let index = e.scalar(n, "index")?;
            let size = e.vector(n, "size")?;
            let cols = bounded_count(e.scalar(n, "columns")?, 128)?;
            let rows = bounded_count(e.scalar(n, "rows")?, 128)?;
            if cols == 0 || rows == 0 {
                return Err("grid needs at least one row and column".into());
            }
            let per_cell = e.uniform(n, "per_cell")?.boolean()?;
            let offset = e.field(n, "offset")?;
            let mut mesh = Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
            };
            for y in 0..rows {
                for x in 0..cols {
                    let base = mesh.vertices.len() as u32;
                    for uv in [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
                        let global = [
                            (x as f64 + uv[0]) / cols as f64,
                            (y as f64 + uv[1]) / rows as f64,
                        ];
                        let position = [(global[0] - 0.5) * size[0], (global[1] - 0.5) * size[1]];
                        let sample = Sample {
                            index: (y + uv[1] as usize) * (cols + 1) + x + uv[0] as usize,
                            count: (rows + 1) * (cols + 1),
                            id: derived_id(
                                n.seed(),
                                ((y + uv[1] as usize) * (cols + 1) + x + uv[0] as usize) as u64,
                            ),
                            position,
                            ..Default::default()
                        };
                        let delta = offset.sample(&sample, &mut e.budget)?.vector()?;
                        mesh.vertices.push(Vertex {
                            position: [
                                (position[0] + delta[0]) as f32,
                                (position[1] + delta[1]) as f32,
                            ],
                            uv: if per_cell {
                                uv.map(|v| v as f32)
                            } else {
                                global.map(|v| v as f32)
                            },
                            sample: global.map(|v| v as f32),
                            center: position.map(|v| v as f32),
                        });
                    }
                    mesh.indices
                        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            let image = ImageGeometry {
                texture: Texture::Source(choose(&sources, index)?),
                driver: None,
                draw: Draw::new(Arc::new(mesh)),
            };
            image.draw.validate()?;
            Ok(content(Arc::new(vec![Element::new(
                n.seed(),
                Geometry::Image(Arc::new(image)),
            )])))
        },
    );
    let mut sample = definition(
        "fold.motion.sample_image",
        "Sample Image",
        "Image",
        vec![
            Socket::required("image", Kind::Image),
            scalar("index", 0., "integer"),
            scalar("gain", 1., "factor"),
            scalar("bias", 0., "factor"),
        ],
        vec![("sample", Kind::ImageField)],
        |e, n| {
            let sources = images(e, n)?;
            let image = choose(&sources, e.scalar(n, "index")?)?;
            let channel = serde_json::from_value(
                n.settings
                    .get("channel")
                    .cloned()
                    .unwrap_or(serde_json::json!("Luminance")),
            )
            .map_err(|e| e.to_string())?;
            Ok(output(
                "sample",
                Value::ImageField(Arc::new(Samples {
                    image,
                    channel,
                    remap: [e.scalar(n, "gain")? as f32, e.scalar(n, "bias")? as f32],
                })),
            ))
        },
    );
    sample.defaults = || serde_json::json!({"channel":"Luminance"});
    sample.validate = |n| {
        serde_json::from_value::<Channel>(n.settings["channel"].clone())
            .map(|_| ())
            .map_err(|e| e.to_string())
    };
    sample.description = "Deferred GPU image field for Image Response. Bilinear clamped normalized UVs; unpremultiplied working RGB/luminance or alpha. Gain/bias preserve HDR. CPU fields/Wrangles cannot consume this socket; no hidden pixel readback.";
    vec![
        source,
        card,
        cards,
        grid,
        sample,
        response::definition(),
        ascii::definition(),
        bake::definition(),
    ]
}

mod ascii;
mod placement;
pub use placement::place;

mod response;

pub mod bake;
