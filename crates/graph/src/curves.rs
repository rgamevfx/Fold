//! Component curves are views of coupled typed keys, not independent key storage.
use crate::{
    binding::{Interpolation, Key, Track},
    types::{Literal, Type},
};
use fold_animation::Curve;
use fold_foundation::ObjectId;
use std::collections::{BTreeMap, BTreeSet};

pub fn components(ty: &Type) -> &'static [&'static str] {
    match ty {
        Type::Scalar | Type::Distance { .. } => &[""],
        Type::Vector { dimensions: 2, .. } => &["X", "Y"],
        Type::Vector { dimensions: 3, .. } => &["X", "Y", "Z"],
        Type::Color { .. } => &["R", "G", "B", "A"],
        Type::Rotation { .. } => &["X", "Y", "Z", "W"],
        _ => &[],
    }
}
fn value(literal: &Literal, index: usize) -> Result<f64, String> {
    match literal {
        Literal::Scalar { value } | Literal::Distance { value, .. } if index == 0 => Ok(*value),
        Literal::Vector { value, .. } => value
            .get(index)
            .copied()
            .ok_or("Missing vector component".into()),
        Literal::Color { value, .. } | Literal::Rotation { value, .. } => value
            .get(index)
            .copied()
            .ok_or("Missing color component".into()),
        _ => Err("Unsupported curve value".into()),
    }
}
pub(crate) fn set(literal: &mut Literal, index: usize, number: f64) -> Result<(), String> {
    match literal {
        Literal::Scalar { value } | Literal::Distance { value, .. } if index == 0 => {
            *value = number
        }
        Literal::Vector { value, .. } => {
            *value.get_mut(index).ok_or("Missing vector component")? = number
        }
        Literal::Color { value, .. } | Literal::Rotation { value, .. } => {
            *value.get_mut(index).ok_or("Missing color component")? = number
        }
        _ => return Err("Unsupported curve value".into()),
    };
    Ok(())
}
pub fn curves(track: &Track) -> Result<Vec<(String, Curve)>, String> {
    track.validate()?;
    components(&track.ty)
        .iter()
        .enumerate()
        .map(|(index, name)| {
            let curve = Curve {
                keys: track
                    .keys
                    .iter()
                    .map(|key| {
                        Ok(fold_animation::Key {
                            rotation: if let Literal::Rotation { value, .. } = key.value {
                                Some(fold_animation::RotationSample {
                                    quaternion: value,
                                    component: index,
                                    reversed: false,
                                })
                            } else {
                                None
                            },
                            id: key.id,
                            time: key.time,
                            value: value(&key.value, index)?,
                            interpolation: match key.interpolation {
                                Interpolation::Linear => fold_animation::Interpolation::Linear,
                                Interpolation::Hold => fold_animation::Interpolation::Hold,
                                Interpolation::HoldNext => fold_animation::Interpolation::HoldNext,
                                Interpolation::Bezier => fold_animation::Interpolation::Bezier,
                            },
                            tangents: key.handles.get(index).copied().unwrap_or_default().tangents,
                            incoming: key.handles.get(index).copied().unwrap_or_default().incoming,
                            outgoing: key.handles.get(index).copied().unwrap_or_default().outgoing,
                        })
                    })
                    .collect::<Result<Vec<_>, String>>()?,
            };
            curve.validate()?;
            Ok(((*name).into(), curve))
        })
        .collect()
}

/// Moving/deleting one component key moves/deletes its whole typed key. Component
/// values remain independent. Conflicting coupled edits fail atomically.
pub fn replace(track: &Track, edits: &[(String, Curve)]) -> Result<Track, String> {
    let available = curves(track)?;
    let mut contribution = track.clone();
    contribution.combination = crate::binding::Combination::Replace;
    contribution.base = None;
    let mut changed = BTreeMap::<ObjectId, Key>::new();
    let mut timing = BTreeMap::new();
    let mut deleted = BTreeSet::new();
    let mut touched = BTreeSet::new();
    let mut additions = BTreeMap::new();
    let mut seen = BTreeSet::new();
    let mut shapes = BTreeMap::new();
    for (component, curve) in edits {
        let index = available
            .iter()
            .position(|(name, _)| name == component)
            .ok_or("Unknown track component")?;
        if !seen.insert(index) {
            return Err("Duplicate track component".into());
        }
        let original = &available[index].1;
        if curve == original {
            continue;
        }
        curve.validate()?;
        for key in &curve.keys {
            if let Some(old) = original.keys.iter().find(|k| k.id == key.id)
                && key.rotation != old.rotation
            {
                return Err("Rotation context is read-only".into());
            }
        }
        for key in &track.keys {
            if !curve.keys.iter().any(|k| k.id == key.id) {
                deleted.insert(key.id);
            }
        }
        for key in &curve.keys {
            let interpolation = match key.interpolation {
                fold_animation::Interpolation::Linear => Interpolation::Linear,
                fold_animation::Interpolation::Hold => Interpolation::Hold,
                fold_animation::Interpolation::HoldNext => Interpolation::HoldNext,
                fold_animation::Interpolation::Bezier => Interpolation::Bezier,
            };
            shapes.insert(
                (key.id, index),
                crate::binding::Handles {
                    tangents: key.tangents,
                    incoming: key.incoming,
                    outgoing: key.outgoing,
                },
            );
            let old = track.keys.iter().find(|old| old.id == key.id);
            if let Some(old) = old {
                if old.time != key.time || old.interpolation != interpolation {
                    let intent = (key.time, interpolation);
                    if timing
                        .insert(key.id, intent)
                        .is_some_and(|previous| previous != intent)
                    {
                        return Err(
                            "Coupled key received conflicting timing or interpolation edits".into(),
                        );
                    }
                }
                if value(&old.value, index)? != key.value
                    || old.time != key.time
                    || old.interpolation != interpolation
                    || old.handles.get(index).copied().unwrap_or_default()
                        != shapes[&(key.id, index)]
                {
                    touched.insert(key.id);
                }
                let target = changed.entry(key.id).or_insert_with(|| old.clone());
                set(&mut target.value, index, key.value)?;
            } else {
                // Independent component insertions at the same time create one typed key.
                let target = additions.entry(key.time).or_insert(Key {
                    id: key.id,
                    time: key.time,
                    value: contribution.sample(key.time)?,
                    interpolation,
                    handles: Default::default(),
                    extensions: Default::default(),
                });
                if target.interpolation != interpolation {
                    return Err("Coupled key received conflicting interpolation edits".into());
                }
                set(&mut target.value, index, key.value)?;
                let shape = shapes[&(key.id, index)];
                shapes.insert((target.id, index), shape);
            }
        }
    }
    if deleted.iter().any(|id| touched.contains(id)) {
        return Err("Coupled key was both deleted and edited".into());
    }
    let mut result = track.clone();
    result.keys = track
        .keys
        .iter()
        .filter(|key| !deleted.contains(&key.id))
        .map(|key| {
            let mut key = changed.remove(&key.id).unwrap_or_else(|| key.clone());
            if let Some((time, interpolation)) = timing.get(&key.id) {
                key.time = *time;
                key.interpolation = *interpolation;
            }
            key
        })
        .chain(additions.into_values())
        .collect();
    for key in &mut result.keys {
        if let Literal::Rotation { value, .. } = &mut key.value {
            *value = fold_foundation::rotation::normalize(*value);
        }
        for index in 0..available.len() {
            if let Some(shape) = shapes.get(&(key.id, index))
                && (*shape != crate::binding::Handles::default() || !key.handles.is_empty())
            {
                key.handles.resize(available.len(), Default::default());
                key.handles[index] = *shape;
            }
        }
    }
    result.keys.sort_by_key(|key| key.time);
    result.validate()?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::{Binding, Track};
    use crate::types::{Space, Unit};
    use fold_foundation::Time;
    #[test]
    fn bezier_component_edits_roundtrip_and_sample_shared_curve_semantics() {
        let mut track = Track::constant(
            Literal::Vector {
                value: vec![0., 2.],
                unit: Unit::Unitless,
                space: Space::World,
            },
            Time::ZERO,
        )
        .unwrap();
        let mut last = track.keys[0].clone();
        last.id = ObjectId::new();
        last.time = Time::new(2, 1).unwrap();
        last.value = Literal::Vector {
            value: vec![4., 6.],
            unit: Unit::Unitless,
            space: Space::World,
        };
        track.keys.push(last);
        track.keys[0]
            .extensions
            .insert("vendor".into(), serde_json::json!(7));
        let mut edit = curves(&track).unwrap().remove(0);
        edit.1.keys[0].interpolation = fold_animation::Interpolation::Bezier;
        edit.1.keys[0].tangents = fold_animation::Tangents::Broken;
        edit.1.keys[0].outgoing.value = 3.;
        edit.1.keys[0].outgoing.fraction = 0.2;
        let result = replace(&track, &[edit.clone()]).unwrap();
        assert_eq!(result.id, track.id);
        assert_eq!(result.keys[0].id, track.keys[0].id);
        assert_eq!(result.keys[0].extensions, track.keys[0].extensions);
        let view = curves(&result).unwrap();
        assert_eq!(view[0], edit);
        for tick in [0, 1, 3, 7, 12, 19, 20] {
            let time = Time::new(tick, 10).unwrap();
            let Literal::Vector { value, .. } = result.sample(time).unwrap() else {
                panic!()
            };
            for i in 0..2 {
                assert!((value[i] - view[i].1.sample(time).unwrap()).abs() < 1e-12);
            }
        }
        let binding = Binding::Track {
            track: result.clone(),
        };
        let decoded: Binding =
            serde_json::from_slice(&serde_json::to_vec(&binding).unwrap()).unwrap();
        assert_eq!(binding, decoded);
        let mut tangent = view[0].clone();
        tangent.1.keys[0].outgoing.value = 1.;
        let mut delete = view[1].clone();
        delete.1.keys.remove(0);
        assert!(
            replace(&result, &[tangent, delete])
                .unwrap_err()
                .contains("deleted and edited")
        );
    }
}

#[cfg(test)]
mod rotation_tests {
    use super::*;
    use crate::types::{Literal, Space};
    use fold_foundation::Time;
    #[test]
    fn rotation_projection_samples_spherically_and_edits_coupled_keys() {
        let rotation = |value| Literal::Rotation {
            value,
            space: Space::World,
        };
        let mut track = Track::constant(rotation([0., 0., 0., 1.]), Time::ZERO).unwrap();
        let mut end = track.keys[0].clone();
        end.id = ObjectId::new();
        end.time = Time::new(2, 1).unwrap();
        end.value = rotation([0., 0., 1., 0.]);
        track.keys.push(end);
        let mut projected = curves(&track).unwrap();
        assert_eq!(projected.len(), 4);
        for tick in 0..=20 {
            let time = Time::new(tick, 10).unwrap();
            let Literal::Rotation { value, .. } = track.sample(time).unwrap() else {
                panic!()
            };
            for (i, (_, curve)) in projected.iter().enumerate() {
                assert!((curve.sample(time).unwrap() - value[i]).abs() < 1e-12);
            }
        }
        // Insertion uses the same spherical sample as the evaluator.
        projected[2]
            .1
            .insert(Time::new(1, 1).unwrap(), std::f64::consts::FRAC_1_SQRT_2);
        let inserted = replace(&track, &[projected[2].clone()]).unwrap();
        assert_eq!(inserted.keys.len(), 3);
        assert_eq!(
            inserted.sample(Time::new(1, 1).unwrap()).unwrap(),
            track.sample(Time::new(1, 1).unwrap()).unwrap()
        );
        let mut edit = projected[0].clone();
        edit.1.keys[0].value = 0.5;
        let updated = replace(&track, &[edit]).unwrap();
        let Literal::Rotation { value, .. } = updated.keys[0].value else {
            panic!()
        };
        assert!((value.iter().map(|v| v * v).sum::<f64>() - 1.).abs() < 1e-12);
        assert_eq!(updated.keys[0].id, track.keys[0].id);
    }
}
