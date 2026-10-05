//! Named path attributes with explicit normalized positions and typed values.
use crate::{
    fields::Datum,
    geometry::path::{Attributes, Stop},
};
use fold_ui::sdk::{
    EditResponse,
    imgui::{Drag, Ui},
};
pub(super) fn draw(
    ui: &Ui,
    settings: &mut serde_json::Value,
    aces: bool,
    response: &mut EditResponse,
    node: fold_foundation::ObjectId,
    drafts: &mut std::collections::BTreeMap<(fold_foundation::ObjectId, String), String>,
) {
    if !ui.collapsing_header(
        "Path attributes",
        fold_ui::sdk::imgui::TreeNodeFlags::empty(),
    ) {
        return;
    }
    let mut attributes: Attributes = serde_json::from_value(
        settings
            .get("attributes")
            .cloned()
            .unwrap_or(serde_json::json!({})),
    )
    .unwrap_or_default();
    let mut changed = false;
    if let Some(_menu) =
        fold_ui::sdk::toolbar::menu_button(ui, "Add attribute", "Add a named path attribute")
    {
        for (label, key, values) in [
            (
                "Color",
                "color",
                [
                    Datum::Color([0.08, 0.7, 1., 1.]),
                    Datum::Color([1., 0.15, 0.4, 1.]),
                ],
            ),
            ("Number", "value", [Datum::Scalar(0.), Datum::Scalar(1.)]),
            (
                "Vector",
                "vector",
                [Datum::Vector([0.; 2]), Datum::Vector([1.; 2])],
            ),
        ] {
            if ui.menu_item(label) {
                let mut name = key.to_string();
                let mut suffix = 2;
                while attributes.contains_key(&name) {
                    name = format!("{key} {suffix}");
                    suffix += 1;
                }
                attributes.insert(
                    name,
                    values
                        .into_iter()
                        .enumerate()
                        .map(|(i, value)| Stop {
                            position: i as f64,
                            value,
                            extensions: Default::default(),
                        })
                        .collect(),
                );
                changed = true;
                response.finished = true;
            }
        }
    }
    let mut remove_attribute = None;
    let mut rename = None;
    for (name, stops) in &mut attributes {
        let _id = ui.push_id(name);
        let label = drafts
            .entry((node, name.clone()))
            .or_insert_with(|| name.clone());
        ui.input_text("Attribute", label).build();
        if ui.is_item_deactivated_after_edit() {
            rename = Some((name.clone(), label.clone()));
            changed = true;
            response.finished = true;
        }
        let mut remove = None;
        let positions: Vec<_> = stops.iter().map(|s| s.position).collect();
        for (i, stop) in stops.iter_mut().enumerate() {
            let _id = ui.push_id(i as i32);
            changed |= Drag::new("Along path")
                .speed(0.002)
                .range(
                    if i == 0 { 0. } else { positions[i - 1] },
                    positions.get(i + 1).copied().unwrap_or(1.),
                )
                .build(ui, &mut stop.position);
            response.finished |= ui.is_item_deactivated_after_edit();
            match &mut stop.value {
                Datum::Color(color) => {
                    let mut edit = EditResponse::default();
                    fold_ui::sdk::color_controls::authored(ui, color, aces, &mut edit);
                    changed |= edit.changed;
                    response.finished |= edit.finished;
                }
                Datum::Scalar(value) => {
                    changed |= Drag::new("Value").speed(0.01).build(ui, value);
                    response.finished |= ui.is_item_deactivated_after_edit();
                }
                Datum::Vector(value) => {
                    changed |= Drag::new("Vector").speed(0.01).build_array(ui, value);
                    response.finished |= ui.is_item_deactivated_after_edit();
                }
                _ => {
                    ui.text_disabled("Discrete attribute");
                }
            }
            if ui.small_button("Remove sample") {
                remove = Some(i);
            }
        }
        if let Some(i) = remove
            && stops.len() > 1
        {
            stops.remove(i);
            changed = true;
            response.finished = true;
        }
        if ui.small_button("Insert sample") && stops.len() < 1024 {
            stops.sort_by(|a, b| a.position.total_cmp(&b.position));
            if stops.len() == 1 {
                let position = if stops[0].position < 0.5 { 1. } else { 0. };
                stops.push(Stop {
                    position,
                    value: stops[0].value.clone(),
                    extensions: Default::default(),
                });
                stops.sort_by(|a, b| a.position.total_cmp(&b.position));
                changed = true;
                response.finished = true;
            } else if let Some(pair) = stops.windows(2).max_by(|a, b| {
                (a[1].position - a[0].position).total_cmp(&(b[1].position - b[0].position))
            }) {
                let position = (pair[0].position + pair[1].position) * 0.5;
                let value = crate::geometry::path::attribute(stops, position).unwrap();
                stops.push(Stop {
                    position,
                    value,
                    extensions: Default::default(),
                });
                changed = true;
                response.finished = true;
            }
        }
        ui.same_line();
        if ui.small_button("Remove attribute") {
            remove_attribute = Some(name.clone());
        }
        stops.sort_by(|a, b| a.position.total_cmp(&b.position));
    }
    if let Some(name) = remove_attribute {
        attributes.remove(&name);
        changed = true;
        response.finished = true;
    }
    if let Some((old, new)) = rename
        && old != new
        && !new.is_empty()
        && !attributes.contains_key(&new)
    {
        let values = attributes.remove(&old).unwrap();
        attributes.insert(new, values);
    }
    if changed {
        settings["attributes"] = serde_json::to_value(attributes).unwrap();
        response.changed = true;
    }
}
