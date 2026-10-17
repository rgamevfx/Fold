//! Shared design geometry and guarded operations for canvas tools and agents.
use crate::{
    editing::{self, Edit},
    hierarchy,
    model::*,
    tools::{self, Transform},
};
use fold_foundation::{ObjectId, Time};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Bounds {
    pub min: [f64; 2],
    pub max: [f64; 2],
}
impl Bounds {
    pub fn new(a: [f64; 2], b: [f64; 2]) -> Self {
        Self {
            min: [a[0].min(b[0]), a[1].min(b[1])],
            max: [a[0].max(b[0]), a[1].max(b[1])],
        }
    }
    pub fn center(self) -> [f64; 2] {
        [0, 1].map(|i| (self.min[i] + self.max[i]) * 0.5)
    }
    pub fn size(self) -> [f64; 2] {
        [0, 1].map(|i| self.max[i] - self.min[i])
    }
    pub fn corners(self) -> [[f64; 2]; 4] {
        [
            self.min,
            [self.max[0], self.min[1]],
            self.max,
            [self.min[0], self.max[1]],
        ]
    }
    pub fn contains(self, p: [f64; 2]) -> bool {
        (0..2).all(|i| p[i] >= self.min[i] && p[i] <= self.max[i])
    }
    pub fn intersects(self, other: Self) -> bool {
        (0..2).all(|i| self.min[i] <= other.max[i] && self.max[i] >= other.min[i])
    }
    pub fn union(self, other: Self) -> Self {
        Self {
            min: [0, 1].map(|i| self.min[i].min(other.min[i])),
            max: [0, 1].map(|i| self.max[i].max(other.max[i])),
        }
    }
    pub fn transformed(self, transform: Transform) -> Self {
        points_bounds(self.corners().map(|p| transform.point(p))).expect("four corners")
    }
}
fn points_bounds(points: impl IntoIterator<Item = [f64; 2]>) -> Option<Bounds> {
    points
        .into_iter()
        .map(|p| Bounds::new(p, p))
        .reduce(Bounds::union)
}

/// Analytic bounds of cubic segments, including internal extrema.
fn cubic_bounds(p: [[f64; 2]; 4]) -> Bounds {
    let mut points = vec![p[0], p[3]];
    for axis in 0..2 {
        let [a, b, c, d] = p.map(|p| p[axis]);
        let qa = -a + 3. * b - 3. * c + d;
        let qb = 2. * (a - 2. * b + c);
        let qc = b - a;
        let mut roots = vec![];
        if qa.abs() < 1e-12 {
            if qb.abs() >= 1e-12 {
                roots.push(-qc / qb);
            }
        } else {
            let disc = qb * qb - 4. * qa * qc;
            if disc >= 0. {
                roots.extend([
                    (-qb - disc.sqrt()) / (2. * qa),
                    (-qb + disc.sqrt()) / (2. * qa),
                ]);
            }
        }
        for t in roots.into_iter().filter(|t| *t > 0. && *t < 1.) {
            let u = 1. - t;
            points.push([0, 1].map(|i| {
                u * u * u * p[0][i]
                    + 3. * u * u * t * p[1][i]
                    + 3. * u * t * t * p[2][i]
                    + t * t * t * p[3][i]
            }));
        }
    }
    points_bounds(points).expect("endpoints")
}

pub fn local_bounds(object: &Object, time: Time) -> Option<Bounds> {
    match &object.content {
        Content::Rectangle { width, height, .. } | Content::Image { width, height, .. } => {
            Some(Bounds::new([0.; 2], [width.at(time), height.at(time)]))
        }
        Content::Ellipse {
            radius_x, radius_y, ..
        } => {
            let r = [radius_x.at(time), radius_y.at(time)];
            Some(Bounds::new(r.map(|x| -x), r))
        }
        Content::Text {
            text, size, layout, ..
        } => {
            let lines = crate::typography::layout(text, size.at(time), layout);
            Some(Bounds::new(
                [0.; 2],
                [
                    layout
                        .width
                        .max(lines.iter().map(|l| l.width).fold(0., f64::max)),
                    (lines.len().max(1) as f64) * size.at(time) * layout.line_height,
                ],
            ))
        }
        Content::Path { contours, .. } => {
            let mut bounds = None;
            for contour in contours {
                let sampled = contour.at(time).ok()?;
                let n = sampled.points.len();
                for (i, a) in sampled.points.iter().enumerate() {
                    let next = sampled.points.get(i + 1).or_else(|| {
                        if sampled.closed {
                            sampled.points.first()
                        } else {
                            None
                        }
                    });
                    let b = if let Some(b) = next {
                        cubic_bounds([
                            a.position,
                            [0, 1].map(|i| a.position[i] + a.outgoing[i]),
                            [0, 1].map(|i| b.position[i] + b.incoming[i]),
                            b.position,
                        ])
                    } else {
                        Bounds::new(a.position, a.position)
                    };
                    bounds = Some(bounds.map_or(b, |v: Bounds| v.union(b)));
                }
                if n == 0 {
                    continue;
                }
            }
            bounds
        }
        _ => None,
    }
}

pub fn bounds(canvas: &Canvas, time: Time) -> Result<BTreeMap<ObjectId, Bounds>, String> {
    let transforms = tools::transforms(canvas, time)?;
    fn visit(
        canvas: &Canvas,
        id: ObjectId,
        time: Time,
        transforms: &BTreeMap<ObjectId, Transform>,
        out: &mut BTreeMap<ObjectId, Bounds>,
    ) -> Option<Bounds> {
        let object = &canvas.objects[&id];
        let b = if let Content::Group { children, .. } = &object.content {
            children
                .iter()
                .filter_map(|id| visit(canvas, *id, time, transforms, out))
                .reduce(Bounds::union)
        } else {
            local_bounds(object, time)
                .or_else(|| {
                    matches!(object.content, Content::Script { .. }).then_some(Bounds {
                        min: [0., 0.],
                        max: [canvas.width as f64, canvas.height as f64],
                    })
                })
                .map(|b| b.transformed(transforms[&id]))
        };
        if let Some(b) = b {
            out.insert(id, b);
        }
        b
    }
    let mut out = BTreeMap::new();
    for id in &canvas.roots {
        visit(canvas, *id, time, &transforms, &mut out);
    }
    Ok(out)
}
pub fn selection_bounds(
    canvas: &Canvas,
    selected: &[ObjectId],
    time: Time,
) -> Result<Option<Bounds>, String> {
    let all = bounds(canvas, time)?;
    Ok(selected
        .iter()
        .filter_map(|id| all.get(id).copied())
        .reduce(Bounds::union))
}
pub fn selection_roots(canvas: &Canvas, selected: &[ObjectId]) -> Result<Vec<ObjectId>, String> {
    let selected: BTreeSet<_> = selected.iter().copied().collect();
    let mut result = vec![];
    for id in &selected {
        let mut parent = hierarchy::placement(canvas, *id)?.0;
        let mut inherited = false;
        while let Some(p) = parent {
            if selected.contains(&p) {
                inherited = true;
                break;
            }
            parent = hierarchy::placement(canvas, p)?.0;
        }
        if !inherited {
            result.push(*id);
        }
    }
    Ok(result)
}

/// Apply a canvas-space affine gesture about its chosen pivot. Decomposition
/// preserves the existing parameter IDs and writes keys using the shared rules.
pub fn transform(
    canvas: &Canvas,
    selected: &[ObjectId],
    time: Time,
    matrix: Transform,
    auto_key: bool,
) -> Result<Vec<Edit>, String> {
    let world = tools::transforms(canvas, time)?;
    let mut edits = vec![];
    for id in selection_roots(canvas, selected)? {
        editing::editable(canvas, id)?;
        let parent = hierarchy::placement(canvas, id)?
            .0
            .map_or(Transform::IDENTITY, |p| world[&p]);
        let local = parent.inverse()?.compose(matrix).compose(world[&id]);
        let mut sampled = crate::model::Transform::from_affine(local.0)?;
        let anchor = [
            canvas.objects[&id].transform.anchor_x.at(time),
            canvas.objects[&id].transform.anchor_y.at(time),
        ];
        let offset = local.vector(anchor);
        sampled.x.value += offset[0] - anchor[0];
        sampled.y.value += offset[1] - anchor[1];
        if let Some(motion) = &canvas.objects[&id].motion {
            let pose = motion.pose(time);
            if pose.scale.iter().any(|v| v.abs() < 1e-10) {
                return Err(
                    "Edit the design at a time with nonzero motion scale, or disable its motion"
                        .into(),
                );
            }
            sampled.x.value -= pose.position[0];
            sampled.y.value -= pose.position[1];
            sampled.rotation.value -= pose.rotation;
            sampled.scale_x.value /= pose.scale[0];
            sampled.scale_y.value /= pose.scale[1];
        }
        let before = &canvas.objects[&id];
        let mut after = before.clone();
        for (target, value) in [
            (&mut after.transform.x, sampled.x.value),
            (&mut after.transform.y, sampled.y.value),
            (&mut after.transform.rotation, sampled.rotation.value),
            (&mut after.transform.scale_x, sampled.scale_x.value),
            (&mut after.transform.scale_y, sampled.scale_y.value),
            (&mut after.transform.shear, sampled.shear.value),
        ] {
            if (target.at(time) - value).abs() > 1e-10 {
                tools::set_scalar(target, time, value, auto_key);
            }
        }
        edits.push(Edit::Replace {
            id,
            expected: Box::new(before.clone()),
            object: Box::new(after),
        });
    }
    editing::apply(canvas, &edits)?;
    Ok(edits)
}
#[derive(Clone, Copy, Debug, serde::Serialize, serde::Deserialize, schemars::JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Align {
    Left,
    Center,
    Right,
    Top,
    Middle,
    Bottom,
    DistributeHorizontal,
    DistributeVertical,
}
pub fn align(
    canvas: &Canvas,
    selected: &[ObjectId],
    time: Time,
    mode: Align,
    auto_key: bool,
) -> Result<Vec<Edit>, String> {
    let mut ids = selection_roots(canvas, selected)?;
    let all = bounds(canvas, time)?;
    if ids.is_empty() || ids.iter().any(|id| !all.contains_key(id)) {
        return Err("Select artwork with editable bounds".into());
    }
    let axis = if matches!(
        mode,
        Align::Top | Align::Middle | Align::Bottom | Align::DistributeVertical
    ) {
        1
    } else {
        0
    };
    let distribution = matches!(
        mode,
        Align::DistributeHorizontal | Align::DistributeVertical
    );
    if distribution && ids.len() < 3 {
        return Err("Select at least three objects to distribute".into());
    }
    let target = if ids.len() == 1 {
        Bounds::new([0.; 2], [canvas.width as f64, canvas.height as f64])
    } else {
        ids.iter().map(|id| all[id]).reduce(Bounds::union).unwrap()
    };
    ids.sort_by(|a, b| all[a].center()[axis].total_cmp(&all[b].center()[axis]));
    let centers = [
        all[&ids[0]].center()[axis],
        all[ids.last().unwrap()].center()[axis],
    ];
    let mut edits = vec![];
    for (index, id) in ids.iter().enumerate() {
        let b = all[id];
        let delta = if distribution {
            centers[0] + (centers[1] - centers[0]) * index as f64 / (ids.len() - 1) as f64
                - b.center()[axis]
        } else if matches!(mode, Align::Left | Align::Top) {
            target.min[axis] - b.min[axis]
        } else if matches!(mode, Align::Right | Align::Bottom) {
            target.max[axis] - b.max[axis]
        } else {
            target.center()[axis] - b.center()[axis]
        };
        let mut offset = [0.; 2];
        offset[axis] = delta;
        edits.extend(tools::translate(canvas, &[*id], time, offset, auto_key)?);
    }
    editing::apply(canvas, &edits)?;
    Ok(edits)
}

/// Move the native pivot while preserving the current world-space artwork.
pub fn anchor(
    canvas: &Canvas,
    id: ObjectId,
    time: Time,
    position: [f64; 2],
    auto_key: bool,
) -> Result<Edit, String> {
    editing::editable(canvas, id)?;
    let world = tools::transforms(canvas, time)?;
    let point = world[&id].inverse()?.point(position);
    let before = &canvas.objects[&id];
    let mut after = before.clone();
    let delta = [
        point[0] - before.transform.anchor_x.at(time),
        point[1] - before.transform.anchor_y.at(time),
    ];
    let linear = Transform(before.affine(time)?).vector(delta);
    for (scalar, value) in [
        (&mut after.transform.anchor_x, point[0]),
        (&mut after.transform.anchor_y, point[1]),
        (
            &mut after.transform.x,
            before.transform.x.at(time) + linear[0] - delta[0],
        ),
        (
            &mut after.transform.y,
            before.transform.y.at(time) + linear[1] - delta[1],
        ),
    ] {
        tools::set_scalar(scalar, time, value, auto_key);
    }
    Ok(Edit::Replace {
        id,
        expected: Box::new(before.clone()),
        object: Box::new(after),
    })
}
