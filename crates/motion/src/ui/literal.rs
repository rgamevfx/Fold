//! Precision-preserving native literal controls. Color encodings remain explicit.
use crate::types::Literal;
use fold_ui::sdk::{EditResponse, NumericProperty, imgui::Ui};
#[derive(Default)]
pub(super) struct Controls {
    pub invalid: bool,
    pub assets: Vec<(fold_foundation::ObjectId, String)>,
    rotations: std::collections::HashMap<fold_ui::sdk::imgui::Id, Rotation>,
}
impl Controls {
    pub(super) fn reset_draft(&mut self, ui: &Ui) {
        self.rotations.remove(&ui.get_id("rotation"));
    }

    pub(super) fn set_assets(&mut self, value: &crate::model::Construction) {
        let mut counts = std::collections::BTreeMap::new();
        for asset in &value.definitions {
            *counts.entry(asset.name.as_str()).or_insert(0usize) += 1;
        }
        self.assets = value
            .definitions
            .iter()
            .enumerate()
            .map(|(index, asset)| {
                let label = if counts[asset.name.as_str()] > 1 {
                    format!("{} · Asset {}", asset.name, index + 1)
                } else {
                    asset.name.clone()
                };
                (asset.id, label)
            })
            .collect();
    }
}
struct Rotation {
    stored: [f64; 4],
    axis: [f64; 3],
    degrees: f64,
}
impl Rotation {
    fn new(stored: [f64; 4]) -> Self {
        let length = stored[0].hypot(stored[1]).hypot(stored[2]);
        let axis = if length == 0. {
            [0., 0., 1.]
        } else {
            [stored[0] / length, stored[1] / length, stored[2] / length]
        };
        Self {
            stored,
            axis,
            degrees: 2. * length.atan2(stored[3]).to_degrees(),
        }
    }
    fn quaternion(&self) -> Option<[f64; 4]> {
        if !self.degrees.is_finite() || !self.axis.iter().all(|v| v.is_finite()) {
            return None;
        }
        let max = self.axis.iter().fold(0f64, |a, b| a.max(b.abs()));
        if max == 0. {
            return None;
        }
        let scaled = self.axis.map(|v| v / max);
        let length = scaled[0].hypot(scaled[1]).hypot(scaled[2]);
        let angle = (self.degrees % 720.).to_radians() / 2.;
        let (s, c) = angle.sin_cos();
        Some([
            scaled[0] / length * s,
            scaled[1] / length * s,
            scaled[2] / length * s,
            c,
        ])
    }
}
/// Exact rational input shared by duration and instance clock controls.
pub(super) fn rational(
    ui: &Ui,
    label: &str,
    value: &mut fold_foundation::Time,
    response: &mut EditResponse,
) {
    ui.text(label);
    let mut numerator = value.numerator();
    let mut denominator = value.denominator();
    ui.text("Numerator");
    let _width = ui.push_item_width(ui.content_region_avail()[0].max(1.));
    let changed = ui.input_scalar("##numerator", &mut numerator).build();
    response.item(ui, changed);
    ui.text("Denominator");
    let rate_changed = fold_ui::sdk::imgui::Drag::new("##denominator")
        .speed(1.)
        .range(1u32, u32::MAX)
        .flags(fold_ui::sdk::imgui::DragFlags::ALWAYS_CLAMP)
        .build(ui, &mut denominator);
    response.item(ui, rate_changed);
    if changed || rate_changed {
        *value = fold_foundation::Time::new(numerator, denominator).expect("positive denominator");
    }
}
pub(super) fn draw(
    ui: &Ui,
    label: &str,
    value: &mut Literal,
    response: &mut EditResponse,
    controls: &mut Controls,
) {
    let number =
        |ui: &Ui, label: &str, value: &mut f64, unit: &str, response: &mut EditResponse| {
            NumericProperty {
                id: label,
                label,
                unit,
                speed: 0.1,
                range: None,
                default: None,
            }
            .draw(ui, value, response);
        };
    match value {
        Literal::Rotation { value, space } => {
            ui.text(label);
            ui.text_disabled(match space {
                crate::types::Space::Screen => "Screen · axis and angle",
                crate::types::Space::World => "World · axis and angle",
                crate::types::Space::CameraPlane => "Camera plane · axis and angle",
            });
            let draft = controls
                .rotations
                .entry(ui.get_id("rotation"))
                .or_insert_with(|| Rotation::new(*value));
            if draft.stored != *value {
                *draft = Rotation::new(*value);
            }
            let mut edit = EditResponse::default();
            number(ui, "Angle", &mut draft.degrees, "°", &mut edit);
            for (label, component) in ["Axis X", "Axis Y", "Axis Z"].iter().zip(&mut draft.axis) {
                number(ui, label, component, "", &mut edit);
            }
            response.began |= edit.began;
            response.changed |= edit.changed;
            response.finished |= edit.finished;
            if let Some(quaternion) = draft.quaternion() {
                if edit.changed {
                    *value = quaternion;
                    draft.stored = quaternion;
                }
            } else {
                ui.text_wrapped("The rotation axis needs a nonzero direction. Edit an axis component or press Escape to cancel.");
                controls.invalid = true;
                response.finished = false;
            }
        }
        Literal::Integer { value } => {
            ui.text(label);
            let _width = ui.push_item_width(ui.content_region_avail()[0].max(1.));
            let changed = ui.input_scalar("##integer", value).build();
            response.item(ui, changed);
        }
        Literal::Text { value } => {
            ui.text(label);
            let choices: Option<&[&str]> = match label {
                "Domain" | "Source Domain" | "Run Over" => {
                    Some(&["point", "primitive", "corner", "edge", "detail", "instance"])
                }
                "Sampling" => Some(&["nearest", "blend"]),
                _ => None,
            };
            if label == "Parameters" {
                if let Ok(serde_json::Value::Object(mut params)) = serde_json::from_str(value) {
                    let before = params.clone();
                    for (name, item) in &mut params {
                        let _id = ui.push_id(name);
                        match item {
                            serde_json::Value::Number(n) => {
                                if let Some(mut v) = n.as_i64() {
                                    ui.text(name);
                                    let changed = ui.input_scalar("##integer", &mut v).build();
                                    response.item(ui, changed);
                                    if changed {
                                        *item = serde_json::Value::Number(v.into());
                                    }
                                } else if let Some(mut v) = n.as_f64() {
                                    let before = v;
                                    number(ui, name, &mut v, "", response);
                                    if v != before
                                        && let Some(n) = serde_json::Number::from_f64(v)
                                    {
                                        *item = serde_json::Value::Number(n);
                                    }
                                }
                            }
                            serde_json::Value::Bool(v) => {
                                let changed = ui.checkbox(name, v);
                                response.item(ui, changed);
                            }
                            _ => {}
                        }
                    }
                    if params != before {
                        *value = serde_json::to_string(&params).expect("JSON parameters");
                    }
                }
                if ui.collapsing_header(
                    "Edit parameter definitions",
                    fold_ui::sdk::imgui::TreeNodeFlags::empty(),
                ) {
                    let changed = ui
                        .input_text_multiline(
                            "##definitions",
                            value,
                            [ui.content_region_avail()[0].max(1.), 100.],
                        )
                        .build();
                    response.item(ui, changed);
                    if serde_json::from_str::<serde_json::Map<String, serde_json::Value>>(value)
                        .is_err()
                    {
                        ui.text_wrapped("Enter a JSON object of named parameters.");
                    }
                }
            } else if let Some(choices) = choices {
                if let Some(_combo) = ui.begin_combo("##choice", value.as_str()) {
                    for choice in choices {
                        if ui
                            .selectable_config(*choice)
                            .selected(value == choice)
                            .build()
                        {
                            *value = (*choice).into();
                            response.began = true;
                            response.changed = true;
                            response.finished = true;
                        }
                    }
                }
            } else {
                let source = label == "Rhai Source";
                let changed = ui
                    .input_text_multiline(
                        "##text",
                        value,
                        [
                            ui.content_region_avail()[0].max(1.),
                            if source { 240. } else { 80. },
                        ],
                    )
                    .build();
                response.item(ui, changed);
            }
        }
        Literal::Duration { value } => {
            rational(ui, &format!("{label} · seconds"), value, response);
        }
        Literal::Color {
            value,
            encoding,
            alpha,
        } => {
            ui.text(label);
            ui.text_disabled(format!(
                "{} · {}",
                match encoding {
                    crate::types::Encoding::LinearRec709 => "Linear Rec.709",
                    crate::types::Encoding::AcesCg => "ACEScg",
                },
                match alpha {
                    crate::types::Alpha::Straight => "Straight alpha",
                    crate::types::Alpha::Premultiplied => "Premultiplied alpha",
                }
            ));
            for (index, label) in ["Red", "Green", "Blue", "Alpha"].iter().enumerate() {
                let zero_alpha = *alpha == crate::types::Alpha::Premultiplied && value[3] == 0.;
                let _disabled = ui.begin_disabled_with_cond(zero_alpha && index < 3);
                NumericProperty {
                    id: label,
                    label,
                    unit: "",
                    speed: 0.01,
                    range: (index == 3).then_some([0., 1.]),
                    default: None,
                }
                .draw(ui, &mut value[index], response);
            }
            if *alpha == crate::types::Alpha::Premultiplied && value[3] == 0. {
                value[..3].fill(0.);
                ui.text_disabled("RGB is zero at zero alpha");
            }
        }
        Literal::Scalar { value } => number(ui, label, value, "", response),
        Literal::Distance { value, unit } => number(
            ui,
            label,
            value,
            match unit {
                crate::types::Unit::Canvas => "px",
                crate::types::Unit::Meters => "m",
                _ => "",
            },
            response,
        ),
        Literal::Bool { value } => {
            let changed = ui.checkbox(label, value);
            response.item(ui, changed);
        }
        Literal::Vector { value, unit, .. } => {
            ui.text(label);
            for (index, value) in value.iter_mut().enumerate() {
                number(
                    ui,
                    ["X", "Y", "Z"].get(index).copied().unwrap_or("Component"),
                    value,
                    match unit {
                        crate::types::Unit::Canvas => "px",
                        crate::types::Unit::Meters => "m",
                        _ => "",
                    },
                    response,
                );
            }
        }
        Literal::AssetRef { value } => {
            use crate::types::AssetReference;
            ui.text(label);
            let current = match value {
                AssetReference::Definition(id) => controls
                    .assets
                    .iter()
                    .find(|(key, _)| key == id)
                    .map_or("Missing asset", |(_, name)| name.as_str()),
                AssetReference::Resource(_) => "External resource unavailable",
            };
            let _width = ui.push_item_width(ui.content_region_avail()[0].max(1.));
            if let Some(_menu) = ui.begin_combo("##asset", current) {
                if controls.assets.is_empty() {
                    ui.text_disabled("No native assets");
                }
                for (id, name) in &controls.assets {
                    let _id = ui.push_id(&id.to_string());
                    let target = AssetReference::Definition(*id);
                    let clicked = ui
                        .selectable_config(name)
                        .selected(*value == target)
                        .build();
                    let changed = clicked && *value != target;
                    response.item(ui, changed);
                    if changed {
                        *value = target;
                    }
                }
            }
            if ui.is_item_hovered() {
                ui.tooltip_text(current);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn axis_angle_preserves_orientation_and_handles_degenerate_inputs() {
        for q in [
            [0., 0., 0., 1.],
            [0., 0., 0., -1.],
            [1., 0., 0., 0.],
            [0.5, 0.5, 0.5, 0.5],
        ] {
            let result = Rotation::new(q).quaternion().unwrap();
            let dot: f64 = q.iter().zip(result).map(|(a, b)| a * b).sum();
            assert!((dot.abs() - 1.).abs() < 1e-12);
        }
        let mut draft = Rotation::new([0., 0., 0., 1.]);
        draft.axis = [1., 0., 0.];
        assert_eq!(draft.quaternion().unwrap(), [0., 0., 0., 1.]);
        draft.degrees = 180.;
        assert!((draft.quaternion().unwrap()[0] - 1.).abs() < 1e-12);
        draft.axis = [f64::MAX, f64::MAX, 0.];
        let q = draft.quaternion().unwrap();
        Literal::Rotation {
            value: q,
            space: crate::types::Space::World,
        }
        .validate()
        .unwrap();
        draft.axis = [0.; 3];
        assert!(draft.quaternion().is_none());
        draft.axis = [1., 0., 0.];
        draft.degrees = f64::NAN;
        assert!(draft.quaternion().is_none());
    }
    #[test]
    fn invalid_axis_keeps_the_last_valid_value_and_prevents_finish() {
        let _guard = super::super::UI_TEST_LOCK.lock().unwrap();
        let mut context = fold_ui::sdk::imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([400., 400.]);
        context.io_mut().set_delta_time(1. / 60.);
        let original = [0.5, 0.5, 0.5, 0.5];
        let mut value = Literal::Rotation {
            value: original,
            space: crate::types::Space::World,
        };
        let mut controls = Controls::default();
        let ui = context.frame();
        ui.window("Rotation").build(|| {
            controls.rotations.insert(
                ui.get_id("rotation"),
                Rotation {
                    stored: original,
                    axis: [0.; 3],
                    degrees: 120.,
                },
            );
            let mut response = EditResponse {
                finished: true,
                ..Default::default()
            };
            draw(ui, "Orientation", &mut value, &mut response, &mut controls);
            assert!(!response.finished);
            assert!(controls.invalid);
        });
        drop(context.render_legacy());
        assert_eq!(
            value,
            Literal::Rotation {
                value: original,
                space: crate::types::Space::World
            }
        );
    }
}
