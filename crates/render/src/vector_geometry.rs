//! Shared geometric preparation and analytic CPU oracle. Pixel coverage is not
//! produced by preparation: the GPU consumes triangles and float paint values.
use crate::vector::{self, Drawing, Segment, Stroke};
use fold_media::Cancel;
use lyon_tessellation::{
    FillGeometryBuilder, FillOptions, FillTessellator, FillVertex, GeometryBuilder,
    GeometryBuilderError, VertexId,
    path::{Path, math::point},
};
use std::{collections::VecDeque, sync::Arc};

pub(crate) const MAX_TRIANGLES: usize = 262_144;
type Point = [f64; 2];
pub(crate) type Triangle = [[f32; 2]; 3];
#[derive(Clone, PartialEq)]
struct Key {
    path: Arc<Vec<Segment>>,
    stroke: Option<Stroke>,
    even_odd: bool,
    scale: i32,
}
struct Entry {
    key: Key,
    triangles: Arc<Vec<Triangle>>,
}
#[derive(Default)]
pub(crate) struct GeometryCache {
    entries: VecDeque<Entry>,
    bytes: usize,
}
const CACHE_BYTES: usize = 16 * 1024 * 1024;

struct Builder<'a> {
    vertices: Vec<[f32; 2]>,
    triangles: Vec<Triangle>,
    cancel: &'a Cancel,
    overflow: bool,
}
impl GeometryBuilder for Builder<'_> {
    fn add_triangle(&mut self, a: VertexId, b: VertexId, c: VertexId) {
        if self.triangles.len() == MAX_TRIANGLES {
            self.overflow = true;
            return;
        }
        self.triangles.push([
            self.vertices[a.to_usize()],
            self.vertices[b.to_usize()],
            self.vertices[c.to_usize()],
        ]);
    }
}
impl FillGeometryBuilder for Builder<'_> {
    fn add_fill_vertex(&mut self, v: FillVertex<'_>) -> Result<VertexId, GeometryBuilderError> {
        if self.vertices.len() == MAX_TRIANGLES || self.overflow || self.cancel.check().is_err() {
            return Err(GeometryBuilderError::TooManyVertices);
        }
        let id = VertexId(self.vertices.len() as u32);
        let p = v.position();
        self.vertices.push([p.x, p.y]);
        Ok(id)
    }
}
impl GeometryCache {
    fn get(
        &mut self,
        d: &Drawing,
        stroke: Option<&Stroke>,
        cancel: &Cancel,
    ) -> Result<Arc<Vec<Triangle>>, String> {
        let [a, b, c, e, _, _] = d.transform;
        let scale = ((a.abs() + c.abs())
            .max(b.abs() + e.abs())
            .max(1e-9)
            .log2()
            .ceil()) as i32;
        let mut style = stroke.cloned();
        if let Some(s) = style.as_mut() {
            s.color = [0.; 4];
        }
        let key = Key {
            path: d.path.clone(),
            stroke: style,
            even_odd: stroke.is_none() && d.even_odd,
            scale,
        };
        if let Some(index) = self.entries.iter().position(|e| e.key == key) {
            let entry = self.entries.remove(index).unwrap();
            let result = entry.triangles.clone();
            self.entries.push_back(entry);
            return Ok(result);
        }
        // Bound flattening before lyon builds its event queue: callbacks occur
        // after event generation and alone cannot bound pathological subdivision.
        let tolerance = 2f64.powi(-8 - scale);
        let mut previous = [0.; 2];
        let mut estimated = 0f64;
        for segment in d.path.iter() {
            cancel.check()?;
            let (points, end): (&[[f64; 2]], [f64; 2]) = match segment {
                Segment::Move(p) | Segment::Line(p) => {
                    previous = *p;
                    estimated += 1.;
                    continue;
                }
                Segment::Quad(a, b) => (std::slice::from_ref(a), *b),
                Segment::Cubic(a, b, c) => {
                    let span = (a[0] - b[0]).abs().max((a[1] - b[1]).abs());
                    estimated += (8. * span / tolerance).sqrt();
                    (std::slice::from_ref(a), *c)
                }
                Segment::Close => {
                    estimated += 1.;
                    continue;
                }
            };
            let span = points
                .iter()
                .chain(std::iter::once(&end))
                .map(|p| (p[0] - previous[0]).abs().max((p[1] - previous[1]).abs()))
                .fold(0f64, f64::max);
            estimated += (8. * span / tolerance).sqrt() + 1.;
            previous = end;
        }
        if let Some(s) = stroke {
            estimated += (64. * s.width / tolerance).sqrt();
        }
        if estimated > MAX_TRIANGLES as f64 / 4. {
            return Err("vector subdivision work budget exceeded".into());
        }
        let Some(mut path) = vector::build_path(d) else {
            return Ok(Arc::new(Vec::new()));
        };
        if let Some(s) = stroke {
            path = path
                .stroke(&vector::raster_stroke(s), 2f32.powi(scale))
                .ok_or("invalid stroke geometry")?;
        }
        let mut builder = Path::builder();
        let mut open = false;
        for segment in path.segments() {
            use tiny_skia::PathSegment::*;
            match segment {
                MoveTo(p) => {
                    if open {
                        builder.end(true);
                    }
                    builder.begin(point(p.x, p.y));
                    open = true;
                }
                LineTo(p) => {
                    builder.line_to(point(p.x, p.y));
                }
                QuadTo(a, b) => {
                    builder.quadratic_bezier_to(point(a.x, a.y), point(b.x, b.y));
                }
                CubicTo(a, b, c) => {
                    builder.cubic_bezier_to(point(a.x, a.y), point(b.x, b.y), point(c.x, c.y));
                }
                Close => {
                    if open {
                        builder.end(true);
                        open = false;
                    }
                }
            }
        }
        if open {
            builder.end(true);
        }
        let mut output = Builder {
            vertices: Vec::new(),
            triangles: Vec::new(),
            cancel,
            overflow: false,
        };
        let options = FillOptions::default()
            .with_tolerance(2f32.powi(-8 - scale))
            .with_fill_rule(if key.even_odd {
                lyon_tessellation::FillRule::EvenOdd
            } else {
                lyon_tessellation::FillRule::NonZero
            });
        FillTessellator::new()
            .tessellate_path(&builder.build(), &options, &mut output)
            .map_err(|e| format!("vector tessellation: {e:?}"))?;
        cancel.check()?;
        if output.overflow {
            return Err("vector triangle budget exceeded".into());
        }
        let triangles = Arc::new(output.triangles);
        let bytes = triangles.capacity() * std::mem::size_of::<Triangle>()
            + key.path.capacity() * std::mem::size_of::<Segment>();
        while !self.entries.is_empty()
            && (self.bytes + bytes > CACHE_BYTES || self.entries.len() >= 512)
        {
            let old = self.entries.pop_front().unwrap();
            self.bytes -= old.triangles.capacity() * std::mem::size_of::<Triangle>()
                + old.key.path.capacity() * std::mem::size_of::<Segment>();
        }
        if bytes <= CACHE_BYTES {
            self.bytes += bytes;
            self.entries.push_back(Entry {
                key,
                triangles: triangles.clone(),
            });
        }
        Ok(triangles)
    }
}

#[derive(Clone, Copy)]
struct Polygon {
    points: [Point; 8],
    count: usize,
}
impl Polygon {
    fn triangle(t: [Point; 3]) -> Self {
        let mut points = [[0.; 2]; 8];
        points[..3].copy_from_slice(&t);
        Self { points, count: 3 }
    }
    fn clip(self, axis: usize, bound: f64, greater: bool) -> Self {
        let mut out = Self {
            points: [[0.; 2]; 8],
            count: 0,
        };
        if self.count == 0 {
            return out;
        }
        let mut a = self.points[self.count - 1];
        let mut ain = (a[axis] >= bound) == greater;
        for &b in &self.points[..self.count] {
            let bin = (b[axis] >= bound) == greater;
            if ain != bin {
                let t = (bound - a[axis]) / (b[axis] - a[axis]);
                out.points[out.count] = [a[0] + t * (b[0] - a[0]), a[1] + t * (b[1] - a[1])];
                out.count += 1;
            }
            if bin {
                out.points[out.count] = b;
                out.count += 1;
            }
            a = b;
            ain = bin;
        }
        out
    }
}
pub(crate) fn pixel_area(t: &Triangle, x: u32, y: u32) -> f32 {
    let p = Polygon::triangle(t.map(|p| {
        [
            f64::from(p[0]) - f64::from(x),
            f64::from(p[1]) - f64::from(y),
        ]
    }))
    .clip(0, 0., true)
    .clip(0, 1., false)
    .clip(1, 0., true)
    .clip(1, 1., false);
    let mut area = 0.;
    for i in 0..p.count {
        let a = p.points[i];
        let b = p.points[(i + 1) % p.count];
        area += a[0] * b[1] - a[1] * b[0];
    }
    (area.abs() * 0.5).min(1.) as f32
}
#[derive(Debug)]
pub(crate) struct Paint {
    pub rect: [u32; 4],
    pub color: [f32; 4],
    pub triangles: Vec<Triangle>,
}
pub(crate) fn prepare(
    cache: &mut GeometryCache,
    drawings: &[Drawing],
    [width, height]: [u32; 2],
    cancel: &Cancel,
) -> Result<Vec<Paint>, String> {
    vector::validate_working(drawings, true)?;
    let mut paints = Vec::new();
    let mut total = 0;
    for d in drawings {
        cancel.check()?;
        let [a, b, c, e, tx, ty] = d.transform;
        if a * e - b * c == 0. || d.path.is_empty() {
            continue;
        }
        for (color, stroke) in [
            d.fill.map(|c| (c, None)),
            d.stroke
                .as_ref()
                .filter(|s| s.width > 0.)
                .map(|s| (s.color, Some(s))),
        ]
        .into_iter()
        .flatten()
        {
            if color[3] == 0. {
                continue;
            }
            let geometry = cache.get(d, stroke, cancel)?;
            let mut triangles = Vec::new();
            let mut bounds = [width as f32, height as f32, 0f32, 0f32];
            for triangle in geometry.iter() {
                if triangles.len() % 4096 == 0 {
                    cancel.check()?;
                }
                let p = Polygon::triangle(triangle.map(|[x, y]| {
                    let x = f64::from(x);
                    let y = f64::from(y);
                    [a * x + c * y + tx, b * x + e * y + ty]
                }))
                .clip(0, 0., true)
                .clip(0, f64::from(width), false)
                .clip(1, 0., true)
                .clip(1, f64::from(height), false);
                for i in 1..p.count.saturating_sub(1) {
                    total += 1;
                    if total > MAX_TRIANGLES {
                        return Err("vector triangle budget exceeded".into());
                    }
                    let t =
                        [p.points[0], p.points[i], p.points[i + 1]].map(|v| v.map(|v| v as f32));
                    for v in t {
                        bounds[0] = bounds[0].min(v[0]);
                        bounds[1] = bounds[1].min(v[1]);
                        bounds[2] = bounds[2].max(v[0]);
                        bounds[3] = bounds[3].max(v[1]);
                    }
                    triangles.push(t);
                }
            }
            if triangles.is_empty() {
                continue;
            }
            let x = bounds[0].floor() as u32;
            let y = bounds[1].floor() as u32;
            paints.push(Paint {
                rect: [
                    x,
                    y,
                    bounds[2].ceil() as u32 - x,
                    bounds[3].ceil() as u32 - y,
                ],
                color: color.map(|v| v as f32),
                triangles,
            });
        }
    }
    Ok(paints)
}
#[repr(C)]
#[derive(Clone, Copy)]
#[cfg_attr(feature = "gpu", derive(bytemuck::Pod, bytemuck::Zeroable))]
pub(crate) struct PaintRecord {
    pub rect: [u32; 4],
    pub color: [f32; 4],
}
pub(crate) struct Packet {
    pub triangles: Vec<Triangle>,
    pub paints: Vec<PaintRecord>,
    pub tiles: Vec<u32>,
}
pub(crate) fn bin(
    paints: Vec<Paint>,
    [width, height]: [u32; 2],
    cancel: &Cancel,
) -> Result<Packet, String> {
    struct Group {
        paint: u32,
        indices: Vec<u32>,
    }
    let columns = width.div_ceil(16);
    let mut bins: Vec<Vec<Group>> = (0..columns * height.div_ceil(16))
        .map(|_| Vec::new())
        .collect();
    let mut triangles = Vec::new();
    let mut records = Vec::new();
    let mut references = 0usize;
    let mut groups = 0usize;
    for p in paints {
        let paint = records.len() as u32;
        records.push(PaintRecord {
            rect: p.rect,
            color: p.color,
        });
        for t in p.triangles {
            cancel.check()?;
            let left = t.iter().map(|v| v[0]).fold(f32::INFINITY, f32::min).floor() as u32;
            let top = t.iter().map(|v| v[1]).fold(f32::INFINITY, f32::min).floor() as u32;
            let right = t.iter().map(|v| v[0]).fold(0f32, f32::max).ceil() as u32;
            let bottom = t.iter().map(|v| v[1]).fold(0f32, f32::max).ceil() as u32;
            references += (right.div_ceil(16) - left / 16) as usize
                * (bottom.div_ceil(16) - top / 16) as usize;
            if references > 2 * 1024 * 1024 {
                return Err("vector triangle/tile work budget exceeded".into());
            }
            let index = triangles.len() as u32;
            triangles.push(t);
            for y in top / 16..bottom.div_ceil(16) {
                for x in left / 16..right.div_ceil(16) {
                    let bin = &mut bins[(y * columns + x) as usize];
                    if bin.last().is_none_or(|g| g.paint != paint) {
                        groups += 1;
                        if groups > MAX_TRIANGLES {
                            return Err("vector paint/tile budget exceeded".into());
                        }
                        bin.push(Group {
                            paint,
                            indices: Vec::new(),
                        });
                    }
                    bin.last_mut().unwrap().indices.push(index);
                }
            }
        }
    }
    let mut tiles = Vec::with_capacity(bins.len() * 2 + groups * 2 + references);
    tiles.resize(bins.len() * 2, 0);
    for (i, bin) in bins.into_iter().enumerate() {
        tiles[2 * i] = tiles.len() as u32;
        for group in bin {
            tiles.push(group.paint);
            tiles.push(group.indices.len() as u32);
            tiles.extend(group.indices);
        }
        tiles[2 * i + 1] = tiles.len() as u32;
    }
    if triangles.is_empty() {
        triangles.push([[0.; 2]; 3]);
    }
    if records.is_empty() {
        records.push(PaintRecord {
            rect: [0; 4],
            color: [0.; 4],
        });
    }
    Ok(Packet {
        triangles,
        paints: records,
        tiles,
    })
}
pub(crate) fn rasterize(
    drawings: &[Drawing],
    width: u32,
    height: u32,
    cancel: &Cancel,
) -> Result<Vec<[f32; 4]>, String> {
    let prepared = prepare(
        &mut GeometryCache::default(),
        drawings,
        [width, height],
        cancel,
    )?;
    let packet = bin(prepared, [width, height], cancel)?;
    let mut pixels = vec![[0.; 4]; width as usize * height as usize];
    for py in 0..height {
        for px in 0..width {
            if px % 64 == 0 {
                cancel.check()?;
            }
            let tile = (py / 16 * width.div_ceil(16) + px / 16) as usize;
            let mut at = packet.tiles[2 * tile] as usize;
            let end = packet.tiles[2 * tile + 1] as usize;
            let p = &mut pixels[(py * width + px) as usize];
            while at < end {
                let paint = packet.paints[packet.tiles[at] as usize];
                let count = packet.tiles[at + 1] as usize;
                at += 2;
                let mut coverage = 0.;
                for (i, &t) in packet.tiles[at..at + count].iter().enumerate() {
                    if i % 256 == 0 {
                        cancel.check()?;
                    }
                    coverage += pixel_area(&packet.triangles[t as usize], px, py);
                }
                at += count;
                let alpha = coverage.clamp(0., 1.) * paint.color[3];
                for (v, c) in p[..3].iter_mut().zip(paint.color) {
                    *v = c * alpha + *v * (1. - alpha);
                }
                p[3] = alpha + p[3] * (1. - alpha);
            }
        }
    }
    Ok(pixels)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn shape(path: Vec<Segment>) -> Drawing {
        Drawing {
            path: Arc::new(path),
            transform: [1., 0., 0., 1., 0., 0.],
            fill: Some([1.; 4]),
            even_odd: false,
            stroke: None,
        }
    }
    fn square(x: f64, y: f64, w: f64) -> Vec<Segment> {
        vec![
            Segment::Move([x, y]),
            Segment::Line([x + w, y]),
            Segment::Line([x + w, y + w]),
            Segment::Line([x, y + w]),
            Segment::Close,
        ]
    }
    #[test]
    fn winding_holes_self_intersections_and_fractional_area() {
        let cancel = Cancel::default();
        let mut path = square(0., 0., 4.);
        path.extend(square(1., 1., 2.));
        let mut d = shape(path);
        assert_eq!(rasterize(&[d.clone()], 4, 4, &cancel).unwrap()[5][3], 1.);
        d.even_odd = true;
        assert_eq!(rasterize(&[d], 4, 4, &cancel).unwrap()[5][3], 0.);
        let bow = shape(vec![
            Segment::Move([0., 0.]),
            Segment::Line([4., 4.]),
            Segment::Line([0., 4.]),
            Segment::Line([4., 0.]),
            Segment::Close,
        ]);
        assert!(
            (rasterize(&[bow], 4, 4, &cancel)
                .unwrap()
                .iter()
                .map(|p| p[3])
                .sum::<f32>()
                - 8.)
                .abs()
                < 1e-5
        );
        let fractional = shape(square(0.1, 0.1, 0.3));
        assert!((rasterize(&[fractional], 1, 1, &cancel).unwrap()[0][3] - 0.09).abs() < 1e-6);
    }
    #[test]
    fn immutable_geometry_reused_across_color_and_translation() {
        let cancel = Cancel::default();
        let mut cache = GeometryCache::default();
        let mut d = shape(square(0., 0., 4.));
        let first = cache.get(&d, None, &cancel).unwrap();
        d.fill = Some([2., 0., 0., 0.4]);
        d.transform[4] = 7.;
        let second = cache.get(&d, None, &cancel).unwrap();
        assert!(Arc::ptr_eq(&first, &second));
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(prepare(&mut cache, &[d], [16, 16], &cancel).is_err());
    }
    #[test]
    fn excessive_subdivision_is_rejected_before_tessellation() {
        let mut d = shape(vec![
            Segment::Move([0., 0.]),
            Segment::Cubic([0., 1e9], [1e9, 0.], [1e9, 1e9]),
            Segment::Close,
        ]);
        d.transform[0] = 1e9;
        assert!(
            prepare(
                &mut GeometryCache::default(),
                &[d],
                [16, 16],
                &Cancel::default()
            )
            .unwrap_err()
            .contains("subdivision")
        );
    }
    #[test]
    fn analytic_area_has_no_sample_grid_quantization() {
        let t = [[0., 0.], [1., 0.], [0., 1.]];
        assert_eq!(pixel_area(&t, 0, 0), 0.5);
        assert_eq!(pixel_area(&t, 1, 0), 0.);
        let t = [[0., 0.], [1., 0.], [0., 0.2]];
        assert!((pixel_area(&t, 0, 0) - 0.1).abs() < 1e-7);
    }
}
