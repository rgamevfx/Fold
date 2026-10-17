//! One control surface for native scripts and their placed source containers.
use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::model::{Control, ControlValue, Scalar};
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::{
    EditResponse,
    animated_property::AnimatedProperty,
    imgui::{TreeNodeFlags, Ui},
};
use std::collections::BTreeMap;

pub(super) fn scalar(
    ui: &Ui,
    host: &dyn DesktopClient,
    time: Time,
    label: &str,
    value: &mut Scalar,
    range: Option<[f64; 2]>,
    unit: &str,
    response: &mut EditResponse,
) {
    let identity = value.id.to_string();
    let path = format!("{identity}.0");
    let mut channels = BTreeMap::new();
    if let Some(curve) = value.animation.clone() {
        channels.insert(path.clone(), curve);
    }
    let _disabled = ui.begin_disabled_with_cond(value.protected);
    AnimatedProperty {
        id: &identity,
        label,
        components: &[""],
        unit,
        range,
        color: None,
    }
    .draw(
        ui,
        std::slice::from_mut(&mut value.value),
        &mut channels,
        time,
        host.state().auto_key,
        response,
    );
    value.animation = channels.remove(&path);
}

pub(super) fn draw(
    ui: &Ui,
    host: &mut dyn DesktopClient,
    time: Time,
    controls: &mut BTreeMap<ObjectId, Control>,
    response: &mut EditResponse,
) {
    let mut groups: Vec<String> = vec![];
    for control in controls.values() {
        let group = control
            .extensions
            .get("fold.control")
            .and_then(|v| v["group"].as_str())
            .unwrap_or("Controls");
        if !groups.iter().any(|g| g == group) {
            groups.push(group.into());
        }
    }
    groups.sort_by_key(|g| match g.as_str() {
        "Content" => 0,
        "Appearance" => 1,
        "Motion" => 2,
        _ => 3,
    });
    for group in groups {
        if !ui.collapsing_header(&group, TreeNodeFlags::DEFAULT_OPEN) {
            continue;
        }
        let mut values: Vec<_> = controls
            .iter_mut()
            .filter(|(_, c)| {
                c.extensions
                    .get("fold.control")
                    .and_then(|v| v["group"].as_str())
                    .unwrap_or("Controls")
                    == group
            })
            .collect();
        values.sort_by_key(|(_, c)| {
            c.extensions
                .get("fold.control")
                .and_then(|v| v["order"].as_u64())
                .unwrap_or(0)
        });
        for (id, control) in values {
            let _scope = ui.push_id(&id.to_string());
            let _disabled = ui.begin_disabled_with_cond(control.protected);
            let meta = control.extensions.get("fold.control");
            match &mut control.value {
                ControlValue::Number(value) => {
                    let range = meta
                        .and_then(|v| v["range"].as_array())
                        .and_then(|v| Some([v.first()?.as_f64()?, v.get(1)?.as_f64()?]));
                    scalar(
                        ui,
                        host,
                        time,
                        &control.name,
                        value,
                        range,
                        if control.name == "Start" { "s" } else { "" },
                        response,
                    );
                }
                ControlValue::Image(asset) => {
                    super::image_control::draw(ui, host, &control.name, asset, response)
                }
                ControlValue::Text(value) => {
                    let choices: Vec<_> = meta
                        .and_then(|v| v["choices"].as_array())
                        .into_iter()
                        .flatten()
                        .filter_map(|v| v.as_str())
                        .collect();
                    let changed = if choices.is_empty() {
                        if value.contains('\n')
                            || matches!(control.name.as_str(), "Words" | "Lines")
                        {
                            ui.input_text_multiline(
                                &control.name,
                                value,
                                [ui.content_region_avail()[0], ui.frame_height() * 3.],
                            )
                            .build()
                        } else {
                            ui.input_text(&control.name, value).build()
                        }
                    } else {
                        let mut index = choices.iter().position(|s| *s == value).unwrap_or(0);
                        let changed = ui.combo_simple_string(&control.name, &mut index, &choices);
                        if changed {
                            *value = choices[index].to_owned();
                        }
                        changed
                    };
                    response.item(ui, changed);
                }
                ControlValue::Boolean(value) => {
                    let changed = ui.checkbox(&control.name, value);
                    response.item(ui, changed);
                }
                ControlValue::Color(value) => {
                    ui.text(&control.name);
                    super::paint::color(ui, value, response);
                }
            }
        }
    }
}
