//! Typed key editing without splitting native key identity into component curves.
use super::literal;
use crate::binding::{Combination, Key, Track};
use fold_foundation::{ObjectId, Time};
use fold_ui::sdk::{EditResponse, imgui::Ui};

fn insert(track: &mut Track, time: Time) -> Result<(), String> {
    track.validate()?;
    if track.keys.len() >= 4096 || track.keys.iter().any(|key| key.time == time) {
        return Err("Choose an unused time; a track supports up to 4096 keys.".into());
    }
    // Keys store the authored contribution, before Add/Multiply applies the base.
    let mut contribution = track.clone();
    contribution.combination = Combination::Replace;
    contribution.base = None;
    let value = contribution.sample(time)?;
    let segment = track
        .keys
        .partition_point(|key| key.time <= time)
        .saturating_sub(1);
    let interpolation = track.keys[segment].interpolation;
    track.keys.push(Key {
        id: ObjectId::new(),
        time,
        value,
        interpolation,
        handles: Default::default(),
        extensions: Default::default(),
    });
    track.keys.sort_by_key(|key| key.time);
    Ok(())
}

fn supports(track: &Track, combination: Combination) -> bool {
    if combination == Combination::Replace {
        return true;
    }
    let op = if combination == Combination::Add {
        crate::expression::Binary::Add
    } else {
        crate::expression::Binary::Multiply
    };
    crate::expression::binary_type(op, &track.ty, &track.ty).is_ok_and(|ty| ty == track.ty)
}

fn combine(track: &mut Track, combination: Combination) -> Result<(), String> {
    use crate::types::Literal;
    if !supports(track, combination) {
        return Err("This key type does not support that combination.".into());
    }
    let base = if combination == Combination::Replace {
        None
    } else if let Some(base) = &track.base {
        Some(base.clone())
    } else {
        let mut neutral = track
            .keys
            .first()
            .ok_or("A track requires a key")?
            .value
            .clone();
        match &mut neutral {
            Literal::Scalar { value } => {
                *value = if combination == Combination::Multiply {
                    1.
                } else {
                    0.
                }
            }
            Literal::Distance { value, .. } => *value = 0.,
            Literal::Vector { value, .. } => value.fill(0.),
            _ => return Err("This key type has no supported neutral base.".into()),
        }
        Some(neutral)
    };
    track.combination = combination;
    track.base = base;
    Ok(())
}

pub(super) fn draw(
    ui: &Ui,
    label: &str,
    track: &mut Track,
    time: Option<Time>,
    response: &mut EditResponse,
    controls: &mut literal::Controls,
) {
    if let Err(error) = track.validate() {
        ui.text_wrapped(error);
        controls.invalid = true;
        response.finished = false;
        return;
    }
    let Some(time) = time else {
        ui.text_disabled(format!("{label} · Select an instance occurrence"));
        return;
    };
    let mut contribution = track.clone();
    contribution.combination = Combination::Replace;
    contribution.base = None;
    let Ok(mut value) = contribution.sample(time) else {
        return;
    };
    let before = value.clone();
    literal::draw(ui, label, &mut value, response, controls);
    if value != before && !controls.invalid {
        if !track.keys.iter().any(|key| key.time == time)
            && let Err(error) = insert(track, time)
        {
            ui.text_wrapped(error);
            return;
        }
        if let Some(key) = track.keys.iter_mut().find(|key| key.time == time) {
            key.value = value;
        }
    }
    ui.same_line();
    ui.text_disabled(if track.keys.iter().any(|key| key.time == time) {
        "Keyed"
    } else {
        "Animated"
    });
    ui.same_line();
    if let Some(_menu) = fold_ui::sdk::toolbar::icon_menu(
        ui,
        "animation",
        "Animation · edit keys and curves in Composition",
    ) {
        let has_key = track.keys.iter().any(|key| key.time == time);
        if !has_key && ui.menu_item("Add key at playhead") {
            match insert(track, time) {
                Ok(()) => {
                    response.changed = true;
                    response.finished = true;
                }
                Err(error) => ui.text_wrapped(error),
            }
        }
        if has_key && track.keys.len() > 1 && ui.menu_item("Remove key at playhead") {
            track.keys.retain(|key| key.time != time);
            response.changed = true;
            response.finished = true;
        }
        ui.separator();
        for (label, combination) in [
            ("Replace", Combination::Replace),
            ("Add to base", Combination::Add),
            ("Multiply base", Combination::Multiply),
        ] {
            let _disabled = ui.begin_disabled_with_cond(!supports(track, combination));
            if ui
                .selectable_config(label)
                .selected(track.combination == combination)
                .build()
                && track.combination != combination
            {
                match combine(track, combination) {
                    Ok(()) => {
                        response.changed = true;
                        response.finished = true;
                    }
                    Err(error) => ui.text_wrapped(error),
                }
            }
        }
        if let Some(base) = &mut track.base {
            literal::draw(ui, "Base", base, response, controls);
        }
    }
    if let Err(error) = track.validate() {
        ui.text_wrapped(error);
        controls.invalid = true;
        response.finished = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::binding::Interpolation;
    use crate::types::{Literal, Space, Type, Unit};
    #[test]
    fn insertion_uses_exact_time_and_preserves_neighbor_identity() {
        let mut track =
            Track::constant(Literal::Scalar { value: 4. }, Time::new(-1, 3).unwrap()).unwrap();
        track.keys[0].interpolation = Interpolation::Hold;
        let before = track.clone();
        let time = Time::new(2, 3).unwrap();
        insert(&mut track, time).unwrap();
        assert_eq!(track.keys[0], before.keys[0]);
        assert_ne!(track.keys[0].id, track.keys[1].id);
        assert_eq!(track.keys[1].interpolation, Interpolation::Hold);
        assert_eq!(track.sample(time).unwrap(), Literal::Scalar { value: 4. });
    }
    #[test]
    fn combinations_preserve_keys_and_use_neutral_bases() {
        for value in [
            Literal::Scalar { value: 12. },
            Literal::Distance {
                value: 12.,
                unit: Unit::Canvas,
            },
            Literal::Vector {
                value: vec![12., -4.],
                unit: Unit::Canvas,
                space: Space::Screen,
            },
            Literal::Rotation {
                value: [0., 0., 0., 1.],
                space: Space::World,
            },
        ] {
            let mut track = Track {
                id: ObjectId::new(),
                ty: value.ty(),
                combination: Combination::Replace,
                base: None,
                extensions: Default::default(),
                keys: vec![Key {
                    id: ObjectId::new(),
                    time: Time::ZERO,
                    value: value.clone(),
                    interpolation: Interpolation::Hold,
                    handles: Default::default(),
                    extensions: Default::default(),
                }],
            };
            let before = track.clone();
            for combination in [Combination::Add, Combination::Multiply] {
                if supports(&track, combination) {
                    combine(&mut track, combination).unwrap();
                    assert_eq!(track.sample(Time::new(1, 3).unwrap()).unwrap(), value);
                    assert_eq!(track.keys, before.keys);
                    assert_eq!(track.id, before.id);
                    combine(&mut track, Combination::Replace).unwrap();
                    assert_eq!(track, before);
                } else {
                    assert!(combine(&mut track, combination).is_err());
                    assert_eq!(track, before);
                }
            }
        }
    }
    #[test]
    fn insertion_samples_contribution_and_retains_typed_identity() {
        let vector = |x| Literal::Vector {
            value: vec![x, 0.],
            unit: Unit::Canvas,
            space: Space::Screen,
        };
        let mut track = Track {
            id: ObjectId::new(),
            ty: Type::Vector {
                dimensions: 2,
                unit: Unit::Canvas,
                space: Space::Screen,
            },
            combination: Combination::Add,
            base: Some(vector(100.)),
            extensions: Default::default(),
            keys: [0, 2]
                .into_iter()
                .map(|n| Key {
                    id: ObjectId::new(),
                    time: Time::new(n, 1).unwrap(),
                    value: vector(n as f64 * 10.),
                    interpolation: Interpolation::Linear,
                    handles: Default::default(),
                    extensions: Default::default(),
                })
                .collect(),
        };
        let before = track.clone();
        insert(&mut track, Time::new(1, 1).unwrap()).unwrap();
        assert_eq!(track.keys[1].value, vector(10.));
        assert_eq!(
            track.sample(Time::new(1, 1).unwrap()).unwrap(),
            vector(110.)
        );
        assert_eq!(track.keys[0], before.keys[0]);
        assert_eq!(track.keys[2], before.keys[1]);
        assert_eq!(track.id, before.id);
        let inserted = track.clone();
        assert!(insert(&mut track, Time::new(1, 1).unwrap()).is_err());
        assert_eq!(track, inserted);
    }
    #[test]
    fn insertion_preserves_hold_and_linear_samples_with_combination() {
        for interpolation in [Interpolation::Hold, Interpolation::Linear] {
            for combination in [
                Combination::Replace,
                Combination::Add,
                Combination::Multiply,
            ] {
                let scalar = |value| Literal::Scalar { value };
                let mut track = Track {
                    id: ObjectId::new(),
                    ty: Type::Scalar,
                    combination,
                    base: (combination != Combination::Replace).then(|| scalar(3.)),
                    extensions: Default::default(),
                    keys: [0, 2]
                        .into_iter()
                        .map(|n| Key {
                            id: ObjectId::new(),
                            time: Time::new(n, 1).unwrap(),
                            value: scalar(n as f64 * 10.),
                            interpolation,
                            handles: Default::default(),
                            extensions: [("vendor".into(), serde_json::json!({"kept":true}))]
                                .into(),
                        })
                        .collect(),
                };
                let before = track.clone();
                insert(&mut track, Time::new(1, 1).unwrap()).unwrap();
                assert_eq!(track.keys[1].interpolation, interpolation);
                assert_eq!(track.keys[0], before.keys[0]);
                assert_eq!(track.keys[2], before.keys[1]);
                for numerator in [-1, 0, 1, 2, 3, 4, 5] {
                    let time = Time::new(numerator, 2).unwrap();
                    assert_eq!(track.sample(time).unwrap(), before.sample(time).unwrap());
                }
            }
        }
    }
    #[test]
    fn duplicate_key_time_blocks_finish_until_the_draft_is_corrected() {
        let _guard = super::super::UI_TEST_LOCK.lock().unwrap();
        let mut context = fold_ui::sdk::imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([400., 800.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut track = Track {
            id: ObjectId::new(),
            ty: Type::Scalar,
            combination: Combination::Replace,
            base: None,
            extensions: Default::default(),
            keys: [0., 1.]
                .into_iter()
                .map(|value| Key {
                    id: ObjectId::new(),
                    time: Time::ZERO,
                    value: Literal::Scalar { value },
                    interpolation: Interpolation::Linear,
                    handles: Default::default(),
                    extensions: Default::default(),
                })
                .collect(),
        };
        let mut controls = literal::Controls::default();
        for corrected in [false, true] {
            if corrected {
                track.keys[1].time = Time::new(1, 3).unwrap();
            }
            let before = track.clone();
            controls.invalid = false;
            let mut response = EditResponse {
                finished: true,
                ..Default::default()
            };
            let ui = context.frame();
            ui.window("Keys")
                .size([260., 700.], fold_ui::sdk::imgui::Condition::Always)
                .build(|| {
                    draw(
                        ui,
                        "Opacity",
                        &mut track,
                        Some(Time::ZERO),
                        &mut response,
                        &mut controls,
                    );
                });
            drop(context.render_legacy());
            assert_eq!(track, before);
            assert_eq!(response.finished, corrected);
            assert_eq!(controls.invalid, !corrected);
        }
    }
}
