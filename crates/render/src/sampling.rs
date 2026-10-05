//! Shared source reconstruction policy: integrate pixel area when reducing,
//! linear interpolation when enlarging. Operate on premultiplied working pixels
//! before the display transform. Separable passes bound work and scratch space.
use crate::{Frame, ImageOp, RenderGraph, region::Region};

pub(crate) fn source_dimensions(op: &ImageOp) -> Option<[u32; 2]> {
    match op {
        ImageOp::Media(image) => Some(image.dimensions()),
        ImageOp::Exr { source, .. } => Some(source.info.dimensions),
        ImageOp::Video { source, .. } | ImageOp::VideoInput { source, .. } => {
            Some([source.info.width, source.info.height])
        }
        _ => None,
    }
}
/// Leave codec reconstruction at native resolution. Scaling belongs after the
/// input color transform, shared by CPU/GPU, preview and delivery.
pub(crate) fn prepare(graph: &mut RenderGraph) {
    let dimensions = [graph.width, graph.height];
    for op in &mut graph.nodes {
        if source_dimensions(op).is_some_and(|native| native != dimensions) {
            *op = ImageOp::WindowedSource {
                source: Box::new(op.clone()),
                dimensions,
                region: Region::full(dimensions),
            };
        }
    }
}
fn taps(index: u32, input: u32, output: u32) -> (i64, i64, f64, f64) {
    let scale = f64::from(input) / f64::from(output);
    if input >= output {
        let left = f64::from(index) * scale;
        let right = f64::from(index + 1) * scale;
        (left.floor() as i64, right.ceil() as i64 - 1, left, right)
    } else {
        let center = (f64::from(index) + 0.5) * scale - 0.5;
        let start = center.floor();
        (start as i64, start as i64 + 1, center, center)
    }
}
fn weight(index: i64, input: u32, output: u32, left: f64, right: f64) -> f64 {
    if input >= output {
        (((index + 1) as f64).min(right) - left.max(index as f64)).max(0.) / (right - left)
    } else {
        (1. - (index as f64 - left).abs()).max(0.)
    }
}
// Avoid evaluating rows that cannot contribute to the visible output region.
pub(crate) fn rows(input: u32, output: u32, region: Region) -> [u32; 2] {
    let first = taps(region.y, input, output)
        .0
        .clamp(0, i64::from(input) - 1) as u32;
    let last = taps(region.y + region.height - 1, input, output)
        .1
        .clamp(0, i64::from(input) - 1) as u32;
    [first, last + 1]
}

pub(crate) fn resize(
    mut frame: Frame,
    dimensions: [u32; 2],
    region: Region,
    cancel: &fold_media::Cancel,
) -> Result<Frame, String> {
    region.validate(dimensions)?;
    if frame.dimensions() == dimensions {
        return crate::region::crop(frame, region);
    }
    let [top, bottom] = rows(frame.height, dimensions[1], region);
    let count = region.width as usize * region.height as usize;
    let scratch_count = region.width as usize * (bottom - top) as usize;
    let _scratch = fold_media::budget::reserve_working(scratch_count as u64 * 16)?;
    let storage = fold_media::budget::reserve_working(count as u64 * 16)?;
    let mut horizontal = Vec::new();
    horizontal
        .try_reserve_exact(scratch_count)
        .map_err(|_| "resize scratch allocation failed")?;
    for y in top..bottom {
        cancel.check()?;
        for x in region.x..region.x + region.width {
            let (start, end, left, right) = taps(x, frame.width, dimensions[0]);
            let mut pixel = [0_f64; 4];
            for sx in start..=end {
                let w = weight(sx, frame.width, dimensions[0], left, right);
                let sx = sx.clamp(0, i64::from(frame.width) - 1) as u32;
                for (v, sample) in pixel
                    .iter_mut()
                    .zip(frame.pixels[(y * frame.width + sx) as usize])
                {
                    *v += f64::from(sample) * w;
                }
            }
            horizontal.push(pixel.map(|v| v as f32));
        }
    }
    let mut pixels = Vec::new();
    pixels
        .try_reserve_exact(count)
        .map_err(|_| "resize allocation failed")?;
    for y in region.y..region.y + region.height {
        cancel.check()?;
        let (start, end, left, right) = taps(y, frame.height, dimensions[1]);
        for x in 0..region.width {
            let mut pixel = [0_f64; 4];
            for sy in start..=end {
                let w = weight(sy, frame.height, dimensions[1], left, right);
                let sy = sy.clamp(0, i64::from(frame.height) - 1) as u32 - top;
                for (v, sample) in pixel
                    .iter_mut()
                    .zip(horizontal[(sy * region.width + x) as usize])
                {
                    *v += f64::from(sample) * w;
                }
            }
            let mut pixel = pixel.map(|v| v as f32);
            pixel[3] = pixel[3].clamp(0., 1.);
            fold_color::validate_pixel(pixel)?;
            pixels.push(pixel);
        }
    }
    frame.width = region.width;
    frame.height = region.height;
    frame.pixels = pixels;
    frame._storage = storage;
    Ok(frame)
}
