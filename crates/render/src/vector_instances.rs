//! Shared local triangles and an immutable spatial hierarchy. Frame preparation
//! touches instance records and bounds, never expands/transforms source triangles.
use crate::{
    vector::{self, Drawing},
    vector_geometry::{GeometryCache, MAX_INSTANCE_TRIANGLES, MAX_TRIANGLES, Triangle},
};
use fold_media::Cancel;
use std::{
    collections::{HashMap, VecDeque},
    sync::Arc,
};

const CACHE_BYTES: usize = 32 * 1024 * 1024;
const MAX_COVERED_PIXELS: u64 = 256 * 1024 * 1024;

#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Node {
    pub bounds: [f32; 4],
    // Triangle start/count, escape node, unused. Internal nodes have count zero.
    pub links: [u32; 4],
}
pub(crate) struct Source {
    original: Arc<Vec<Triangle>>,
    pub triangles: Vec<Triangle>,
    pub nodes: Vec<Node>,
}
impl Source {
    fn new(original: Arc<Vec<Triangle>>, cancel: &Cancel) -> Result<Self, String> {
        fn build(
            triangles: &mut [Triangle],
            start: u32,
            nodes: &mut Vec<Node>,
            cancel: &Cancel,
        ) -> Result<(), String> {
            cancel.check()?;
            let mut bounds = [
                f32::INFINITY,
                f32::INFINITY,
                f32::NEG_INFINITY,
                f32::NEG_INFINITY,
            ];
            for p in triangles.iter().flat_map(|t| t.iter()) {
                for a in 0..2 {
                    bounds[a] = bounds[a].min(p[a]);
                    bounds[a + 2] = bounds[a + 2].max(p[a]);
                }
            }
            let index = nodes.len();
            nodes.push(Node {
                bounds,
                links: [start, triangles.len() as u32, 0, 0],
            });
            if triangles.len() > 8 {
                let axis = usize::from(bounds[3] - bounds[1] > bounds[2] - bounds[0]);
                let middle = triangles.len() / 2;
                triangles.select_nth_unstable_by(middle, |a, b| {
                    let center = |t: &Triangle| t.iter().map(|p| p[axis] as f64).sum::<f64>();
                    center(a).total_cmp(&center(b))
                });
                let (left, right) = triangles.split_at_mut(middle);
                build(left, start, nodes, cancel)?;
                build(right, start + middle as u32, nodes, cancel)?;
                nodes[index].links[1] = 0;
            }
            nodes[index].links[2] = nodes.len() as u32;
            Ok(())
        }
        let mut triangles = original.as_ref().clone();
        let mut nodes = Vec::new();
        if !triangles.is_empty() {
            build(&mut triangles, 0, &mut nodes, cancel)?;
        }
        Ok(Self {
            original,
            triangles,
            nodes,
        })
    }
    fn bytes(&self) -> usize {
        self.original.capacity() * size_of::<Triangle>()
            + self.triangles.capacity() * size_of::<Triangle>()
            + self.nodes.capacity() * size_of::<Node>()
    }
}
#[derive(Default)]
pub(crate) struct Sources {
    geometry: GeometryCache,
    entries: VecDeque<Arc<Source>>,
    bytes: usize,
}

/// GPU layout: six vec4 records. Split coefficients retain the authored f64
/// transform when producing f32 screen triangles on devices without shader f64.
#[repr(C)]
#[derive(Clone, Copy, Debug, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Instance {
    pub matrix: [f32; 4],
    pub matrix_low: [f32; 4],
    pub translation: [f32; 4],
    pub color: [f32; 4],
    // Source triangle/node starts, triangle/node counts.
    pub source: [u32; 4],
    // Screen triangle/node starts, unused, unused.
    pub target: [u32; 4],
}
#[repr(C)]
#[derive(Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub(crate) struct Job {
    pub instance: u32,
    pub start: u32,
}
pub(crate) struct Scene {
    pub sources: Vec<Arc<Source>>,
    pub instances: Vec<Instance>,
    pub jobs: Vec<Job>,
    pub triangles: usize,
    pub nodes: usize,
    pub unique_triangles: usize,
}
impl Sources {
    pub fn prepare(
        &mut self,
        drawings: &[Drawing],
        size: [u32; 2],
        cancel: &Cancel,
    ) -> Result<Scene, String> {
        vector::validate_working(drawings, true)?;
        let mut scene = Scene {
            sources: vec![],
            instances: vec![],
            jobs: vec![],
            triangles: 0,
            nodes: 0,
            unique_triangles: 0,
        };
        let mut source_nodes = 0;
        let mut source_offsets = HashMap::new();
        let mut covered_pixels = 0u64;
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
                let geometry = self.geometry.get(d, stroke, cancel)?;
                if geometry.is_empty() {
                    continue;
                }
                let source = if let Some(index) = self
                    .entries
                    .iter()
                    .position(|s| Arc::ptr_eq(&s.original, &geometry))
                {
                    let source = self.entries.remove(index).unwrap();
                    self.entries.push_back(source.clone());
                    source
                } else {
                    let source = Arc::new(Source::new(geometry, cancel)?);
                    let bytes = source.bytes();
                    while !self.entries.is_empty()
                        && (self.bytes + bytes > CACHE_BYTES || self.entries.len() >= 512)
                    {
                        self.bytes -= self.entries.pop_front().unwrap().bytes();
                    }
                    if bytes <= CACHE_BYTES {
                        self.bytes += bytes;
                        self.entries.push_back(source.clone());
                    }
                    source
                };
                let bounds = source.nodes[0].bounds;
                let corners = [
                    [bounds[0], bounds[1]],
                    [bounds[2], bounds[1]],
                    [bounds[0], bounds[3]],
                    [bounds[2], bounds[3]],
                ];
                let mut rect = [
                    f64::INFINITY,
                    f64::INFINITY,
                    f64::NEG_INFINITY,
                    f64::NEG_INFINITY,
                ];
                for [x, y] in corners {
                    let p = [
                        a * f64::from(x) + c * f64::from(y) + tx,
                        b * f64::from(x) + e * f64::from(y) + ty,
                    ];
                    for axis in 0..2 {
                        rect[axis] = rect[axis].min(p[axis]);
                        rect[axis + 2] = rect[axis + 2].max(p[axis]);
                    }
                }
                // Whole-instance culling is constant work; GPU tile preparation
                // performs the fine screen-space visibility check.
                if rect[2] < -1.
                    || rect[3] < -1.
                    || rect[0] > f64::from(size[0]) + 1.
                    || rect[1] > f64::from(size[1]) + 1.
                {
                    continue;
                }
                // Clip only precision-sensitive geometry, then use the same
                // retained hierarchy, visibility and coverage kernels as every instance.
                let (source, transform) = if rect.iter().any(|v| v.abs() > 65536.) {
                    let triangles = crate::vector_geometry::clipped_triangles(
                        &source.original,
                        d.transform,
                        size,
                        cancel,
                    )?;
                    if triangles.is_empty() {
                        continue;
                    }
                    (
                        Arc::new(Source::new(Arc::new(triangles), cancel)?),
                        [1., 0., 0., 1., 0., 0.],
                    )
                } else {
                    (source, d.transform)
                };
                let width =
                    (rect[2].ceil() + 1.).min(f64::from(size[0])) - (rect[0].floor() - 1.).max(0.);
                let height =
                    (rect[3].ceil() + 1.).min(f64::from(size[1])) - (rect[1].floor() - 1.).max(0.);
                covered_pixels += width.max(0.) as u64 * height.max(0.) as u64;
                if covered_pixels > MAX_COVERED_PIXELS {
                    return Err(format!(
                        "vector instance coverage budget exceeded: {covered_pixels} pixel visits, limit {MAX_COVERED_PIXELS}"
                    ));
                }
                let key = Arc::as_ptr(&source);
                let offsets = if let Some(&offsets) = source_offsets.get(&key) {
                    offsets
                } else {
                    let offsets = [scene.unique_triangles as u32, source_nodes as u32];
                    scene.unique_triangles += source.triangles.len();
                    source_nodes += source.nodes.len();
                    if scene.unique_triangles > MAX_TRIANGLES {
                        return Err(format!(
                            "vector unique geometry budget exceeded: {} triangles, limit {MAX_TRIANGLES}",
                            scene.unique_triangles
                        ));
                    }
                    scene.sources.push(source.clone());
                    source_offsets.insert(key, offsets);
                    offsets
                };
                let triangle_count = source.triangles.len();
                let node_count = source.nodes.len();
                if scene.triangles + triangle_count > MAX_INSTANCE_TRIANGLES {
                    return Err(format!(
                        "vector instance execution budget exceeded: {} triangles, limit {MAX_INSTANCE_TRIANGLES}",
                        scene.triangles + triangle_count
                    ));
                }
                let hi = transform.map(|v| v as f32);
                let lo: [f32; 6] =
                    std::array::from_fn(|i| (transform[i] - f64::from(hi[i])) as f32);
                let instance = scene.instances.len() as u32;
                scene.instances.push(Instance {
                    matrix: [hi[0], hi[1], hi[2], hi[3]],
                    matrix_low: [lo[0], lo[1], lo[2], lo[3]],
                    translation: [hi[4], hi[5], lo[4], lo[5]],
                    color: color.map(|v| v as f32),
                    source: [
                        offsets[0],
                        offsets[1],
                        triangle_count as u32,
                        node_count as u32,
                    ],
                    target: [scene.triangles as u32, scene.nodes as u32, 0, 0],
                });
                for start in (0..triangle_count + node_count).step_by(64) {
                    scene.jobs.push(Job {
                        instance,
                        start: start as u32,
                    });
                }
                scene.triangles += triangle_count;
                scene.nodes += node_count;
            }
        }
        Ok(scene)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::vector::{Cap, Join, Segment, Stroke};
    fn drawing() -> Drawing {
        Drawing {
            feather: None,
            path: Arc::new(vec![
                Segment::Move([0., 0.]),
                Segment::Line([4., 0.]),
                Segment::Line([4., 4.]),
                Segment::Line([0., 4.]),
                Segment::Close,
            ]),
            transform: [1., 0., 0., 1., 0., 0.],
            fill: Some([1.; 4]),
            stroke: None,
            even_odd: false,
        }
    }
    #[test]
    fn transforms_and_colors_reuse_local_geometry_but_shape_edits_invalidate() {
        let mut cache = Sources::default();
        let cancel = Cancel::default();
        let mut d = drawing();
        let first = cache.prepare(&[d.clone()], [32, 32], &cancel).unwrap();
        d.transform[4] = 2.;
        d.fill = Some([0.2, 0.3, 0.4, 0.5]);
        let second = cache
            .prepare(&vec![d.clone(); 100], [32, 32], &cancel)
            .unwrap();
        assert_eq!(second.sources.len(), 1);
        assert!(Arc::ptr_eq(&first.sources[0], &second.sources[0]));
        assert_eq!(second.triangles, first.triangles * 100);
        assert_eq!(second.unique_triangles, first.unique_triangles);
        d.stroke = Some(Stroke {
            color: [1.; 4],
            width: 1.,
            cap: Cap::Round,
            join: Join::Round,
        });
        let stroke = cache.prepare(&[d.clone()], [32, 32], &cancel).unwrap();
        d.stroke.as_mut().unwrap().width = 2.;
        let changed = cache.prepare(&[d.clone()], [32, 32], &cancel).unwrap();
        assert!(Arc::ptr_eq(&stroke.sources[0], &changed.sources[0]));
        assert!(!Arc::ptr_eq(&stroke.sources[1], &changed.sources[1]));
        d.even_odd = true;
        let changed_fill = cache.prepare(&[d.clone()], [32, 32], &cancel).unwrap();
        assert!(!Arc::ptr_eq(&changed.sources[0], &changed_fill.sources[0]));
        d.transform[0] = 4.;
        let scale = cache.prepare(&[d.clone()], [32, 32], &cancel).unwrap();
        assert!(!Arc::ptr_eq(&changed_fill.sources[0], &scale.sources[0]));
        Arc::make_mut(&mut d.path)[1] = Segment::Line([3., 0.]);
        let edited = cache.prepare(&[d], [32, 32], &cancel).unwrap();
        assert!(!Arc::ptr_eq(&scale.sources[0], &edited.sources[0]));
    }
    #[test]
    fn invisible_instances_and_cancellation_do_not_consume_execution_space() {
        let mut cache = Sources::default();
        let cancel = Cancel::default();
        let mut d = drawing();
        d.transform[4] = 100.;
        assert!(
            cache
                .prepare(&[d.clone()], [16, 16], &cancel)
                .unwrap()
                .instances
                .is_empty()
        );
        d.transform[4] = 0.;
        d.transform[0] = 0.;
        assert!(
            cache
                .prepare(&[d.clone()], [16, 16], &cancel)
                .unwrap()
                .instances
                .is_empty()
        );
        cancel.cancel();
        assert!(cache.prepare(&[d], [16, 16], &cancel).is_err());
    }
    #[test]
    fn hierarchy_is_bounded_and_covers_each_triangle_once() {
        let triangles = Arc::new(
            (0..100)
                .map(|x| Triangle::from([[x as f32, 0.], [x as f32 + 1., 0.], [x as f32, 1.]]))
                .collect(),
        );
        let source = Source::new(triangles, &Cancel::default()).unwrap();
        assert_eq!(source.nodes[0].links[2] as usize, source.nodes.len());
        let mut seen = vec![0; source.triangles.len()];
        for (index, node) in source.nodes.iter().enumerate() {
            assert!(node.links[2] as usize > index && node.links[2] as usize <= source.nodes.len());
            for t in node.links[0]..node.links[0] + node.links[1] {
                seen[t as usize] += 1;
                for p in source.triangles[t as usize].points {
                    assert!(
                        p[0] >= node.bounds[0]
                            && p[0] <= node.bounds[2]
                            && p[1] >= node.bounds[1]
                            && p[1] <= node.bounds[3]
                    );
                }
            }
        }
        assert!(seen.iter().all(|&v| v == 1));
    }
}

#[cfg(test)]
mod limits_tests {
    use super::*;
    use crate::vector::{Cap, Join, Segment, Stroke};
    #[test]
    fn expanded_work_is_rejected_before_gpu_allocation() {
        // Reuse one small authored curve rather than exceeding segment validation.
        let r = 6.;
        let k = r * 0.5522847498307936;
        let path = vec![
            Segment::Move([8., 2.]),
            Segment::Cubic([8. + k, 2.], [14., 8. - k], [14., 8.]),
            Segment::Cubic([14., 8. + k], [8. + k, 14.], [8., 14.]),
            Segment::Cubic([8. - k, 14.], [2., 8. + k], [2., 8.]),
            Segment::Cubic([2., 8. - k], [8. - k, 2.], [8., 2.]),
            Segment::Close,
        ];
        let d = Drawing {
            feather: None,
            path: Arc::new(path),
            transform: [1., 0., 0., 1., 0., 0.],
            fill: Some([1.; 4]),
            even_odd: false,
            stroke: Some(Stroke {
                color: [1.; 4],
                width: 0.5,
                cap: Cap::Round,
                join: Join::Round,
            }),
        };
        let error = Sources::default()
            .prepare(&vec![d; vector::MAX_DRAWINGS], [16, 16], &Cancel::default())
            .err()
            .unwrap();
        assert!(error.contains("instance execution budget"), "{error}");
    }
}
