//! Scene channels retain object ownership and exact nested clocks.
use super::{Binding, Clock, Scene, Time};
use fold_foundation::ObjectId;
use fold_platform::animation::{Channel, Target};
use std::collections::{BTreeMap, BTreeSet};

struct Entry {
    channel: Channel,
    native: Option<Channel>,
    transform: Option<&'static str>,
    clocks: Vec<Clock>,
    source_curve: fold_animation::Curve,
}
fn retime(
    curve: &mut fold_animation::Curve,
    clocks: &[Clock],
    start: Time,
    to_scene: bool,
) -> Result<(), String> {
    if let Some(time) = frozen_time(clocks, start)? {
        if !to_scene {
            return Err("A paused clock has no inverse for key timing edits".into());
        }
        let value = curve.sample(time).ok_or("Empty animation track")?;
        let existing = curve.at(time).map(|k| k.id);
        let marker = existing.unwrap_or(curve.keys[0].id);
        curve.insert(time, value);
        let mut key = curve.at(time).ok_or("Missing frozen sample")?.clone();
        key.id = marker;
        key.time = Time::ZERO;
        key.interpolation = fold_animation::Interpolation::Linear;
        curve.keys = vec![key];
        return curve.validate();
    }
    let reversed = clocks.iter().filter(|c| c.rate.numerator() < 0).count() % 2 == 1;
    let original = curve.clone();
    for key in &mut curve.keys {
        if to_scene {
            for clock in clocks.iter().rev() {
                key.time = key
                    .time
                    .checked_divide(clock.rate.numerator())
                    .and_then(|t| t.checked_scale(i64::from(clock.rate.denominator()), 1))
                    .and_then(|t| t.checked_add(clock.offset))
                    .map_err(|e| e.to_string())?;
            }
            key.time = key.time.checked_sub(start).map_err(|e| e.to_string())?;
        } else {
            key.time = key.time.checked_add(start).map_err(|e| e.to_string())?;
            for clock in clocks {
                key.time = clock.at(key.time)?;
            }
        }
    }
    if reversed {
        use fold_animation::Interpolation;
        let reverse = |i| match i {
            Interpolation::Hold => Interpolation::HoldNext,
            Interpolation::HoldNext => Interpolation::Hold,
            other => other,
        };
        curve.keys.reverse();
        let n = curve.keys.len();
        for (i, key) in curve.keys.iter_mut().enumerate() {
            std::mem::swap(&mut key.incoming, &mut key.outgoing);
            if let Some(rotation) = &mut key.rotation {
                rotation.reversed = !rotation.reversed;
            }
            key.interpolation =
                reverse(original.keys[if i + 1 == n { n - 1 } else { n - 2 - i }].interpolation);
        }
    }
    curve.validate()
}
fn frozen_time(clocks: &[Clock], start: Time) -> Result<Option<Time>, String> {
    if !clocks.iter().any(|c| c.rate.numerator() == 0) {
        return Ok(None);
    }
    let mut time = start;
    for clock in clocks {
        time = clock.at(time)?;
    }
    Ok(Some(time))
}
fn restore(
    entry: &Entry,
    edited: &fold_animation::Curve,
    start: Time,
) -> Result<fold_animation::Curve, String> {
    if let Some(time) = frozen_time(&entry.clocks, start)? {
        // A frozen marker is a sampled value, not an invertible key-time handle.
        let mut identity = edited.clone();
        if identity.keys.len() != 1 {
            return Err("Resume the clock before adding or removing timeline keys".into());
        }
        identity.keys[0].value = entry.channel.curve.keys[0].value;
        if identity != entry.channel.curve {
            return Err("Resume the clock before editing key timing or interpolation".into());
        }
        let mut curve = entry.source_curve.clone();
        curve.insert(time, edited.keys[0].value);
        curve.validate()?;
        Ok(curve)
    } else {
        let mut curve = edited.clone();
        retime(&mut curve, &entry.clocks, start, false)?;
        Ok(curve)
    }
}
fn entries(scene: &Scene) -> Result<Vec<Entry>, String> {
    let mut result = vec![];
    for object in &scene.objects {
        let clocks: Vec<_> = scene
            .ancestors(object.id)?
            .iter()
            .map(|o| o.clock)
            .collect();
        for (name, binding) in [
            ("Position", &object.position),
            ("Rotation", &object.rotation),
            ("Scale", &object.scale),
        ] {
            if let Binding::Track { track } = binding {
                for (component, mut curve) in crate::keyed::track::curves(track)? {
                    let source_curve = curve.clone();
                    retime(&mut curve, &clocks, scene.canvas.start, true)?;
                    result.push(Entry {
                        channel: Channel {
                            target: Target {
                                group: Some(object.id),
                                object: object.id,
                                path: format!("transform:{name}:{}:{component}", track.id),
                            },
                            node_label: object.name.clone(),
                            property: name.into(),
                            property_label: if frozen_time(&clocks, scene.canvas.start)?.is_some() {
                                format!("{name} · Paused")
                            } else {
                                name.into()
                            },
                            component,
                            curve,
                        },
                        native: None,
                        transform: Some(name),
                        source_curve,
                        clocks: clocks.clone(),
                    });
                }
            }
        }
        let available = crate::keyed::channels(&object.graph, None)?;
        let mut pending = vec![(&object.graph.root, clocks, Vec::<ObjectId>::new())];
        let mut occurrences = 0usize;
        while let Some((instance, parent_clocks, mut path)) = pending.pop() {
            occurrences += 1;
            if occurrences > 4096 || path.len() > 256 {
                return Err("Animation occurrence budget exceeded".into());
            }
            path.push(instance.id);
            let mut local_clocks = parent_clocks.clone();
            local_clocks.push(instance.clock);
            let definition = object.graph.definition(instance.definition)?;
            for native in &available {
                let override_track = native.target.object == instance.id;
                if !override_track
                    && !definition
                        .nodes
                        .iter()
                        .any(|n| n.id == native.target.object)
                {
                    continue;
                }
                let clocks = if override_track {
                    &parent_clocks
                } else {
                    &local_clocks
                };
                let mut channel = native.clone();
                channel.target = Target {
                    group: Some(object.id),
                    object: native.target.object,
                    path: format!(
                        "graph:{}:{}",
                        serde_json::to_string(&path).map_err(|e| e.to_string())?,
                        serde_json::to_string(&native.target).map_err(|e| e.to_string())?
                    ),
                };
                channel.node_label = format!("{} / {}", instance.name, channel.node_label);
                retime(&mut channel.curve, clocks, scene.canvas.start, true)?;
                if frozen_time(clocks, scene.canvas.start)?.is_some() {
                    channel.property_label.push_str(" · Paused");
                }
                result.push(Entry {
                    channel,
                    native: Some(native.clone()),
                    transform: None,
                    clocks: clocks.clone(),
                    source_curve: native.curve.clone(),
                });
            }
            for child in definition.instances.iter().rev() {
                pending.push((child, local_clocks.clone(), path.clone()));
            }
        }
    }
    Ok(result)
}
pub fn channels(scene: &Scene, object: Option<ObjectId>) -> Result<Vec<Channel>, String> {
    Ok(entries(scene)?
        .into_iter()
        .map(|e| e.channel)
        .filter(|c| object.is_none_or(|id| c.target.group == Some(id) || c.target.object == id))
        .collect())
}
pub fn replace(scene: &Scene, edits: &[Channel]) -> Result<Scene, String> {
    let available = entries(scene)?;
    let mut seen = BTreeSet::new();
    let mut graph_edits = BTreeMap::<_, Vec<_>>::new();
    let mut transforms = BTreeMap::<_, Vec<_>>::new();
    for edit in edits {
        if !seen.insert(edit.target.clone()) {
            return Err("Duplicate scene animation channel".into());
        }
        let entry = available
            .iter()
            .find(|e| e.channel.target == edit.target)
            .ok_or("Unknown scene animation channel")?;
        let mut identity = edit.clone();
        identity.curve = entry.channel.curve.clone();
        if identity != entry.channel {
            return Err("Scene animation channel identity changed".into());
        }
        if edit.curve == entry.channel.curve {
            continue;
        }
        let object = edit.target.group.ok_or("Missing scene object scope")?;
        if scene.locked(object) {
            return Err("Scene object or ancestor is locked".into());
        }
        let curve = restore(entry, &edit.curve, scene.canvas.start)?;
        if let Some(native) = &entry.native {
            let mut native = native.clone();
            native.curve = curve;
            let proposals = graph_edits.entry(object).or_default();
            if let Some(existing) = proposals
                .iter()
                .find(|c: &&Channel| c.target == native.target)
            {
                if existing != &native {
                    return Err("Reusable track received conflicting occurrence edits".into());
                }
            } else {
                proposals.push(native);
            }
        } else if let Some(name) = entry.transform {
            transforms
                .entry((object, name))
                .or_default()
                .push((edit.component.clone(), curve));
        }
    }
    let mut result = scene.clone();
    for object in &mut result.objects {
        if let Some(edits) = graph_edits.get(&object.id) {
            object.graph = crate::keyed::replace(&object.graph, edits)?;
        }
        for (name, binding) in [
            ("Position", &mut object.position),
            ("Rotation", &mut object.rotation),
            ("Scale", &mut object.scale),
        ] {
            if let Some(edits) = transforms.get(&(object.id, name)) {
                let Binding::Track { track } = binding else {
                    return Err("Scene transform track changed".into());
                };
                *track = crate::keyed::track::replace(track, edits)?;
            }
        }
    }
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn reverse_rotation_keeps_the_native_quaternion_branch() {
        use crate::{
            binding::Track,
            types::{Literal, Ratio, Space},
        };
        let mut track = Track::constant(
            Literal::Rotation {
                value: [0., 0., 0., 1.],
                space: Space::World,
            },
            Time::ZERO,
        )
        .unwrap();
        let mut key = track.keys[0].clone();
        key.id = ObjectId::new();
        key.time = Time::new(2, 1).unwrap();
        let angle = std::f64::consts::FRAC_PI_4;
        key.value = Literal::Rotation {
            value: [0., 0., -angle.sin(), -angle.cos()],
            space: Space::World,
        };
        track.keys.push(key);
        let clock = Clock {
            offset: Time::new(2, 1).unwrap(),
            rate: Ratio::new(-1, 1).unwrap(),
        };
        for (_, source) in crate::keyed::track::curves(&track).unwrap() {
            let mut view = source.clone();
            retime(&mut view, &[clock], Time::ZERO, true).unwrap();
            for tick in 0..=20 {
                let time = Time::new(tick, 10).unwrap();
                assert!(
                    (view.sample(time).unwrap() - source.sample(clock.at(time).unwrap()).unwrap())
                        .abs()
                        < 1e-12
                );
            }
            retime(&mut view, &[clock], Time::ZERO, false).unwrap();
            assert_eq!(view, source);
        }
    }
    #[test]
    fn reversed_nested_clocks_preserve_hold_sides_bezier_and_exact_key_times() {
        use crate::types::Ratio;
        use fold_animation::{Curve, Interpolation, Key, Tangents};
        let clocks = [
            Clock {
                offset: Time::new(3, 1).unwrap(),
                rate: Ratio::new(-2, 3).unwrap(),
            },
            Clock {
                offset: Time::new(1, 2).unwrap(),
                rate: Ratio::new(3, 2).unwrap(),
            },
        ];
        let start = Time::new(1, 4).unwrap();
        for mode in [
            Interpolation::Hold,
            Interpolation::Linear,
            Interpolation::Bezier,
        ] {
            let mut a = Key::new(Time::ZERO, 2.);
            a.interpolation = mode;
            a.tangents = Tangents::Broken;
            a.outgoing.value = 3.;
            a.outgoing.fraction = 0.2;
            let mut b = Key::new(Time::new(2, 1).unwrap(), 8.);
            b.tangents = Tangents::Broken;
            b.incoming.value = -1.;
            let source = Curve {
                keys: vec![a, b, Key::new(Time::new(3, 1).unwrap(), 10.)],
            };
            let mut view = source.clone();
            retime(&mut view, &clocks, start, true).unwrap();
            for tick in -40..=40 {
                let scene = Time::new(tick, 8).unwrap();
                let mut local = scene.checked_add(start).unwrap();
                for clock in clocks {
                    local = clock.at(local).unwrap();
                }
                assert!(
                    (view.sample(scene).unwrap() - source.sample(local).unwrap()).abs() < 1e-10
                );
            }
            retime(&mut view, &clocks, start, false).unwrap();
            assert_eq!(view, source);
        }
    }
}
