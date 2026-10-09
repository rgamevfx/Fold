//! Typed key editing without splitting native key identity into component curves.
use super::literal;
use crate::binding::{Combination, Interpolation, Key, Track};
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
        extensions: Default::default(),
    });
    track.keys.sort_by_key(|key| key.time);
    Ok(())
}

fn after_last(track: &Track) -> Option<Time> {
    track
        .keys
        .last()?
        .time
        .checked_add(Time::new(1, 1).expect("one second"))
        .ok()
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
    ui.text(label);
    let label = |combination| match combination {
        Combination::Replace => "Replace",
        Combination::Add => "Add to base",
        Combination::Multiply => "Multiply base",
    };
    if let Some(_menu) = ui.begin_combo("Combination", label(track.combination)) {
        for combination in [
            Combination::Replace,
            Combination::Add,
            Combination::Multiply,
        ] {
            let _disabled = ui.begin_disabled_with_cond(!supports(track, combination));
            if ui
                .selectable_config(label(combination))
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
    }
    if let Some(base) = &mut track.base {
        let _base = ui.push_id("base");
        literal::draw(ui, "Base", base, response, controls);
    }
    let mut removed = None;
    let can_remove = track.keys.len() > 1;
    for (index, key) in track.keys.iter_mut().enumerate() {
        let _key = ui.push_id(&key.id.to_string());
        let title = format!(
            "Key · {}/{} s",
            key.time.numerator(),
            key.time.denominator()
        );
        if let Some(_tree) = ui
            .tree_node_config("key")
            .label(&title)
            .default_open(index == 0)
            .push()
        {
            let invalid_before = controls.invalid;
            literal::rational(ui, "Time · seconds", &mut key.time, response);
            literal::draw(ui, "Value", &mut key.value, response, controls);
            let current = match key.interpolation {
                Interpolation::Linear => "Linear",
                Interpolation::Hold => "Hold",
            };
            if let Some(_menu) = ui.begin_combo("Interpolation", current) {
                for (name, value) in [
                    ("Linear", Interpolation::Linear),
                    ("Hold", Interpolation::Hold),
                ] {
                    let clicked = ui
                        .selectable_config(name)
                        .selected(key.interpolation == value)
                        .build();
                    let changed = clicked && key.interpolation != value;
                    response.item(ui, changed);
                    if changed {
                        key.interpolation = value;
                    }
                }
            }
            let _disabled = ui.begin_disabled_with_cond(!can_remove);
            if ui.small_button("Remove key") {
                removed = Some(key.id);
                controls.reset_draft(ui);
                controls.invalid = invalid_before;
            }
        }
    }
    if let Some(id) = removed {
        track.keys.retain(|key| key.id != id);
        response.changed = true;
        response.finished = true;
    }
    track.keys.sort_by_key(|key| key.time);
    let error = track.validate().err();
    {
        let insertion_time = time.or_else(|| after_last(track));
        let _disabled = ui.begin_disabled_with_cond(
            error.is_some()
                || controls.invalid
                || track.keys.len() >= 4096
                || insertion_time.is_none()
                || track
                    .keys
                    .iter()
                    .any(|key| Some(key.time) == insertion_time),
        );
        let label = if time.is_some() {
            "Add key at playhead"
        } else {
            "Add key after last"
        };
        if ui.button(label)
            && let Some(time) = insertion_time
        {
            match insert(track, time) {
                Ok(()) => {
                    response.changed = true;
                    response.finished = true;
                }
                Err(error) => {
                    ui.text_wrapped(error);
                }
            }
        }
        if time.is_none() && ui.is_item_hovered() {
            ui.tooltip_text(
                "Add a key one second after the final key, then edit its Time for exact placement.",
            );
        }
    }
    if let Some(error) = error {
        ui.text_wrapped(error);
        controls.invalid = true;
        response.finished = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Literal, Space, Type, Unit};
    #[test]
    fn root_insert_button_finishes_once_and_respects_disabled_state() {
        use fold_ui::sdk::imgui::{Condition, Context, MouseButton};
        fn frame(
            context: &mut Context,
            track: &mut Track,
            locked: bool,
        ) -> (EditResponse, [f32; 2]) {
            let mut response = EditResponse::default();
            let mut point = [0.; 2];
            let ui = context.frame();
            ui.window("Root keys")
                .position([0.; 2], Condition::Always)
                .size([260., 700.], Condition::Always)
                .build(|| {
                    let _locked = ui.begin_disabled_with_cond(locked);
                    draw(
                        ui,
                        "Width",
                        track,
                        None,
                        &mut response,
                        &mut literal::Controls::default(),
                    );
                    let min = ui.item_rect_min();
                    let max = ui.item_rect_max();
                    point = [(min[0] + max[0]) / 2., (min[1] + max[1]) / 2.];
                });
            drop(context.render_legacy());
            (response, point)
        }
        let _guard = super::super::UI_TEST_LOCK.lock().unwrap();
        let mut context = Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([400., 800.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut track = Track::constant(Literal::Scalar { value: 4. }, Time::ZERO).unwrap();
        for locked in [true, false] {
            frame(&mut context, &mut track, locked);
            let (_, point) = frame(&mut context, &mut track, locked);
            let before = track.clone();
            context.io_mut().add_mouse_pos_event(point);
            frame(&mut context, &mut track, locked);
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, true);
            let (pressed, _) = frame(&mut context, &mut track, locked);
            assert!(!pressed.changed);
            context
                .io_mut()
                .add_mouse_button_event(MouseButton::Left, false);
            let (released, _) = frame(&mut context, &mut track, locked);
            assert_eq!(released.changed, !locked);
            assert_eq!(released.finished, !locked);
            if locked {
                assert_eq!(track, before);
            } else {
                assert_eq!(track.keys.len(), 2);
                assert_eq!(track.keys[0], before.keys[0]);
                assert_eq!(track.keys[1].time, Time::new(1, 1).unwrap());
            }
            assert!(!frame(&mut context, &mut track, locked).0.changed);
        }
    }
    #[test]
    fn root_insertion_uses_exact_time_and_rejects_overflow() {
        let mut track =
            Track::constant(Literal::Scalar { value: 4. }, Time::new(-1, 3).unwrap()).unwrap();
        track.keys[0].interpolation = Interpolation::Hold;
        let before = track.clone();
        let time = after_last(&track).unwrap();
        assert_eq!(time, Time::new(2, 3).unwrap());
        insert(&mut track, time).unwrap();
        assert_eq!(track.keys[0], before.keys[0]);
        assert_ne!(track.keys[0].id, track.keys[1].id);
        assert_eq!(track.keys[1].interpolation, Interpolation::Hold);
        assert_eq!(track.sample(time).unwrap(), Literal::Scalar { value: 4. });
        track.keys[1].time = Time::new(i64::MAX, 1).unwrap();
        assert_eq!(after_last(&track), None);
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
