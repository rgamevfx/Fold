//! Spatial demand planning over the shared logical graph. A common working
//! rectangle conservatively encloses every required input region. This keeps
//! existing CPU/GPU operator semantics identical while avoiding full-frame
//! processing when the dependency footprint is smaller than the image.
use crate::{Frame, ImageOp, RenderGraph};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Region {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}
impl Region {
    pub fn full([width, height]: [u32; 2]) -> Self {
        Self {
            x: 0,
            y: 0,
            width,
            height,
        }
    }
    pub fn dimensions(self) -> [u32; 2] {
        [self.width, self.height]
    }
    pub fn validate(self, dimensions: [u32; 2]) -> Result<(), String> {
        if self.width == 0
            || self.height == 0
            || u64::from(self.x) + u64::from(self.width) > u64::from(dimensions[0])
            || u64::from(self.y) + u64::from(self.height) > u64::from(dimensions[1])
        {
            return Err("render region must be nonempty and inside the image".into());
        }
        Ok(())
    }
    fn union(self, other: Self) -> Self {
        let x = self.x.min(other.x);
        let y = self.y.min(other.y);
        Self {
            x,
            y,
            width: (self.x + self.width).max(other.x + other.width) - x,
            height: (self.y + self.height).max(other.y + other.height) - y,
        }
    }
    fn expand(self, padding: [u32; 2], dimensions: [u32; 2]) -> Self {
        let x = self.x.saturating_sub(padding[0]);
        let y = self.y.saturating_sub(padding[1]);
        Self {
            x,
            y,
            width: (self.x + self.width)
                .saturating_add(padding[0])
                .min(dimensions[0])
                - x,
            height: (self.y + self.height)
                .saturating_add(padding[1])
                .min(dimensions[1])
                - y,
        }
    }
}

/// The graph operates in local working-rectangle coordinates. `output` is the
/// requested crop in that local raster; it must be extracted before display.
pub struct Plan {
    pub graph: RenderGraph,
    pub working: Region,
    pub output: Region,
}
impl Plan {
    pub fn new(mut graph: RenderGraph, region: Region, aces: bool) -> Result<Self, String> {
        crate::sampling::prepare(&mut graph);
        crate::graph::validate(&graph, aces)?;
        let dimensions = [graph.width, graph.height];
        region.validate(dimensions)?;
        let mut required = vec![None::<Region>; graph.nodes.len()];
        required[graph.output] = Some(region);
        let mut working = region;
        for id in (0..graph.nodes.len()).rev() {
            let Some(output) = required[id] else {
                continue;
            };
            working = working.union(output);
            let input = match &graph.nodes[id] {
                ImageOp::Blur { radius, .. } => output.expand([*radius; 2], dimensions),
                ImageOp::Gaussian { size, .. } => output.expand(
                    size.map(|s| (crate::operations::gaussian_weights(s).len() / 2) as u32),
                    dimensions,
                ),
                ImageOp::Transform { transform, .. } | ImageOp::Resample { transform, .. } => {
                    let m = transform.inverse()?;
                    // Conservative pixel-center bounds plus the largest supported
                    // reconstruction kernel (bicubic). Clamp only at image edges.
                    let mut low = [f64::INFINITY; 2];
                    let mut high = [f64::NEG_INFINITY; 2];
                    for x in [output.x, output.x + output.width] {
                        for y in [output.y, output.y + output.height] {
                            let x = f64::from(x);
                            let y = f64::from(y);
                            let p = [m.a * x + m.c * y + m.tx, m.b * x + m.d * y + m.ty];
                            for i in 0..2 {
                                low[i] = low[i].min(p[i]);
                                high[i] = high[i].max(p[i]);
                            }
                        }
                    }
                    let low: [u32; 2] = std::array::from_fn(|i| {
                        (low[i].floor() - 3.).clamp(0., f64::from(dimensions[i] - 1)) as u32
                    });
                    let high: [u32; 2] = std::array::from_fn(|i| {
                        (high[i].ceil() + 3.).clamp(f64::from(low[i] + 1), f64::from(dimensions[i]))
                            as u32
                    });
                    Region {
                        x: low[0],
                        y: low[1],
                        width: high[0] - low[0],
                        height: high[1] - low[1],
                    }
                }
                _ => output,
            };
            for source in graph.nodes[id].inputs() {
                required[source] = Some(required[source].map_or(input, |r| r.union(input)));
            }
        }
        if working != Region::full(dimensions) {
            for op in &mut graph.nodes {
                match op {
                    ImageOp::Transform { transform: m, .. }
                    | ImageOp::Resample { transform: m, .. } => {
                        let x = f64::from(working.x);
                        let y = f64::from(working.y);
                        m.tx += m.a * x + m.c * y - x;
                        m.ty += m.b * x + m.d * y - y;
                    }
                    ImageOp::Crop { rect, .. } => {
                        rect[0] = rect[0].saturating_sub(working.x);
                        rect[2] = rect[2].saturating_sub(working.x);
                        rect[1] = rect[1].saturating_sub(working.y);
                        rect[3] = rect[3].saturating_sub(working.y);
                    }
                    ImageOp::Vector(drawings) => {
                        let mut local = drawings.as_ref().clone();
                        for d in &mut local {
                            d.transform[4] -= f64::from(working.x);
                            d.transform[5] -= f64::from(working.y);
                        }
                        *drawings = std::sync::Arc::new(local);
                    }
                    ImageOp::WindowedSource { region, .. } => {
                        region.x += working.x;
                        region.y += working.y;
                        region.width = working.width;
                        region.height = working.height;
                    }
                    ImageOp::Media(_)
                    | ImageOp::Exr { .. }
                    | ImageOp::Video { .. }
                    | ImageOp::VideoInput { .. } => {
                        *op = ImageOp::WindowedSource {
                            source: Box::new(op.clone()),
                            dimensions,
                            region: working,
                        };
                    }
                    _ => {}
                }
            }
            graph.width = working.width;
            graph.height = working.height;
        }
        Ok(Self {
            graph,
            working,
            output: Region {
                x: region.x - working.x,
                y: region.y - working.y,
                ..region
            },
        })
    }
}

/// Worker-only extraction. The returned frame retains working color and timing.
pub fn crop(mut frame: Frame, region: Region) -> Result<Frame, String> {
    region.validate(frame.dimensions())?;
    if region == Region::full(frame.dimensions()) {
        return Ok(frame);
    }
    let count = region.width as usize * region.height as usize;
    let storage = fold_media::budget::reserve_working(count as u64 * 16)?;
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count)
        .map_err(|_| "region allocation failed")?;
    for y in region.y..region.y + region.height {
        let start = (y * frame.width + region.x) as usize;
        pixels.extend_from_slice(&frame.pixels[start..start + region.width as usize]);
    }
    frame.width = region.width;
    frame.height = region.height;
    frame.pixels = pixels;
    frame._storage = storage;
    Ok(frame)
}
