//! Instance inputs retain their definition defaults and native override identities.
use super::{keyframes, literal};
use crate::{
    binding::Binding,
    model::{Input, Instance, Parameter},
};
use fold_foundation::ObjectId;
use fold_ui::sdk::{self, EditResponse, imgui::Ui};
use std::collections::BTreeSet;

fn animate(
    instance: &mut Instance,
    input: &Input,
    time: fold_foundation::Time,
) -> Result<(), String> {
    let value = match instance
        .overrides
        .get(&input.id)
        .map(Parameter::binding)
        .transpose()?
    {
        None => input.default.clone(),
        Some(Binding::Literal { value }) => value,
        _ => return Err("Only a literal input can start animation".into()),
    };
    let binding = Binding::Track {
        track: crate::binding::Track::constant(value, time)?,
    };
    if let Some(parameter) = instance.overrides.get_mut(&input.id) {
        parameter.value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
    } else {
        instance
            .overrides
            .insert(input.id, Parameter::new(binding)?);
    }
    Ok(())
}

pub(super) fn draw(
    ui: &Ui,
    instance: &mut Instance,
    inputs: &[Input],
    locked: &BTreeSet<ObjectId>,
    parent_time: Option<fold_foundation::Time>,
    response: &mut EditResponse,
    controls: &mut literal::Controls,
) {
    let _instance = ui.push_id(&instance.id.to_string());
    ui.text(&instance.name);
    for input in inputs {
        let _input = ui.push_id(&input.id.to_string());
        let overridden = instance.overrides.contains_key(&input.id);
        let protected = locked.contains(&instance.id)
            || locked.contains(&input.id)
            || instance
                .overrides
                .get(&input.id)
                .is_some_and(|p| locked.contains(&p.id));
        let _locked = ui.begin_disabled_with_cond(protected);
        let mut edit = EditResponse::default();
        ui.text_disabled(if protected {
            "Locked"
        } else if overridden {
            "Instance override"
        } else {
            "Asset default"
        });
        ui.same_line();
        if let Some(_menu) = sdk::toolbar::icon_menu(ui, "input-options", "Instance input options")
        {
            if overridden {
                if ui.menu_item("Use asset default") {
                    instance.overrides.remove(&input.id);
                    edit.changed = true;
                    edit.finished = true;
                }
            } else if ui.menu_item("Override for this instance") {
                instance.overrides.insert(
                    input.id,
                    Parameter::new(Binding::Literal {
                        value: input.default.clone(),
                    })
                    .expect("native default"),
                );
                edit.changed = true;
                edit.finished = true;
            }
            let can_animate = input.ty.is_interpolatable()
                && instance.overrides.get(&input.id).is_none_or(|parameter| {
                    matches!(parameter.binding(), Ok(Binding::Literal { .. }))
                });
            let _disabled = ui.begin_disabled_with_cond(!can_animate);
            if ui.menu_item(if parent_time.is_some() {
                "Animate at playhead"
            } else {
                "Animate at local time zero"
            }) {
                match animate(
                    instance,
                    input,
                    parent_time.unwrap_or(fold_foundation::Time::ZERO),
                ) {
                    Ok(()) => {
                        edit.changed = true;
                        edit.finished = true;
                    }
                    Err(error) => ui.text_wrapped(error),
                }
            }
        }
        if edit.changed {
            controls.reset_draft(ui);
        }
        let overridden = instance.overrides.contains_key(&input.id);
        let binding = instance.overrides.get(&input.id).map(Parameter::binding);
        {
            let _inherited = ui.begin_disabled_with_cond(!overridden);
            match binding {
                None | Some(Ok(Binding::Literal { .. })) => {
                    let mut value = match binding {
                        Some(Ok(Binding::Literal { value })) => value,
                        _ => input.default.clone(),
                    };
                    literal::draw(ui, &input.name, &mut value, &mut edit, controls);
                    if edit.changed
                        && let Some(parameter) = instance.overrides.get_mut(&input.id)
                    {
                        parameter.value = serde_json::to_value(Binding::Literal { value })
                            .expect("native literal");
                    }
                }
                Some(Ok(Binding::Track { mut track })) => {
                    keyframes::draw(
                        ui,
                        &input.name,
                        &mut track,
                        parent_time,
                        &mut edit,
                        controls,
                    );
                    if edit.changed
                        && let Some(parameter) = instance.overrides.get_mut(&input.id)
                    {
                        parameter.value =
                            serde_json::to_value(Binding::Track { track }).expect("native track");
                    }
                }
                Some(Ok(_)) => ui.text(format!("{} · Driven", input.name)),
                Some(Err(_)) => ui.text(format!("{} · Unresolved", input.name)),
            }
        }
        response.began |= edit.began;
        response.changed |= edit.changed;
        response.finished |= edit.finished;
    }
    let _locked = ui.begin_disabled_with_cond(locked.contains(&instance.id));
    let open = ui.collapsing_header("Local timing", sdk::imgui::TreeNodeFlags::DEFAULT_OPEN);
    if ui.is_item_hovered() {
        ui.tooltip_text("Local time = (parent time − offset) × speed. Zero speed freezes at local time zero; negative speed plays in reverse.");
    }
    if open {
        {
            let _offset = ui.push_id("offset");
            literal::rational(ui, "Offset · seconds", &mut instance.clock.offset, response);
        }
        {
            let _rate = ui.push_id("rate");
            let mut rate = fold_foundation::Time::new(
                instance.clock.rate.numerator(),
                instance.clock.rate.denominator(),
            )
            .expect("native rate");
            literal::rational(ui, "Speed · multiplier", &mut rate, response);
            instance.clock.rate = crate::types::Ratio::new(rate.numerator(), rate.denominator())
                .expect("positive denominator");
            if rate.numerator() == 0 {
                ui.text_disabled("Frozen");
            } else if rate.numerator() < 0 {
                ui.text_disabled("Reverse");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::types::{Clock, Literal};
    use fold_foundation::Time;
    #[test]
    fn animate_retains_override_metadata_and_exact_parent_time() {
        let input = Input {
            id: ObjectId::new(),
            name: "Amount".into(),
            ty: crate::types::Type::Scalar,
            default: Literal::Scalar { value: 3. },
            protected: false,
            extensions: Default::default(),
        };
        let mut instance = Instance {
            id: ObjectId::new(),
            name: "Child".into(),
            definition: ObjectId::new(),
            clock: Clock::default(),
            overrides: Default::default(),
            extensions: Default::default(),
        };
        let mut parameter = Parameter::new(Binding::Literal {
            value: Literal::Scalar { value: 9. },
        })
        .unwrap();
        parameter
            .extensions
            .insert("vendor".into(), serde_json::json!({"preserve": true}));
        instance.overrides.insert(input.id, parameter.clone());
        let time = Time::new(-1001, 24000).unwrap();
        animate(&mut instance, &input, time).unwrap();
        let actual = &instance.overrides[&input.id];
        assert_eq!(actual.id, parameter.id);
        assert_eq!(actual.extensions, parameter.extensions);
        let Binding::Track { track } = actual.binding().unwrap() else {
            panic!("track")
        };
        assert_eq!(track.keys[0].time, time);
        assert_eq!(
            track.sample(Time::ZERO).unwrap(),
            Literal::Scalar { value: 9. }
        );
        let unchanged = instance.clone();
        assert!(animate(&mut instance, &input, time).is_err());
        assert_eq!(instance, unchanged);
    }
}
