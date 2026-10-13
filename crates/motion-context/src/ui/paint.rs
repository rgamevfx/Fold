//! Artist controls for native paint; model validation and transactions stay authoritative.
use fold_foundation::ObjectId;
use fold_motion_canvas::model::{PaintSource, Spread, Stop};
use fold_ui::sdk::{
    self, EditResponse,
    imgui::{TreeNodeFlags, Ui},
};

pub(super) fn color(ui: &Ui, value: &mut [f64; 4], response: &mut EditResponse) {
    let mut linear = fold_color::authored::from_picker(*value, false);
    let before = linear;
    sdk::color_controls::authored(ui, &mut linear, false, response);
    if linear != before {
        if linear[..3] == before[..3] {
            value[3] = linear[3];
        } else {
            *value = fold_color::authored::to_picker(linear, false).map(|v| v.clamp(0., 1.));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn gradient_controls_keep_precision_and_stop_identity_at_two_widths() {
        let mut context = sdk::imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_delta_time(1. / 60.);
        let original = PaintSource::Linear {
            start: [0.1234567890123, 3.],
            end: [100., 20.],
            transform: [1., 0., 0.2, 1., 0., 0.],
            spread: Spread::Reflect,
            stops: vec![
                Stop {
                    id: ObjectId::new(),
                    offset: 0.,
                    color: [0.123456789, 0., 1., 0.5],
                    extensions: Default::default(),
                },
                Stop {
                    id: ObjectId::new(),
                    offset: 1.,
                    color: [1.; 4],
                    extensions: Default::default(),
                },
            ],
            extensions: Default::default(),
        };
        let mut radial = original.clone();
        change_type(&mut radial, 2);
        assert_eq!(radial.stops(), original.stops());
        assert_eq!(radial.extensions(), original.extensions());
        for original in [original, radial] {
            for width in [640., 260.] {
                context.io_mut().set_display_size([800., 900.]);
                let mut fill = Some(original.clone());
                let mut stroke = Some(original.clone());
                let mut response = EditResponse::default();
                let ui = context.frame();
                ui.window("Gradient controls")
                    .size([width, 800.], sdk::imgui::Condition::Always)
                    .build(|| {
                        draw(ui, "Fill", None, &mut fill, &mut response);
                        draw(ui, "Stroke", None, &mut stroke, &mut response);
                    });
                drop(context.render_legacy());
                assert_eq!(fill, Some(original.clone()));
                assert_eq!(stroke, Some(original.clone()));
                assert!(!response.changed);
            }
        }
    }
}
pub(super) fn draw(
    ui: &Ui,
    label: &str,
    host: Option<&dyn fold_platform::desktop::DesktopClient>,
    value: &mut Option<PaintSource>,
    response: &mut EditResponse,
) {
    let _id = ui.push_id(label);
    let mut enabled = value.is_some();
    let changed = ui.checkbox(label, &mut enabled);
    response.item(ui, changed);
    if changed {
        *value = enabled.then(|| [0.35, 0.2, 0.95, 1.].into());
    }
    let Some(source) = value else {
        return;
    };
    let mut kind = match source {
        PaintSource::Solid { .. } => 0,
        PaintSource::Linear { .. } => 1,
        PaintSource::Radial { .. } => 2,
        PaintSource::ImagePattern { .. } => 3,
    };
    let labels = [
        "Solid",
        "Linear gradient",
        "Radial gradient",
        "Image pattern",
    ];
    let image = host.and_then(super::image_control::first);
    let mut changed = false;
    if let Some(_combo) = ui.begin_combo("Type", labels[kind]) {
        for (index, label) in labels.iter().enumerate() {
            let _disabled = ui.begin_disabled_with_cond(index == 3 && image.is_none());
            if ui.selectable_config(*label).selected(index == kind).build() && index != kind {
                kind = index;
                changed = true;
            }
        }
    }
    if changed {
        if kind == 3 {
            *source = PaintSource::ImagePattern {
                asset: image.unwrap(),
                origin: [0.; 2],
                size: [100.; 2],
                transform: [1., 0., 0., 1., 0., 0.],
                extensions: source.extensions().clone(),
            };
        } else {
            change_type(source, kind);
        }
        response.changed = true;
        response.finished = true;
    }
    let number = |ui: &Ui, label: &str, value: &mut f64, range, response: &mut EditResponse| {
        sdk::NumericProperty {
            id: label,
            label,
            unit: "",
            speed: 0.05,
            range,
            default: None,
        }
        .draw(ui, value, response);
    };
    match source {
        PaintSource::ImagePattern {
            asset,
            origin,
            size,
            transform,
            ..
        } => {
            if let Some(host) = host {
                super::image_control::draw(ui, host, "Image", asset, response);
            }
            for (label, value) in ["Tile width", "Tile height"].into_iter().zip(size) {
                number(ui, label, value, Some([1e-6, 1e6]), response);
            }
            if ui.collapsing_header("Pattern transform", TreeNodeFlags::empty()) {
                for (label, value) in ["Origin X", "Origin Y"].into_iter().zip(origin) {
                    number(ui, label, value, None, response);
                }
                for (label, value) in ["XX", "YX", "XY", "YY", "X", "Y"]
                    .into_iter()
                    .zip(transform)
                {
                    number(ui, label, value, None, response);
                }
            }
            return;
        }
        PaintSource::Solid { color: value, .. } => {
            color(ui, value, response);
            return;
        }
        PaintSource::Linear { start, end, .. } => {
            for (label, value) in ["Start X", "Start Y"].into_iter().zip(start.iter_mut()) {
                number(ui, label, value, None, response);
            }
            for (label, value) in ["End X", "End Y"].into_iter().zip(end.iter_mut()) {
                number(ui, label, value, None, response);
            }
        }
        PaintSource::Radial {
            center,
            focus,
            radius,
            ..
        } => {
            for (label, value) in ["Center X", "Center Y"].into_iter().zip(center.iter_mut()) {
                number(ui, label, value, None, response);
            }
            number(ui, "Radius", radius, Some([1e-6, 1e6]), response);
            if ui.collapsing_header("Focus", TreeNodeFlags::empty()) {
                for (label, value) in ["Focus X", "Focus Y"].into_iter().zip(focus.iter_mut()) {
                    number(ui, label, value, None, response);
                }
            }
        }
    }
    match source {
        PaintSource::Linear {
            transform,
            spread,
            stops,
            ..
        }
        | PaintSource::Radial {
            transform,
            spread,
            stops,
            ..
        } => {
            let spreads = [Spread::Pad, Spread::Reflect, Spread::Repeat];
            let mut index = spreads.iter().position(|v| v == spread).unwrap();
            let changed =
                ui.combo_simple_string("Spread", &mut index, &["Pad", "Reflect", "Repeat"]);
            response.item(ui, changed);
            *spread = spreads[index];
            if ui.collapsing_header("Gradient transform", TreeNodeFlags::empty()) {
                for (label, value) in ["XX", "YX", "XY", "YY", "X", "Y"]
                    .into_iter()
                    .zip(transform.iter_mut())
                {
                    number(ui, label, value, None, response);
                }
            }
            let mut remove = None;
            let count = stops.len();
            for index in 0..count {
                let min = if index == 0 {
                    0.
                } else {
                    stops[index - 1].offset
                };
                let max = if index + 1 == count {
                    1.
                } else {
                    stops[index + 1].offset
                };
                let stop = &mut stops[index];
                let _id = ui.push_id(&stop.id.to_string());
                if ui.collapsing_header(format!("Stop {}", index + 1), TreeNodeFlags::DEFAULT_OPEN)
                {
                    number(ui, "Position", &mut stop.offset, Some([min, max]), response);
                    color(ui, &mut stop.color, response);
                    if count > 2 && ui.small_button("Remove stop") {
                        remove = Some(index);
                    }
                }
            }
            if let Some(index) = remove {
                stops.remove(index);
                response.changed = true;
                response.finished = true;
            }
            if stops.len() < 256 && ui.small_button("Add stop") {
                let last = stops.last().unwrap();
                let offset = (stops[stops.len() - 2].offset + last.offset) / 2.;
                let previous = &stops[stops.len() - 2];
                let color = std::array::from_fn(|channel| {
                    (previous.color[channel] + last.color[channel]) / 2.
                });
                stops.insert(
                    stops.len() - 1,
                    Stop {
                        id: ObjectId::new(),
                        offset,
                        color,
                        extensions: Default::default(),
                    },
                );
                response.changed = true;
                response.finished = true;
            }
        }
        PaintSource::ImagePattern { .. } | PaintSource::Solid { .. } => unreachable!(),
    }
}

fn change_type(source: &mut PaintSource, kind: usize) {
    let base = match source {
        PaintSource::ImagePattern { .. } => [0.35, 0.2, 0.95, 1.],
        PaintSource::Solid { color, .. } => *color,
        PaintSource::Linear { stops, .. } | PaintSource::Radial { stops, .. } => stops[0].color,
    };
    let extensions = source.extensions().clone();
    if kind == 0 {
        *source = PaintSource::Solid {
            color: base,
            extensions,
        };
        return;
    }
    let (start, end, center, radius) = match source {
        PaintSource::ImagePattern { .. } | PaintSource::Solid { .. } => {
            ([0.; 2], [100., 0.], [50.; 2], 50.)
        }
        PaintSource::Linear { start, end, .. } => (
            *start,
            *end,
            std::array::from_fn(|i| start[i] / 2. + end[i] / 2.),
            (end[0] - start[0]).hypot(end[1] - start[1]) / 2.,
        ),
        PaintSource::Radial { center, radius, .. } => (
            [center[0] - *radius, center[1]],
            [center[0] + *radius, center[1]],
            *center,
            *radius,
        ),
    };
    let (transform, spread, stops) = match source {
        PaintSource::ImagePattern { .. } | PaintSource::Solid { .. } => (
            [1., 0., 0., 1., 0., 0.],
            Spread::Pad,
            vec![
                Stop {
                    id: ObjectId::new(),
                    offset: 0.,
                    color: base,
                    extensions: Default::default(),
                },
                Stop {
                    id: ObjectId::new(),
                    offset: 1.,
                    color: [1.; 4],
                    extensions: Default::default(),
                },
            ],
        ),
        PaintSource::Linear {
            transform,
            spread,
            stops,
            ..
        }
        | PaintSource::Radial {
            transform,
            spread,
            stops,
            ..
        } => (*transform, *spread, stops.clone()),
    };
    *source = if kind == 1 {
        PaintSource::Linear {
            start,
            end,
            transform,
            spread,
            stops,
            extensions,
        }
    } else {
        PaintSource::Radial {
            center,
            focus: center,
            radius,
            transform,
            spread,
            stops,
            extensions,
        }
    };
}
