//! Bounded asynchronous picking geometry. Text shaping and graph evaluation never
//! run on the UI thread; stale results cannot select a different revision/time.
use crate::{
    Motion,
    evaluation::Evaluator,
    geometry::{Content, Geometry, IDENTITY, multiply},
};
use fold_foundation::{ObjectId, Time};
use fold_render::vector::Segment;
use std::sync::mpsc::{self, Receiver, SyncSender};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) struct Key {
    pub revision: u64,
    pub time: Time,
    pub scope: Option<ObjectId>,
    pub selected: Option<ObjectId>,
}
pub(super) struct Outline {
    pub object: ObjectId,
    pub contours: Vec<Vec<[f64; 2]>>,
    pub bounds: [f64; 4],
    fills: Vec<(std::ops::Range<usize>, bool)>,
    pub hidden: bool,
    pub parent: [f64; 6],
}
pub(super) struct ResultGeometry {
    pub key: Key,
    pub outlines: Vec<Outline>,
}
struct Request {
    key: Key,
    motion: Motion,
}
pub(super) struct Picking {
    send: SyncSender<Request>,
    receive: Receiver<Result<ResultGeometry, String>>,
    pending: bool,
    requested: Option<Key>,
    pub result: Option<ResultGeometry>,
    pub error: Option<String>,
}
impl Default for Picking {
    fn default() -> Self {
        let (send, rx) = mpsc::sync_channel::<Request>(1);
        let (tx, receive) = mpsc::sync_channel(1);
        std::thread::Builder::new()
            .name("motion-picking".into())
            .spawn(move || {
                while let Ok(job) = rx.recv() {
                    let result = build(&job.motion, job.key);
                    if tx.send(result).is_err() {
                        break;
                    }
                }
            })
            .expect("start motion picking worker");
        Self {
            send,
            receive,
            pending: false,
            requested: None,
            result: None,
            error: None,
        }
    }
}
impl Picking {
    pub fn update(&mut self, m: &Motion, key: Key) {
        while let Ok(result) = self.receive.try_recv() {
            self.pending = false;
            match result {
                Ok(result) => {
                    self.result = Some(result);
                    self.error = None
                }
                Err(e) => self.error = Some(e),
            }
        }
        if self.requested != Some(key) {
            self.error = None;
        }
        if !self.pending
            && self.result.as_ref().is_none_or(|r| r.key != key)
            && self.error.is_none()
        {
            self.requested = Some(key);
            self.pending = self
                .send
                .try_send(Request {
                    key,
                    motion: m.clone(),
                })
                .is_ok();
        }
    }
    pub fn current(&self, key: Key) -> Option<&ResultGeometry> {
        self.result.as_ref().filter(|r| r.key == key)
    }
}
fn transform(m: [f64; 6], p: [f64; 2]) -> [f64; 2] {
    [
        m[0] * p[0] + m[2] * p[1] + m[4],
        m[1] * p[0] + m[3] * p[1] + m[5],
    ]
}
fn build(m: &Motion, key: Key) -> Result<ResultGeometry, String> {
    let mut evaluator = Evaluator::new(m, key.time)?;
    let mut outlines = vec![];
    let mut budget = 150_000usize;
    if let Some(scene) = &m.scene {
        for o in &scene.objects {
            if o.parent != key.scope && Some(o.id) != key.selected {
                continue;
            }
            if (!o.visible && Some(o.id) != key.selected) || key.time < o.start || key.time >= o.end
            {
                continue;
            }
            let content = evaluator.world_content(o.id)?;
            let parent = multiply(
                if let Some(id) = o.parent {
                    evaluator.world_matrix(id)?
                } else {
                    IDENTITY
                },
                o.offset,
            );
            let mut contours = vec![];
            let mut fills = vec![];
            walk(
                &content,
                IDENTITY,
                &mut contours,
                &mut fills,
                &mut budget,
                0,
            )?;
            let mut bounds = [
                f64::INFINITY,
                f64::INFINITY,
                f64::NEG_INFINITY,
                f64::NEG_INFINITY,
            ];
            for p in contours.iter().flatten() {
                bounds[0] = bounds[0].min(p[0]);
                bounds[1] = bounds[1].min(p[1]);
                bounds[2] = bounds[2].max(p[0]);
                bounds[3] = bounds[3].max(p[1]);
            }
            if bounds.iter().all(|v| v.is_finite()) {
                outlines.push(Outline {
                    object: o.id,
                    contours,
                    bounds,
                    fills,
                    hidden: !o.visible,
                    parent,
                });
            }
        }
    }
    Ok(ResultGeometry { key, outlines })
}
fn walk(
    content: &Content,
    parent: [f64; 6],
    out: &mut Vec<Vec<[f64; 2]>>,
    fills: &mut Vec<(std::ops::Range<usize>, bool)>,
    budget: &mut usize,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Picking geometry exceeds nesting limit".into());
    }
    for e in content.iter() {
        let m = multiply(parent, e.transform);
        match &e.geometry {
            Geometry::Group(c) | Geometry::Instance(c) => {
                walk(c, m, out, fills, budget, depth + 1)?
            }
            Geometry::Path(path) | Geometry::Glyph { path, .. } => {
                let start = out.len();
                let mut points = vec![];
                let mut current = [0.; 2];
                let mut first = [0.; 2];
                for s in path.iter() {
                    let mut next = vec![];
                    match *s {
                        Segment::Move(p) => {
                            if !points.is_empty() {
                                out.push(std::mem::take(&mut points));
                            }
                            current = p;
                            first = p;
                            next.push(p);
                        }
                        Segment::Line(p) => {
                            next.push(p);
                            current = p;
                        }
                        Segment::Close => {
                            next.push(first);
                            current = first;
                        }
                        Segment::Quad(a, b) => {
                            for i in 1..=16 {
                                let t = i as f64 / 16.;
                                let u = 1. - t;
                                next.push(std::array::from_fn(|j| {
                                    u * u * current[j] + 2. * u * t * a[j] + t * t * b[j]
                                }));
                            }
                            current = b;
                        }
                        Segment::Cubic(a, b, c) => {
                            for i in 1..=24 {
                                let t = i as f64 / 24.;
                                let u = 1. - t;
                                next.push(std::array::from_fn(|j| {
                                    u * u * u * current[j]
                                        + 3. * u * u * t * a[j]
                                        + 3. * u * t * t * b[j]
                                        + t * t * t * c[j]
                                }));
                            }
                            current = c;
                        }
                    }
                    *budget = budget
                        .checked_sub(next.len())
                        .ok_or("Picking geometry exceeds point budget")?;
                    points.extend(next.into_iter().map(|p| transform(m, p)));
                }
                if !points.is_empty() {
                    out.push(points);
                }
                if e.fill.is_some_and(|c| c[3] > 0.) && e.opacity > 0. {
                    fills.push((start..out.len(), e.even_odd));
                }
            }
            Geometry::Point => {}
        }
    }
    Ok(())
}
impl Outline {
    pub fn hit(&self, p: [f64; 2], tolerance: f64) -> bool {
        if p[0] < self.bounds[0] - tolerance
            || p[0] > self.bounds[2] + tolerance
            || p[1] < self.bounds[1] - tolerance
            || p[1] > self.bounds[3] + tolerance
        {
            return false;
        }
        let mut winding = vec![0i32; self.contours.len()];
        for (index, points) in self.contours.iter().enumerate() {
            for pair in points.windows(2) {
                let a = pair[0];
                let b = pair[1];
                let d = [b[0] - a[0], b[1] - a[1]];
                let t = (((p[0] - a[0]) * d[0] + (p[1] - a[1]) * d[1])
                    / (d[0] * d[0] + d[1] * d[1]).max(1e-12))
                .clamp(0., 1.);
                if (p[0] - a[0] - t * d[0]).hypot(p[1] - a[1] - t * d[1]) <= tolerance {
                    return true;
                }
                if (a[1] > p[1]) != (b[1] > p[1])
                    && p[0] < (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0]
                {
                    winding[index] += if b[1] > a[1] { 1 } else { -1 };
                }
            }
        }
        self.fills.iter().any(|(range, even_odd)| {
            let count: i32 = winding[range.clone()].iter().sum();
            if *even_odd {
                count.abs() % 2 == 1
            } else {
                count != 0
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn stroke_does_not_capture_empty_interior_and_fill_respects_holes() {
        let square = |a: f64, b: f64| vec![[a, a], [b, a], [b, b], [a, b], [a, a]];
        let mut outline = Outline {
            object: ObjectId::new(),
            contours: vec![square(0., 100.), square(25., 75.)],
            bounds: [0., 0., 100., 100.],
            fills: vec![],
            hidden: false,
            parent: IDENTITY,
        };
        assert!(!outline.hit([50., 50.], 2.));
        assert!(outline.hit([1., 50.], 2.));
        outline.fills.push((0..2, true));
        assert!(!outline.hit([50., 50.], 2.));
        assert!(outline.hit([10., 50.], 2.));
    }
}
