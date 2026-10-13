//! Direct vector authoring over native objects. These operations produce guarded
//! edits; they never mutate a project, flatten artwork or render pixels.
use crate::{
    editing::{self, Edit},
    model::{Canvas, Content, Object, Point, Scalar},
};
use fold_foundation::{ObjectId, Time};
use std::collections::BTreeSet;

/// Defaults shared by artist creation menus and the graph's object templates.
pub fn new_object(kind: &str) -> Result<Object, String> {
    Ok(match kind {
        "rectangle" => rectangle("Rectangle", [0.;2], [160.,100.]),
        "ellipse" => ellipse("Ellipse", [-60.;2], [60.;2]),
        "text" => Object::new("Text", Content::Text { text: "Text".into(), font_family: "Noto Sans".into(), size: Scalar::new(64.), extensions: Default::default() }),
        "group" => Object::new("Group", Content::Group { children: vec![], extensions: Default::default() }),
        "path" => {
            let mut object = Object::new("Path", Content::Path { contours: vec![], extensions: Default::default() });
            object.paint.fill = None; object.paint.stroke = Some([0.8,0.7,1.,1.].into()); object.paint.stroke_width.value = 3.; object
        }
        "script" => Object::new("Script", Content::Script { source: "pub fn render<'a>(_input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {\n    Ok(fframes::svgr!(<g/>))\n}\n".into(), controls: Default::default(), extensions: Default::default() }),
        _ => return Err("Unknown Motion object kind".into()),
    })
}

/// Create an image instance from an already imported project asset. The caller
/// inserts it through the same guarded transaction as other native objects.
pub fn image(asset: fold_foundation::AssetId, size: [u32; 2]) -> Result<Object, String> {
    if size.contains(&0) || size.iter().any(|value| *value > 16_384) {
        return Err("Invalid Motion image dimensions".into());
    }
    let mut object = Object::new(
        "Image",
        Content::Image {
            asset,
            width: Scalar::new(size[0] as f64),
            height: Scalar::new(size[1] as f64),
            fit: crate::model::ImageFit::Contain,
            extensions: Default::default(),
        },
    );
    object.paint.fill = None;
    Ok(object)
}

/// SVG-compatible affine transform, sampled at an exact document time.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Transform(pub [f64; 6]);
impl Transform {
    pub const IDENTITY: Self = Self([1., 0., 0., 1., 0., 0.]);
    pub fn point(self, [x, y]: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, e, f] = self.0;
        [a * x + c * y + e, b * x + d * y + f]
    }
    pub fn vector(self, [x, y]: [f64; 2]) -> [f64; 2] {
        let [a, b, c, d, _, _] = self.0;
        [a * x + c * y, b * x + d * y]
    }
    /// Apply `child`, then this parent. SVG uses translate → rotate → scale
    /// in its authored list; point application proceeds in the reverse order.
    pub fn compose(self, child: Self) -> Self {
        let [a, b, c, d, e, f] = self.0;
        let [g, h, i, j, k, l] = child.0;
        Self([
            a * g + c * h,
            b * g + d * h,
            a * i + c * j,
            b * i + d * j,
            a * k + c * l + e,
            b * k + d * l + f,
        ])
    }
    pub fn inverse(self) -> Result<Self, String> {
        let [a, b, c, d, e, f] = self.0;
        let determinant = a * d - b * c;
        if determinant == 0. || !determinant.is_finite() {
            return Err("The object transform cannot be inverted".into());
        }
        let inverse = Self([
            d / determinant,
            -b / determinant,
            -c / determinant,
            a / determinant,
            (c * f - d * e) / determinant,
            (b * e - a * f) / determinant,
        ]);
        inverse.validate()?;
        Ok(inverse)
    }
    fn validate(self) -> Result<(), String> {
        if self.0.iter().all(|v| v.is_finite()) {
            Ok(())
        } else {
            Err("Motion animation produced a nonfinite transform".into())
        }
    }
}

fn local(object: &Object, time: Time) -> Result<Transform, String> {
    object.transform.affine(time).map(Transform)
}

/// Object transforms include all ancestor groups. The map is also the editor's
/// coordinate source for anchors, handles and selection outlines.
pub fn transforms(
    canvas: &Canvas,
    time: Time,
) -> Result<std::collections::BTreeMap<ObjectId, Transform>, String> {
    canvas.validate()?;
    fn visit(
        canvas: &Canvas,
        id: ObjectId,
        parent: Transform,
        time: Time,
        output: &mut std::collections::BTreeMap<ObjectId, Transform>,
    ) -> Result<(), String> {
        let object = &canvas.objects[&id];
        let transform = parent.compose(local(object, time)?);
        transform.validate()?;
        output.insert(id, transform);
        if let Content::Group { children, .. } = &object.content {
            for child in children {
                visit(canvas, *child, transform, time, output)?;
            }
        }
        Ok(())
    }
    let mut output = Default::default();
    for id in &canvas.roots {
        visit(canvas, *id, Transform::IDENTITY, time, &mut output)?;
    }
    Ok(output)
}

/// Shape tools use two canvas positions. Drawing in any direction gives the
/// same object orientation; the artist can change rotation and scale later.
pub fn rectangle(name: &str, start: [f64; 2], end: [f64; 2]) -> Object {
    let mut object = Object::new(
        name,
        Content::Rectangle {
            width: Scalar::new((end[0] - start[0]).abs()),
            height: Scalar::new((end[1] - start[1]).abs()),
            radius: Scalar::new(0.),
            extensions: Default::default(),
        },
    );
    object.transform.x.value = start[0].min(end[0]);
    object.transform.y.value = start[1].min(end[1]);
    object
}
pub fn ellipse(name: &str, start: [f64; 2], end: [f64; 2]) -> Object {
    let mut object = Object::new(
        name,
        Content::Ellipse {
            radius_x: Scalar::new((end[0] - start[0]).abs() / 2.),
            radius_y: Scalar::new((end[1] - start[1]).abs() / 2.),
            extensions: Default::default(),
        },
    );
    object.transform.x.value = start[0] / 2. + end[0] / 2.;
    object.transform.y.value = start[1] / 2. + end[1] / 2.;
    object
}

/// Updating an animated property inserts/updates the current key. Auto Key
/// also creates a curve for a static property; ordinary edits retain its base.
pub fn set_scalar(value: &mut Scalar, time: Time, sampled: f64, auto_key: bool) {
    if auto_key || value.animation.is_some() {
        value
            .animation
            .get_or_insert_with(Default::default)
            .insert(time, sampled);
    } else {
        value.value = sampled;
    }
}

/// A canvas-space translation is converted to each selected object's parent
/// space. Selected children of selected groups move once, through the group.
pub fn translate(
    canvas: &Canvas,
    selected: &[ObjectId],
    time: Time,
    delta: [f64; 2],
    auto_key: bool,
) -> Result<Vec<Edit>, String> {
    let world = transforms(canvas, time)?;
    if !delta.iter().all(|v| v.is_finite()) {
        return Err("Nonfinite translation".into());
    }
    let selected: BTreeSet<_> = selected.iter().copied().collect();
    let mut parents = std::collections::BTreeMap::new();
    for (&id, object) in &canvas.objects {
        if let Content::Group { children, .. } = &object.content {
            for child in children {
                parents.insert(*child, id);
            }
        }
    }
    let mut edits = vec![];
    for id in &selected {
        editing::editable(canvas, *id)?;
        let mut ancestor = parents.get(id);
        let mut inherited = false;
        while let Some(parent) = ancestor {
            if selected.contains(parent) {
                inherited = true;
                break;
            }
            ancestor = parents.get(parent);
        }
        if inherited {
            continue;
        }
        let parent = parents.get(id).map_or(Transform::IDENTITY, |id| world[id]);
        let offset = parent.inverse()?.vector(delta);
        let object = &canvas.objects[id];
        let mut x = object.transform.x.clone();
        let mut y = object.transform.y.clone();
        set_scalar(
            &mut x,
            time,
            object.transform.x.at(time) + offset[0],
            auto_key,
        );
        set_scalar(
            &mut y,
            time,
            object.transform.y.at(time) + offset[1],
            auto_key,
        );
        // Keep scalar guards precise: a move cannot replace an unrelated edit.
        edits.extend([
            Edit::Parameter {
                object: *id,
                expected: object.transform.x.clone(),
                value: x,
            },
            Edit::Parameter {
                object: *id,
                expected: object.transform.y.clone(),
                value: y,
            },
        ]);
    }
    editing::apply(canvas, &edits)?;
    Ok(edits)
}

#[derive(
    Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize, schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Part {
    Anchor,
    Incoming,
    Outgoing,
}

/// Set an anchor or handle from a canvas-space position. Handle edits link the
/// opposite handle unless the artist explicitly breaks the pair.
pub fn move_point(
    canvas: &Canvas,
    object: ObjectId,
    point: ObjectId,
    part: Part,
    position: [f64; 2],
    time: Time,
    linked: bool,
    auto_key: bool,
) -> Result<Edit, String> {
    editing::editable(canvas, object)?;
    let world = transforms(canvas, time)?;
    let position = world[&object].inverse()?.point(position);
    let Content::Path { contours, .. } = &canvas.objects[&object].content else {
        return Err("Object is not an editable path".into());
    };
    let expected = contours
        .iter()
        .flat_map(|c| &c.points)
        .find(|p| p.id == point)
        .ok_or("Missing path point")?;
    let mut value = expected.clone();
    let anchor = value.position_at(time)?;
    let offset = [position[0] - anchor[0], position[1] - anchor[1]];
    match part {
        Part::Anchor => set_vector(
            &mut value.position,
            &mut value.position_track,
            time,
            position,
            auto_key,
        )?,
        Part::Incoming => {
            set_vector(
                &mut value.incoming,
                &mut value.incoming_track,
                time,
                offset,
                auto_key,
            )?;
            if linked {
                set_vector(
                    &mut value.outgoing,
                    &mut value.outgoing_track,
                    time,
                    offset.map(|v| -v),
                    auto_key,
                )?;
            }
        }
        Part::Outgoing => {
            set_vector(
                &mut value.outgoing,
                &mut value.outgoing_track,
                time,
                offset,
                auto_key,
            )?;
            if linked {
                set_vector(
                    &mut value.incoming,
                    &mut value.incoming_track,
                    time,
                    offset.map(|v| -v),
                    auto_key,
                )?;
            }
        }
    }
    let edit = Edit::Point {
        object,
        expected: expected.clone(),
        value,
    };
    editing::apply(canvas, std::slice::from_ref(&edit))?;
    Ok(edit)
}

fn set_vector(
    base: &mut [f64; 2],
    track: &mut Option<fold_graph::binding::Track>,
    time: Time,
    sampled: [f64; 2],
    auto_key: bool,
) -> Result<(), String> {
    use fold_graph::{
        binding::{Interpolation, Key, Track},
        types::{Literal, Space, Unit},
    };
    if !auto_key && track.is_none() {
        *base = sampled;
        return Ok(());
    }
    let literal = Literal::Vector {
        value: sampled.to_vec(),
        unit: Unit::Canvas,
        space: Space::Screen,
    };
    if let Some(track) = track {
        if let Some(key) = track.keys.iter_mut().find(|k| k.time == time) {
            key.value = literal;
        } else {
            track.keys.push(Key {
                id: ObjectId::new(),
                time,
                value: literal,
                interpolation: Interpolation::Linear,
                handles: vec![],
                extensions: Default::default(),
            });
            track.keys.sort_by_key(|k| k.time);
        }
    } else {
        *track = Some(Track::constant(literal, time)?);
    }
    Ok(())
}
/// Insert a native anchor without changing the curve. Cubics split with de
/// Casteljau; straight segments retain zero handles and split linearly.
/// Compatible Linear, Hold, HoldNext and aligned Bézier intervals are subdivided
/// for the whole timeline. `fraction`
/// follows the sampled segment at `time` (line or cubic), as used by picking.
pub fn split_segment(
    canvas: &Canvas,
    object: ObjectId,
    contour: ObjectId,
    start: ObjectId,
    fraction: f64,
    time: Time,
) -> Result<(ObjectId, Edit), String> {
    canvas.validate()?;
    editing::editable(canvas, object)?;
    if !fraction.is_finite() || !(0. ..1.).contains(&fraction) || fraction == 0. {
        return Err("Choose a position strictly inside the segment".into());
    }
    if time < Time::ZERO || time >= canvas.duration {
        return Err("Subdivision time is outside the document".into());
    }
    let original = &canvas.objects[&object];
    let mut next = original.clone();
    let Content::Path { contours, .. } = &mut next.content else {
        return Err("Object is not an editable path".into());
    };
    let contour = contours
        .iter_mut()
        .find(|c| c.id == contour)
        .ok_or("Missing path contour")?;
    let index = contour
        .points
        .iter()
        .position(|p| p.id == start)
        .ok_or("Missing segment start")?;
    let end = if index + 1 < contour.points.len() {
        index + 1
    } else if contour.closed && contour.points.len() > 1 {
        0
    } else {
        return Err("Point has no following segment".into());
    };
    let a = contour.points[index].clone();
    let b = contour.points[end].clone();
    // SVG emits a line when both sampled handles collapse. Picking supplies a
    // linear fraction there; animated handle tracks require cubic subdivision.
    let fraction = if (a.outgoing_track.is_some() || b.incoming_track.is_some())
        && a.outgoing_at(time)? == [0.; 2]
        && b.incoming_at(time)? == [0.; 2]
    {
        let (mut low, mut high) = (0_f64, 1_f64);
        for _ in 0..64 {
            let middle = (low + high) / 2.;
            if middle * middle * (3. - 2. * middle) < fraction {
                low = middle;
            } else {
                high = middle;
            }
        }
        (low + high) / 2.
    } else {
        fraction
    };
    use fold_graph::binding::{Interpolation, Key, Track};
    use fold_graph::types::{Literal, Space, Unit};
    let tracks = [
        &a.position_track,
        &a.outgoing_track,
        &b.position_track,
        &b.incoming_track,
    ];
    let mut times = BTreeSet::new();
    for track in tracks.iter().copied().flatten() {
        times.extend(track.keys.iter().map(|key| key.time));
    }
    if times.len() > 4096 {
        return Err("Animated subdivision exceeds the native track key limit".into());
    }
    let times = times.into_iter().collect::<Vec<_>>();
    let mut interpolations = Vec::with_capacity(times.len());
    for interval in times.windows(2) {
        let mut mode = None;
        for track in tracks.iter().copied().flatten() {
            let Some(index) = track
                .keys
                .partition_point(|key| key.time <= interval[0])
                .checked_sub(1)
            else {
                continue;
            };
            let key = &track.keys[index];
            let Some(next) = track.keys.get(index + 1) else {
                continue;
            };
            if (key.value == next.value && key.interpolation != Interpolation::Bezier)
                || (key.interpolation == Interpolation::Hold && next.time != interval[1])
                || (key.interpolation == Interpolation::HoldNext && key.time != interval[0])
            {
                continue;
            }
            let source = key.interpolation;
            if mode.is_some_and(|mode| mode != source) {
                return Err(
                    "Subdivision cannot combine different moving interpolation modes in one interval"
                        .into(),
                );
            }
            mode = Some(source);
        }
        interpolations.push(mode.unwrap_or(Interpolation::Linear));
    }
    // The final key has no outgoing interval.
    interpolations.push(Interpolation::Linear);
    let straight = a.outgoing == [0.; 2]
        && b.incoming == [0.; 2]
        && a.outgoing_track.is_none()
        && b.incoming_track.is_none();
    let base = split_vectors(
        a.position, a.outgoing, b.position, b.incoming, fraction, straight,
    );
    let id = ObjectId::new();
    let mut point = Point {
        id,
        position: base[1],
        incoming: base[2],
        outgoing: base[3],
        position_track: None,
        incoming_track: None,
        outgoing_track: None,
        extensions: Default::default(),
    };
    contour.points[index].outgoing = base[0];
    contour.points[end].incoming = base[4];
    scale_vector_track(&mut contour.points[index].outgoing_track, fraction)?;
    scale_vector_track(&mut contour.points[end].incoming_track, 1. - fraction)?;
    if !times.is_empty() {
        let handles = split_temporal_handles(&tracks, &times, &interpolations, fraction, straight)?;
        let poses = times
            .into_iter()
            .map(|time| {
                Ok((
                    time,
                    split_vectors(
                        a.position_at(time)?,
                        a.outgoing_at(time)?,
                        b.position_at(time)?,
                        b.incoming_at(time)?,
                        fraction,
                        straight,
                    ),
                ))
            })
            .collect::<Result<Vec<_>, String>>()?;
        let track = |component: usize| -> Result<Track, String> {
            let literal = |value: [f64; 2]| Literal::Vector {
                value: value.to_vec(),
                unit: Unit::Canvas,
                space: Space::Screen,
            };
            let mut track = Track::constant(literal(poses[0].1[component]), poses[0].0)?;
            track.keys[0].interpolation = interpolations[0];
            track.keys[0].handles = handles
                .first()
                .map_or_else(Vec::new, |key| key[component - 1].to_vec());
            for (index, (time, pose)) in poses.iter().enumerate().skip(1) {
                track.keys.push(Key {
                    id: ObjectId::new(),
                    time: *time,
                    value: literal(pose[component]),
                    interpolation: interpolations[index],
                    handles: handles
                        .get(index)
                        .map_or_else(Vec::new, |key| key[component - 1].to_vec()),
                    extensions: Default::default(),
                });
            }
            track.validate()?;
            Ok(track)
        };
        point.position_track = Some(track(1)?);
        if !straight {
            point.incoming_track = Some(track(2)?);
            point.outgoing_track = Some(track(3)?);
        }
    }
    contour.points.insert(index + 1, point);
    let edit = Edit::Replace {
        id: object,
        expected: Box::new(original.clone()),
        object: Box::new(next),
    };
    editing::apply(canvas, std::slice::from_ref(&edit))?;
    Ok((id, edit))
}

// Spatial subdivision is linear in the four moving vectors. Apply the same
// operation to their temporal handle offsets. Matching time controls keep the
// resulting tracks exact, including overshoot and resolved Auto tangents.
fn split_temporal_handles(
    tracks: &[&Option<fold_graph::binding::Track>; 4],
    times: &[Time],
    modes: &[fold_graph::binding::Interpolation],
    fraction: f64,
    straight: bool,
) -> Result<Vec<[[fold_graph::binding::Handles; 2]; 3]>, String> {
    use fold_animation::{Handle, Tangents};
    use fold_graph::binding::{Handles, Interpolation};
    if !modes.contains(&Interpolation::Bezier) {
        return Ok(Vec::new());
    }
    let mut result = vec![
        [[Handles {
            tangents: Tangents::Broken,
            ..Default::default()
        }; 2]; 3];
        times.len()
    ];
    let curves = tracks
        .iter()
        .map(|track| track.as_ref().map(fold_graph::curves::curves).transpose())
        .collect::<Result<Vec<_>, _>>()?;
    for (interval, pair) in times.windows(2).enumerate() {
        if modes[interval] != Interpolation::Bezier {
            continue;
        }
        let mut outgoing = [[0.; 2]; 4];
        let mut incoming = [[0.; 2]; 4];
        let mut fractions = [None; 2];
        for (source, curves) in curves.iter().enumerate() {
            let Some(curves) = curves else {
                continue;
            };
            for (axis, (_, curve)) in curves.iter().enumerate() {
                let Some(index) = curve
                    .keys
                    .partition_point(|key| key.time <= pair[0])
                    .checked_sub(1)
                else {
                    continue;
                };
                let key = &curve.keys[index];
                let Some(next) = curve.keys.get(index + 1) else {
                    continue;
                };
                if key.interpolation != fold_animation::Interpolation::Bezier {
                    // Mode admission above permits only constant non-Bézier
                    // sources in a moving Bézier interval.
                    continue;
                }
                if key.time != pair[0] || next.time != pair[1] {
                    return Err("Bézier subdivision requires aligned source key times".into());
                }
                let out = curve.handle(index, false);
                let inc = curve.handle(index + 1, true);
                let timing = [out.fraction, inc.fraction];
                if fractions[axis].is_some_and(|expected| expected != timing) {
                    return Err(
                        "Bézier subdivision requires matching time-handle fractions per component"
                            .into(),
                    );
                }
                fractions[axis] = Some(timing);
                outgoing[source][axis] = out.value;
                incoming[source][axis] = inc.value;
            }
        }
        let outgoing = split_vectors(
            outgoing[0],
            outgoing[1],
            outgoing[2],
            outgoing[3],
            fraction,
            straight,
        );
        let incoming = split_vectors(
            incoming[0],
            incoming[1],
            incoming[2],
            incoming[3],
            fraction,
            straight,
        );
        for (axis, timing) in fractions.into_iter().enumerate() {
            let timing = timing.ok_or("Missing Bézier subdivision timing")?;
            for component in 0..3 {
                result[interval][component][axis].outgoing = Handle {
                    fraction: timing[0],
                    value: outgoing[component + 1][axis],
                };
                result[interval + 1][component][axis].incoming = Handle {
                    fraction: timing[1],
                    value: incoming[component + 1][axis],
                };
            }
        }
    }
    Ok(result)
}

// De Casteljau is affine in the moving control vectors. The union of linear
// source intervals therefore gives an exact piecewise-linear derived track.
// Hold and HoldNext sources preserve their endpoint jumps on the same union.
fn split_vectors(
    a: [f64; 2],
    outgoing: [f64; 2],
    b: [f64; 2],
    incoming: [f64; 2],
    fraction: f64,
    straight: bool,
) -> [[f64; 2]; 5] {
    let mix = |a: [f64; 2], b: [f64; 2]| {
        std::array::from_fn(|i| a[i] * (1. - fraction) + b[i] * fraction)
    };
    let add = |a: [f64; 2], b: [f64; 2]| std::array::from_fn(|i| a[i] + b[i]);
    let sub = |a: [f64; 2], b: [f64; 2]| std::array::from_fn(|i| a[i] - b[i]);
    if straight {
        return [[0.; 2], mix(a, b), [0.; 2], [0.; 2], [0.; 2]];
    }
    let p1 = add(a, outgoing);
    let p2 = add(b, incoming);
    let l1 = mix(a, p1);
    let middle = mix(p1, p2);
    let r2 = mix(p2, b);
    let l2 = mix(l1, middle);
    let r1 = mix(middle, r2);
    let point = mix(l2, r1);
    [
        sub(l1, a),
        point,
        sub(l2, point),
        sub(r1, point),
        sub(r2, b),
    ]
}
fn scale_vector_track(
    track: &mut Option<fold_graph::binding::Track>,
    factor: f64,
) -> Result<(), String> {
    if let Some(track) = track {
        for key in &mut track.keys {
            let fold_graph::types::Literal::Vector { value, .. } = &mut key.value else {
                return Err("Path offset track requires a vector".into());
            };
            for value in value {
                *value *= factor;
            }
            for handles in &mut key.handles {
                handles.incoming.value *= factor;
                handles.outgoing.value *= factor;
            }
        }
        track.validate()?;
    }
    Ok(())
}

/// Change a closed contour's traversal origin without changing its geometry.
pub fn first_anchor(
    canvas: &Canvas,
    object: ObjectId,
    point: ObjectId,
) -> Result<Vec<crate::editing::Edit>, String> {
    crate::editing::editable(canvas, object)?;
    let expected = canvas.objects.get(&object).ok_or("Missing Motion object")?;
    let mut value = expected.clone();
    let Content::Path { contours, .. } = &mut value.content else {
        return Err("Object is not an editable path".into());
    };
    let (contour, index) = contours
        .iter_mut()
        .find_map(|contour| {
            contour
                .points
                .iter()
                .position(|p| p.id == point)
                .map(|index| (contour, index))
        })
        .ok_or("Missing path anchor")?;
    if !contour.closed {
        return Err("Only closed contours have a movable first anchor".into());
    }
    if index == 0 {
        return Ok(vec![]);
    }
    contour.points.rotate_left(index);
    let edits = vec![crate::editing::Edit::Replace {
        id: object,
        expected: Box::new(expected.clone()),
        object: Box::new(value),
    }];
    crate::editing::apply(canvas, &edits)?;
    Ok(edits)
}

/// Remove one explicitly targeted anchor and its owned tracks. Keep the path,
/// contour and every other point intact, including an empty final contour.
pub fn remove_point(canvas: &Canvas, object: ObjectId, point: ObjectId) -> Result<Edit, String> {
    canvas.validate()?;
    editing::editable(canvas, object)?;
    let expected = &canvas.objects[&object];
    let mut next = expected.clone();
    let Content::Path { contours, .. } = &mut next.content else {
        return Err("Object is not an editable path".into());
    };
    let (contour, index) = contours
        .iter_mut()
        .find_map(|contour| {
            contour
                .points
                .iter()
                .position(|p| p.id == point)
                .map(|index| (contour, index))
        })
        .ok_or("Missing path anchor")?;
    contour.points.remove(index);
    let edit = Edit::Replace {
        id: object,
        expected: Box::new(expected.clone()),
        object: Box::new(next),
    };
    editing::apply(canvas, std::slice::from_ref(&edit))?;
    Ok(edit)
}
