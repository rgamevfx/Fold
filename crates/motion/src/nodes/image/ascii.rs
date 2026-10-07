//! Glyph atlas authoring for GPU image-driven text.
use super::*;
pub(super) fn definition() -> Definition {
    let mut d = super::definition(
        "fold.motion.image_ascii",
        "Image to ASCII",
        "Image",
        vec![
            Socket::required("image", Kind::Image),
            scalar("index", 0., "integer"),
            vector("size", [640., 360.], "units"),
            scalar("columns", 80., "integer"),
            scalar("rows", 30., "integer"),
            Socket::value("characters", Datum::Text(" .:-=+*#%@".into()), ""),
            scalar("gain", 1., "factor"),
            scalar("bias", 0., "factor"),
            scalar("color", 1., "factor"),
        ],
        vec![("content", Kind::Content)],
        |e, n| {
            let sources = images(e, n)?;
            let image = choose(&sources, e.scalar(n, "index")?)?;
            let size = e.vector(n, "size")?;
            let cols = bounded_count(e.scalar(n, "columns")?, 256)?;
            let rows = bounded_count(e.scalar(n, "rows")?, 256)?;
            if cols == 0 || rows == 0 || cols * rows > 16384 {
                return Err("ASCII requires 1..16384 cells".into());
            }
            let settings = n.settings::<crate::nodes::content::text::Settings>()?;
            let font = e
                .motion
                .fonts
                .get(&settings.font.ok_or("ASCII requires an embedded font")?)
                .ok_or("missing ASCII font")?;
            let chars = e
                .uniform(n, "characters")?
                .text()?
                .chars()
                .collect::<Vec<_>>();
            if chars.is_empty() || chars.len() > 64 {
                return Err("ASCII ramp needs 1..64 characters, dark to light".into());
            }
            // Atlas in document coordinates, scales with output through normal vector IR.
            let cell_w = e.canvas_size()?[0] / chars.len() as f64;
            let cell_h = (cell_w * 1.5).min(e.canvas_size()?[1]);
            let mut atlas = Vec::new();
            for (i, ch) in chars.iter().enumerate() {
                let glyph = crate::geometry::text::shape(
                    &ch.to_string(),
                    font,
                    cell_h * 0.8,
                    1.,
                    0.5,
                    n.seed(),
                )?;
                let Some(bounds) =
                    crate::geometry::bounds::content_with_budget(&glyph, &mut e.budget)?
                else {
                    continue;
                };
                let fit = ((cell_w * 0.85) / (bounds[2] - bounds[0]).max(1e-6))
                    .min((cell_h * 0.85) / (bounds[3] - bounds[1]).max(1e-6))
                    .min(1.);
                let matrix = [
                    fit,
                    0.,
                    0.,
                    fit,
                    (i as f64 + 0.5) * cell_w - (bounds[0] + bounds[2]) * 0.5 * fit,
                    cell_h * 0.5 - (bounds[1] + bounds[3]) * 0.5 * fit,
                ];
                for mut item in glyph.iter().cloned() {
                    item.transform = crate::geometry::multiply(matrix, item.transform);
                    item.fill = Some([1.; 4]);
                    atlas.push(item);
                }
            }
            let mut mesh = Mesh {
                vertices: Vec::new(),
                indices: Vec::new(),
            };
            for y in 0..rows {
                for x in 0..cols {
                    e.budget.spend()?;
                    let base = mesh.vertices.len() as u32;
                    let uv = [
                        (x as f32 + 0.5) / cols as f32,
                        (y as f32 + 0.5) / rows as f32,
                    ];
                    let center = [
                        (uv[0] - 0.5) * size[0] as f32,
                        (uv[1] - 0.5) * size[1] as f32,
                    ];
                    for local in [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
                        mesh.vertices.push(Vertex {
                            position: [
                                center[0] + (local[0] - 0.5) * size[0] as f32 / cols as f32,
                                center[1] + (local[1] - 0.5) * size[1] as f32 / rows as f32,
                            ],
                            uv: local,
                            sample: uv,
                            center,
                        });
                    }
                    mesh.indices
                        .extend([base, base + 1, base + 2, base, base + 2, base + 3]);
                }
            }
            let mut draw = Draw::new(Arc::new(mesh));
            draw.atlas = [
                chars.len() as f32,
                1. / chars.len() as f32,
                (cell_h / e.canvas_size()?[1]) as f32,
            ];
            draw.remap = [e.scalar(n, "gain")? as f32, e.scalar(n, "bias")? as f32];
            draw.color = e.scalar(n, "color")? as f32;
            draw.validate()?;
            Ok(content(Arc::new(vec![Element::new(
                n.seed(),
                Geometry::Image(Arc::new(ImageGeometry {
                    texture: Texture::Atlas(Arc::new(atlas)),
                    driver: Some(image),
                    draw,
                })),
            )])))
        },
    );
    d.defaults = || serde_json::json!({"font":null});
    d.validate = |n| {
        n.settings::<crate::nodes::content::text::Settings>()
            .map(|_| ())
    };
    d.description = "GPU luminance-selected glyph atlas. Character ramp runs dark to light; each cell samples the image at its center. Embedded font, bounded 16384 cells, no per-frame text shaping per cell or pixel readback.";
    d
}
