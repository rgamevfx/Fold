//! Full-frame image IR and bounded CPU reference evaluator.
use crate::{Frame, SolidPlan};

const MAX_NODES: usize = 4096;
const MAX_PIXELS: u64 = 4_194_304;
const PIXEL_BUDGET: usize = 64 * 1024 * 1024;

/// Transient index in a topologically ordered graph, not an authoring identity.
pub type ImageId = usize;

/// Maps source to destination: x' = a*x + c*y + tx, y' = b*x + d*y + ty.
/// Coordinates are raster pixels, top-left origin, X right, Y down. Samples are
/// at pixel centers. Nearest-neighbor sampling uses transparent black outside.
#[derive(Clone, Copy, Debug)]
pub struct Affine {
    pub a: f64,
    pub b: f64,
    pub c: f64,
    pub d: f64,
    pub tx: f64,
    pub ty: f64,
}

impl Affine {
    pub const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        tx: 0.0,
        ty: 0.0,
    };

    fn inverse(self) -> Result<Self, &'static str> {
        let Self { a, b, c, d, tx, ty } = self;
        let det = a * d - b * c;
        if ![a, b, c, d, tx, ty, det].iter().all(|v| v.is_finite()) || det == 0.0 {
            return Err("transform must be finite and invertible");
        }
        let inverse = Self {
            a: d / det,
            b: -b / det,
            c: -c / det,
            d: a / det,
            tx: (c * ty - d * tx) / det,
            ty: (b * tx - a * ty) / det,
        };
        if ![
            inverse.a, inverse.b, inverse.c, inverse.d, inverse.tx, inverse.ty,
        ]
        .iter()
        .all(|v| v.is_finite())
        {
            return Err("transform inverse must be finite");
        }
        Ok(inverse)
    }
}

/// Every port is a full-resolution scene-linear sRGB premultiplied RGBA image.
#[derive(Clone, Debug)]
pub enum ImageOp {
    /// Validated opaque SDR source. Decoded storage is caller-owned; its RGB8
    /// byte count across all source nodes is separately capped at 64 MiB.
    Media(fold_media::RgbImage),
    Video {
        source: fold_media::VideoSource,
        time: fold_foundation::Time,
    },
    Solid {
        rgba: [f32; 4],
    },
    Transform {
        input: ImageId,
        transform: Affine,
    },
    /// Scales all four premultiplied channels by a finite value in 0..=1.
    Opacity {
        input: ImageId,
        opacity: f32,
    },
    /// Porter-Duff foreground over background; ordering is explicit.
    Over {
        foreground: ImageId,
        background: ImageId,
    },
}

impl ImageOp {
    fn inputs(&self) -> impl Iterator<Item = ImageId> {
        let inputs = match *self {
            Self::Solid { .. } | Self::Media(_) | Self::Video { .. } => [None, None],
            Self::Transform { input, .. } | Self::Opacity { input, .. } => [Some(input), None],
            Self::Over {
                foreground,
                background,
            } => [Some(foreground), Some(background)],
        };
        inputs.into_iter().flatten()
    }
}

/// Immutable during evaluation. Inputs must precede their consumer; this makes
/// cycles unrepresentable in a valid graph. Even unused nodes are validated.
#[derive(Clone, Debug)]
pub struct RenderGraph {
    pub width: u32,
    pub height: u32,
    pub nodes: Vec<ImageOp>,
    pub output: ImageId,
}

impl From<SolidPlan> for RenderGraph {
    fn from(plan: SolidPlan) -> Self {
        Self {
            width: plan.width,
            height: plan.height,
            nodes: vec![ImageOp::Solid { rgba: plan.rgba }],
            output: 0,
        }
    }
}

pub(crate) fn evaluate(
    graph: RenderGraph,
    decoder: &mut fold_media::Decoder,
    cancel: &fold_media::Cancel,
    budget: usize,
) -> Result<Frame, String> {
    let count = u64::from(graph.width) * u64::from(graph.height);
    if count == 0 || count > MAX_PIXELS {
        return Err("resolution must contain 1..=4194304 pixels".into());
    }
    if graph.nodes.len() > MAX_NODES || graph.output >= graph.nodes.len() {
        return Err("graph requires a valid output and at most 4096 nodes".into());
    }
    let mut source_bytes = 0u64;
    for (id, op) in graph.nodes.iter().enumerate() {
        if let ImageOp::Media(source) = op {
            if source.dimensions() != [graph.width, graph.height] {
                return Err("media dimensions must match graph resolution".into());
            }
            source_bytes += count * 3;
            if source_bytes > PIXEL_BUDGET as u64 {
                return Err("render graph exceeds 64 MiB RGB8 source budget".into());
            }
        }
        if op.inputs().any(|input| input >= id) {
            return Err("image inputs must precede their consumer".into());
        }
        match *op {
            ImageOp::Solid { rgba: [r, g, b, a] } => {
                if ![r, g, b, a]
                    .iter()
                    .all(|v| v.is_finite() && (0.0..=1.0).contains(v))
                    || r > a
                    || g > a
                    || b > a
                {
                    return Err("expected finite SDR premultiplied linear RGBA".into());
                }
            }
            ImageOp::Transform { transform, .. } => {
                transform.inverse()?;
            }
            ImageOp::Opacity { opacity, .. } => {
                if !opacity.is_finite() || !(0.0..=1.0).contains(&opacity) {
                    return Err("opacity must be finite and in 0..=1".into());
                }
            }
            ImageOp::Video { ref source, time } => {
                source.info.frame_at(time)?;
            }
            ImageOp::Over { .. } | ImageOp::Media(_) => {}
        }
    }

    let mut needed = vec![false; graph.nodes.len()];
    needed[graph.output] = true;
    let mut uses = vec![0usize; graph.nodes.len()];
    for id in (0..graph.nodes.len()).rev() {
        if needed[id] {
            for input in graph.nodes[id].inputs() {
                needed[input] = true;
                uses[input] += 1;
            }
        }
    }
    let mut frames: Vec<Option<Frame>> = (0..graph.nodes.len()).map(|_| None).collect();
    let count = count as usize;
    let bytes = count * std::mem::size_of::<[f32; 4]>();
    let mut live_bytes = 0;
    for (id, op) in graph.nodes.iter().enumerate() {
        if !needed[id] {
            continue;
        }
        cancel.check()?;
        if live_bytes + bytes > budget {
            return Err(format!(
                "render graph exceeds {} MiB live pixel budget",
                budget / (1024 * 1024)
            ));
        }
        let mut pixels = Vec::new();
        pixels
            .try_reserve_exact(count)
            .map_err(|_| "frame allocation failed")?;
        let input = |id: ImageId| frames[id].as_ref().expect("validated live input");
        match *op {
            ImageOp::Media(ref source) => pixels.extend(source.linear_pixels()),
            ImageOp::Video { ref source, time } => {
                let frame = decoder.decode(source, time, [graph.width, graph.height], cancel)?;
                pixels.extend(frame.linear_pixels());
            }
            ImageOp::Solid { rgba } => pixels.resize(count, rgba),
            ImageOp::Opacity {
                input: source,
                opacity,
            } => {
                pixels.extend(input(source).pixels.iter().map(|p| p.map(|v| v * opacity)));
            }
            ImageOp::Over {
                foreground,
                background,
            } => {
                pixels.extend(
                    input(foreground)
                        .pixels
                        .iter()
                        .zip(&input(background).pixels)
                        .map(|(fg, bg)| std::array::from_fn(|c| fg[c] + bg[c] * (1.0 - fg[3]))),
                );
            }
            ImageOp::Transform {
                input: source,
                transform,
            } => {
                let m = transform.inverse()?;
                for y in 0..graph.height {
                    cancel.check()?;
                    for x in 0..graph.width {
                        let x = f64::from(x) + 0.5;
                        let y = f64::from(y) + 0.5;
                        let sx = (m.a * x + m.c * y + m.tx).floor();
                        let sy = (m.b * x + m.d * y + m.ty).floor();
                        let pixel = if sx >= 0.0
                            && sy >= 0.0
                            && sx < f64::from(graph.width)
                            && sy < f64::from(graph.height)
                        {
                            input(source).pixels[sy as usize * graph.width as usize + sx as usize]
                        } else {
                            [0.0; 4]
                        };
                        pixels.push(pixel);
                    }
                }
            }
        }
        frames[id] = Some(Frame {
            width: graph.width,
            height: graph.height,
            pixels,
        });
        live_bytes += bytes;
        for source in op.inputs() {
            uses[source] -= 1;
            if uses[source] == 0 {
                frames[source] = None;
                live_bytes -= bytes;
            }
        }
    }
    Ok(frames[graph.output].take().expect("evaluated output"))
}
