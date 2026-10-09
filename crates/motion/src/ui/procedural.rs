//! Attribute schema controls share the ordinary parameter transaction lifecycle.
use crate::{
    binding::Binding,
    model::Node,
    types::{Literal, Space, Unit},
};
use fold_foundation::ObjectId;
use fold_ui::sdk::{EditResponse, imgui::Ui};
use std::collections::BTreeSet;
pub(super) fn schema(
    ui: &Ui,
    node: &mut Node,
    locked: &BTreeSet<ObjectId>,
    response: &mut EditResponse,
) {
    let random = node.operation == "fold.motion.attribute_randomize";
    if !random && node.operation != "fold.motion.attribute_create" {
        return;
    }
    let names = if random {
        vec!["minimum", "maximum"]
    } else {
        vec!["value"]
    };
    let Some(parameter) = node.parameters.get(names[0]) else {
        return;
    };
    let Ok(Binding::Literal { value }) = parameter.binding() else {
        return;
    };
    let current = match value {
        Literal::Scalar { .. } => "Float",
        Literal::Integer { .. } => "Integer",
        Literal::Vector {
            unit: Unit::Meters, ..
        } => "Position",
        Literal::Vector { .. } => "Vector",
        Literal::Rotation { .. } => "Quaternion",
        Literal::Bool { .. } => "Boolean",
        Literal::Text { .. } => "Text",
        _ => return,
    };
    let disabled = names.iter().any(|name| {
        node.parameters
            .get(*name)
            .is_some_and(|p| locked.contains(&p.id))
    });
    let _id = ui.push_id(&node.id.to_string());
    let _disabled = ui.begin_disabled_with_cond(disabled);
    ui.text("Attribute Type");
    if let Some(_combo) = ui.begin_combo("##attribute-type", current) {
        for choice in [
            "Float",
            "Integer",
            "Vector",
            "Position",
            "Quaternion",
            "Boolean",
            "Text",
        ] {
            if random && matches!(choice, "Boolean" | "Text") {
                continue;
            }
            if ui
                .selectable_config(choice)
                .selected(choice == current)
                .build()
                && choice != current
            {
                for (index, name) in names.iter().enumerate() {
                    let v = index as f64;
                    let value = match choice {
                        "Float" => Literal::Scalar { value: v },
                        "Integer" => Literal::Integer {
                            value: index as i64,
                        },
                        "Vector" | "Position" => Literal::Vector {
                            value: vec![v; 3],
                            unit: if choice == "Position" {
                                Unit::Meters
                            } else {
                                Unit::Unitless
                            },
                            space: Space::World,
                        },
                        "Quaternion" => Literal::Rotation {
                            value: if index == 0 {
                                [0., 0., 0., 1.]
                            } else {
                                [0., 1., 0., 0.]
                            },
                            space: Space::World,
                        },
                        "Boolean" => Literal::Bool { value: false },
                        _ => Literal::Text {
                            value: String::new(),
                        },
                    };
                    if let Some(parameter) = node.parameters.get_mut(*name) {
                        parameter.value = serde_json::to_value(Binding::Literal { value })
                            .expect("literal binding");
                    }
                }
                response.began = true;
                response.changed = true;
                response.finished = true;
            }
        }
    }
}
