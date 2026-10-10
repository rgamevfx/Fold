//! Frozen drag input and pure shape transforms.
use super::transform as tx;
use crate::roto::{Layer, Roto, add, map, mul, sub};
use fold_foundation::{ObjectId, Time};
use std::collections::BTreeSet;
#[derive(Clone, Copy)]
pub(super) enum Action {
    Move,
    Handle(ObjectId, bool),
    Feather(ObjectId),
    Scale(usize),
    Pin(usize),
    Rotate,
    Marquee,
    Create(bool),
}
pub(super) struct Gesture {
    pub before: Roto,
    pub node: crate::Node,
    pub time: Time,
    pub generation: u64,
    pub start: [f64; 2],
    pub action: Action,
    pub layer: ObjectId,
    pub points: BTreeSet<ObjectId>,
    pub bounds: Option<[f64; 4]>,
    pub created: Option<Layer>,
}

pub(super) fn gesture_roto(
    g: &Gesture,
    p: [f64; 2],
    shift: bool,
    alt: bool,
) -> Result<Roto, String> {
    let mut result = g.before.clone();
    if let Action::Create(ellipse) = g.action {
        if result.root.locked {
            return Err("Roto root is locked".into());
        }
        let mut end = p;
        if shift {
            let d = sub(p, g.start);
            let size = d[0].abs().max(d[1].abs());
            end = add(g.start, [size * d[0].signum(), size * d[1].signum()]);
        }
        let mut layer = Layer::rectangle(g.start, end, ellipse);
        let original = g.created.as_ref().unwrap();
        layer.id = original.id;
        for (p, base) in layer
            .shape
            .as_mut()
            .unwrap()
            .points
            .iter_mut()
            .zip(&original.shape.as_ref().unwrap().points)
        {
            p.id = base.id;
        }
        insert_layer(&mut result, layer, Some(g.layer))?;
        return Ok(result);
    }
    if !result.root.editable(g.layer) {
        return Err("Roto layer is locked".into());
    }
    let matrix = result
        .transform_for(g.layer)
        .ok_or("Missing layer transform")?;
    let inverse = matrix.inverse()?;
    let shape = result
        .root
        .find_mut(g.layer)
        .and_then(|l| l.shape.as_mut())
        .ok_or("Select a shape")?;
    let feathers = shape.feather_positions();
    for (i, point) in shape.points.iter_mut().enumerate() {
        match g.action {
            Action::Handle(id, incoming) if point.id == id => {
                let h = sub(map(inverse, p), point.position);
                if incoming {
                    point.incoming = h;
                } else {
                    point.outgoing = h;
                }
                if alt {
                    point.linked = false;
                }
                if point.linked {
                    if incoming {
                        point.outgoing = mul(h, -1.);
                    } else {
                        point.incoming = mul(h, -1.);
                    }
                }
            }
            Action::Feather(id) if point.id == id => {
                point.feather = add(point.feather, sub(map(inverse, p), feathers[i]))
            }
            Action::Move | Action::Scale(_) | Action::Pin(_) | Action::Rotate
                if g.points.contains(&point.id) =>
            {
                tx::point(point, |q| {
                    let q = map(matrix, q);
                    let q = match g.action {
                        Action::Move => add(q, sub(p, g.start)),
                        Action::Scale(corner) => {
                            let b = g.bounds.ok_or("Empty selection")?;
                            let anchor = tx::corners(b)[(corner + 2) % 4];
                            let from = sub(g.start, anchor);
                            let to = sub(p, anchor);
                            let mut factors = std::array::from_fn::<_, 2, _>(|i| {
                                if from[i].abs() > 1e-8 {
                                    to[i] / from[i]
                                } else {
                                    1.
                                }
                            });
                            if shift {
                                let s = if (factors[0] - 1.).abs() > (factors[1] - 1.).abs() {
                                    factors[0]
                                } else {
                                    factors[1]
                                };
                                factors = [s; 2];
                            }
                            let v = sub(q, anchor);
                            add(anchor, [v[0] * factors[0], v[1] * factors[1]])
                        }
                        Action::Pin(corner) => {
                            let b = g.bounds.ok_or("Empty selection")?;
                            let mut corners = tx::corners(b);
                            corners[corner] = add(corners[corner], sub(p, g.start));
                            tx::project(b, corners, q)?
                        }
                        Action::Rotate => {
                            let b = g.bounds.ok_or("Empty selection")?;
                            let center = [(b[0] + b[2]) * 0.5, (b[1] + b[3]) * 0.5];
                            let a = sub(g.start, center);
                            let b = sub(p, center);
                            let mut angle = b[1].atan2(b[0]) - a[1].atan2(a[0]);
                            if shift {
                                let step = std::f64::consts::PI / 12.;
                                angle = (angle / step).round() * step;
                            }
                            tx::rotate(q, center, angle)
                        }
                        _ => q,
                    };
                    Ok(map(inverse, q))
                })?;
            }
            _ => {}
        }
    }
    Ok(result)
}

pub(super) fn insert_layer(
    roto: &mut Roto,
    mut layer: Layer,
    selected: Option<ObjectId>,
) -> Result<(), String> {
    let parent = selected
        .and_then(|id| roto.root.find(id))
        .filter(|l| l.shape.is_none())
        .map_or(roto.root.id, |l| l.id);
    if !roto.root.editable(parent) {
        return Err("Roto layer is locked".into());
    }
    let inverse = roto
        .transform_for(parent)
        .ok_or("Missing Roto group")?
        .inverse()?;
    for p in &mut layer.shape.as_mut().ok_or("Expected shape")?.points {
        tx::point(p, |q| Ok(map(inverse, q)))?;
    }
    roto.root.find_mut(parent).unwrap().children.push(layer);
    Ok(())
}
