//! Instance-local tools. Persistent edits use the same compositor transaction as
//! the inspector; mouse motion never owns a render context or device.
mod gesture;
mod inspector;
mod keys;
mod overlay;
mod transform;
mod viewport;
use crate::roto::Roto;
use fold_foundation::{ObjectId, Time};
pub(super) use inspector::draw_inspector;
use std::collections::BTreeSet;
#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(super) enum Tool {
    #[default]
    Select,
    Rectangle,
    Ellipse,
    Bezier,
    BSpline,
}
pub(super) struct Editor {
    pub target: Option<ObjectId>,
    pub layer: Option<ObjectId>,
    pub points: BTreeSet<ObjectId>,
    pub tool: Tool,
    pub show_feather: bool,
    pub auto_key: bool,
    gesture: Option<gesture::Gesture>,
    drawing: Option<crate::roto::Layer>,
    pen_anchor: Option<[f64; 2]>,
}
impl Default for Editor {
    fn default() -> Self {
        Self {
            target: None,
            layer: None,
            points: BTreeSet::new(),
            tool: Tool::default(),
            show_feather: false,
            auto_key: true,
            gesture: None,
            drawing: None,
            pen_anchor: None,
        }
    }
}
impl Editor {
    pub fn select(&mut self, id: ObjectId) {
        if self.target != Some(id) {
            *self = Self {
                target: Some(id),
                ..Default::default()
            };
        }
    }
    pub fn active(&self) -> bool {
        self.gesture.is_some() || self.drawing.is_some()
    }
    pub fn reset_gesture(&mut self) {
        self.gesture = None;
        self.drawing = None;
        self.pen_anchor = None;
    }
}
/// Transfer changed sampled controls through shared scalar tracks. Other controls
/// keep their authored bases; adding/deleting points retains surviving IDs/tracks.
pub(super) fn edit(
    node: &mut crate::Node,
    before: &Roto,
    after: Roto,
    time: Time,
    auto_key: bool,
) -> Result<(), String> {
    let mut next = node.clone();
    let crate::OperationConfig::Roto { settings: base } = node.prepare()?.config().clone() else {
        return Err("Not a Roto node".into());
    };
    let old: std::collections::BTreeMap<_, _> = before
        .properties()
        .into_iter()
        .map(|p| (p.id, p.values))
        .collect();
    let bases: std::collections::BTreeMap<_, _> = base
        .properties()
        .into_iter()
        .map(|p| (p.id, p.values))
        .collect();
    let properties = after.properties();
    let available: BTreeSet<_> = properties
        .iter()
        .flat_map(|p| (0..p.values.len()).map(move |i| crate::animation::path(&p.id, i)))
        .collect();
    let remove: Vec<_> = next
        .animation
        .bindings()
        .keys()
        .filter(|p| p.starts_with("roto/") && !available.contains(*p))
        .cloned()
        .collect();
    for path in remove {
        next.animation.remove(&path)?;
    }
    let changed_shapes: BTreeSet<_> = properties
        .iter()
        .filter(|p| old.get(&p.id) != Some(&p.values))
        .filter_map(|p| keys::geometry(&p.id))
        .collect();
    for shape in changed_shapes {
        let times = keys::times(&next, shape)?;
        if auto_key || times.contains(&time) {
            keys::insert(&mut next, &after, shape, time)?;
        } else if !times.is_empty() {
            return Err("Enable Auto key or add a shape key before editing between keys".into());
        }
    }
    let mut settings = after;
    for property in properties {
        let Some(previous) = old.get(&property.id) else {
            continue;
        };
        let mut values = property.values.clone();
        for (i, (&value, &previous)) in property.values.iter().zip(previous).enumerate() {
            let path = crate::animation::path(&property.id, i);
            if value != previous {
                if next.animation.driven(&path) {
                    return Err("Roto control is driven; edit its source".into());
                }
                if (auto_key && keys::geometry(&property.id).is_some())
                    || next
                        .animation
                        .curve(&path)?
                        .is_some_and(|c| c.at(time).is_some())
                {
                    let mut curve = next.animation.curve(&path)?.unwrap_or_default();
                    curve.insert(time, value);
                    next.animation.insert(path.clone(), curve)?;
                }
            }
            if next.animation.binding(&path)?.is_some() {
                values[i] = bases.get(&property.id).map_or(previous, |b| b[i]);
            }
        }
        settings.set_property(&property.id, &values)?;
    }
    next.set_configuration(crate::OperationConfig::Roto { settings })?;
    *node = next;
    Ok(())
}

#[cfg(test)]
mod tests;
