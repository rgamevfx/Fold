//! Read source, exact frame mapping and input-color controls.
use crate::{Node, Parameters, Source};
use fold_foundation::Time;
use fold_platform::desktop::{DesktopClient, ViewLocation};
use fold_ui::sdk::{
    EditResponse as Response,
    imgui::{Drag, Ui},
};

#[derive(Default)]
pub(super) struct Actions {
    pub choose_source: bool,
    pub navigate: Option<ViewLocation>,
}
pub(super) fn draw(
    ui: &Ui,
    node: &mut Node,
    host: &dyn DesktopClient,
    aces: bool,
    response: &mut Response,
) -> Actions {
    let mut actions = Actions::default();
    let Parameters::Read {
        source,
        start,
        source_start,
        duration,
    } = &mut node.parameters
    else {
        return actions;
    };
    if !matches!(source, Source::Document { .. }) && ui.button("Choose file…") {
        actions.choose_source = true;
    }
    frame_range(
        ui,
        source.info().rate,
        start,
        source_start,
        duration,
        response,
    );
    ui.separator();
    if let Source::Exr {
        image,
        color_layers,
        ..
    } = source
        && let Some(_advanced) = ui.tree_node("Color layers")
    {
        let layers: std::collections::BTreeSet<_> = image
            .channels
            .iter()
            .filter(|c| c.color)
            .filter_map(|c| c.name.rsplit_once('.').map(|(layer, _)| layer))
            .collect();
        for layer in layers {
            if !["red", "green", "blue"].iter().all(|component| {
                image
                    .channels
                    .iter()
                    .any(|c| c.name == format!("{layer}.{component}"))
            }) {
                continue;
            }
            let mut enabled = color_layers.iter().any(|name| name == layer);
            if ui.checkbox(layer, &mut enabled) {
                color_layers.retain(|name| name != layer);
                if enabled {
                    color_layers.push(layer.to_owned());
                }
                response.changed = true;
                response.finished = true;
            }
        }
        fold_ui::sdk::toolbar::tooltip(
            ui,
            "Apply the input color transform only to checked RGB layers. Depth, vectors and other data stay unchanged.",
        );
    }
    let exr = matches!(source, Source::Exr { .. });
    match source {
        Source::Unassigned { .. } => ui.text_disabled("No source file selected"),
        Source::Document { source, .. } => {
            ui.text("Nested source");
            let time = host
                .state()
                .navigation
                .last()
                .map(|v| v.time)
                .unwrap_or_else(|| {
                    Time::new(
                        i64::from(host.state().frame) * i64::from(host.state().rate[1]),
                        host.state().rate[0],
                    )
                    .unwrap()
                });
            let local = source_time(time, *start, *source_start, *duration);
            if let Some(local) = local {
                ui.text(format!(
                    "Source time: {}/{} s",
                    local.numerator(),
                    local.denominator()
                ));
                if ui.button("Open Source") {
                    actions.navigate = Some(ViewLocation {
                        document: source.document,
                        time: local,
                        label: "Source document".into(),
                    });
                }
            } else {
                ui.text_disabled("Read is inactive at this time");
            }
        }
        Source::Asset { asset, info } | Source::Exr { asset, info, .. } => {
            if aces {
                let assignment = (|| {
                    if let Some(space) = fold_platform::color::input(&node.extensions)? {
                        return Ok(Some(space));
                    }
                    let snapshot = host.snapshot().ok_or("Missing project")?;
                    let asset = snapshot
                        .state()
                        .assets
                        .get(asset)
                        .ok_or("Missing input asset")?;
                    fold_platform::color::input(&asset.extensions)
                })();
                let mut space = match assignment {
                    Ok(space) => space.unwrap_or_else(|| {
                        if exr {
                            "ACEScg".into()
                        } else {
                            fold_platform::color::VIDEO_INPUT.into()
                        }
                    }),
                    Err(error) => {
                        ui.text_wrapped(error);
                        "Invalid input".into()
                    }
                };
                if fold_ui::sdk::color_controls::input(ui, &host.state().color_choices, &mut space)
                {
                    node.extensions.insert(
                        fold_platform::color::INPUT_KEY.into(),
                        serde_json::Value::String(space),
                    );
                    response.changed = true;
                    response.finished = true;
                }
            } else {
                ui.text_disabled("Legacy linear-sRGB");
            }
            ui.text(format!(
                "{} x {} • {}/{} fps",
                info.width, info.height, info.rate[0], info.rate[1]
            ));
            if let Some(snapshot) = host.snapshot()
                && let Some(asset) = snapshot.state().assets.get(asset)
            {
                ui.text_wrapped(&asset.location);
            }
        }
    }
    actions
}
fn frame_range(
    ui: &Ui,
    rate: [u32; 2],
    start: &mut Time,
    source_start: &mut Time,
    duration: &mut Time,
    response: &mut Response,
) {
    let frame = |time: Time| {
        time.to_ticks(rate[0], rate[1], fold_foundation::Rounding::Floor)
            .unwrap_or(0)
    };
    let time = |frame: i64| {
        Time::new(
            frame
                .checked_mul(i64::from(rate[1]))
                .ok_or("Frame time overflow")?,
            rate[0],
        )
        .map_err(|_| "Invalid frame time")
    };
    let mut first = frame(*source_start);
    let mut last = first + frame(*duration) - 1;
    let mut offset = frame(*start) - first;
    let first_changed = Drag::new("First frame")
        .speed(1.)
        .range(0, i32::MAX as i64)
        .build(ui, &mut first);
    response.item(ui, first_changed);
    let last_changed = Drag::new("Last frame")
        .speed(1.)
        .range(first, i32::MAX as i64)
        .build(ui, &mut last);
    response.item(ui, last_changed);
    let offset_changed = Drag::new("Offset").speed(1.).build(ui, &mut offset);
    response.item(ui, offset_changed);
    if (first_changed || last_changed || offset_changed)
        && last >= first
        && let (Ok(first_time), Ok(length), Some(at)) = (
            time(first),
            time(last - first + 1),
            first.checked_add(offset),
        )
        && let Ok(at) = time(at)
    {
        *source_start = first_time;
        *duration = length;
        *start = at;
    }
}

/// The same exact-time navigation is used by Inspector and Network.
pub(super) fn source_location(node: &Node, time: Time) -> Option<ViewLocation> {
    let Parameters::Read {
        source: Source::Document { source, .. },
        start,
        source_start,
        duration,
    } = &node.parameters
    else {
        return None;
    };
    let local = source_time(time, *start, *source_start, *duration)?;
    Some(ViewLocation {
        document: source.document,
        time: local,
        label: "Source document".into(),
    })
}
fn source_time(time: Time, start: Time, source_start: Time, duration: Time) -> Option<Time> {
    if time < start || time >= start.checked_add(duration).ok()? {
        return None;
    }
    time.checked_sub(start).ok()?.checked_add(source_start).ok()
}
