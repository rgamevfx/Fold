//! Compact Read controls over the same exact-time mapping used by rendering.
use super::controls::{check, combo, row};
use crate::{
    Node, OperationConfig, Source,
    read::{Alpha, FrameMode},
};
use fold_foundation::Time;
use fold_platform::desktop::{DesktopClient, ViewLocation};
use fold_ui::sdk::{
    EditResponse as Response,
    imgui::{Drag, Ui},
};
#[derive(Default)]
pub(super) struct Actions {
    pub choose_source: bool,
    pub choose_proxy: bool,
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
    let Ok(prepared) = node.prepare() else {
        return actions;
    };
    let mut config = prepared.config().clone();
    drop(prepared);
    let OperationConfig::Read {
        source,
        start,
        source_start,
        duration,
        options,
    } = &mut config
    else {
        return actions;
    };
    let mut path = match source {
        Source::Asset { asset, .. } | Source::Exr { asset, .. } => host
            .snapshot()
            .and_then(|s| s.state().assets.get(asset).map(|a| a.location.clone()))
            .unwrap_or_default(),
        Source::Document { .. } => "Document output".into(),
        Source::Unassigned { .. } => String::new(),
    };
    let width = row(ui, "Source");
    let button = ui.frame_height();
    ui.set_next_item_width((width - button - ui.clone_style().item_spacing()[0]).max(30.));
    ui.input_text("##source", &mut path)
        .read_only(true)
        .hint("Choose a file")
        .build();
    ui.same_line();
    if matches!(source, Source::Document { .. }) {
        if ui.button_with_size("↗##source", [button, button]) {
            let time = host
                .state()
                .navigation
                .last()
                .map_or(Time::ZERO, |v| v.time);
            if let Source::Document {
                source: reference,
                info,
            } = source
            {
                match options.mapped_time(time, *start, *source_start, *duration, info.rate) {
                    Ok(Some(time)) => {
                        actions.navigate = Some(ViewLocation {
                            document: reference.document,
                            time,
                            label: "Source".into(),
                        })
                    }
                    Ok(None) => {}
                    Err(error) => ui.text_wrapped(error),
                }
            }
        }
    } else {
        actions.choose_source = ui.button_with_size("…##source", [button, button]);
    }
    let rate = source.info().rate;
    let frame = |t: Time| {
        t.to_ticks(rate[0], rate[1], fold_foundation::Rounding::Floor)
            .unwrap_or(0)
    };
    let time = |f: i64| {
        f.checked_mul(i64::from(rate[1]))
            .and_then(|n| Time::new(n, rate[0]).ok())
    };
    let mut first = frame(*source_start);
    let mut last = first + frame(*duration) - 1;
    let max = i64::from(source.info().frames) - 1;
    let width = row(ui, "Range");
    let spacing = ui.clone_style().item_spacing()[0];
    let label = ui.calc_text_size("OUT")[0];
    let field = ((width - 2. * label - 3. * spacing) * 0.5).max(25.);
    ui.align_text_to_frame_padding();
    ui.text("IN");
    ui.same_line();
    ui.set_next_item_width(field);
    let a = Drag::new("##first").range(0, last).build(ui, &mut first);
    response.item(ui, a);
    ui.same_line();
    ui.align_text_to_frame_padding();
    ui.text("OUT");
    ui.same_line();
    ui.set_next_item_width(field);
    let b = Drag::new("##last").range(first, max).build(ui, &mut last);
    response.item(ui, b);
    if (a || b) && first >= 0 && last >= first && last <= max {
        if let (Some(first), Some(length)) = (time(first), time(last - first + 1)) {
            *source_start = first;
            *duration = length;
        }
    }
    combo(
        ui,
        "Frame mode",
        &mut options.frame_mode,
        &[
            (FrameMode::StartAt, "Start at"),
            (FrameMode::Offset, "Offset"),
        ],
        response,
    );
    let mut at = frame(*start)
        - if options.frame_mode == FrameMode::Offset {
            first
        } else {
            0
        };
    row(ui, "Frame");
    let edited = Drag::new("##frame").speed(1.).build(ui, &mut at);
    response.item(ui, edited);
    if edited
        && let Some(value) = at
            .checked_add(if options.frame_mode == FrameMode::Offset {
                first
            } else {
                0
            })
            .and_then(time)
    {
        *start = value;
    }
    use fold_media::exr_sequence::Policy;
    let choices = [
        (Policy::Hold, "Hold"),
        (Policy::Black, "Black"),
        (Policy::Error, "Error"),
    ];
    let width = row(ui, "Before / after");
    let each = (width - spacing) * 0.5;
    for (index, value) in [&mut options.before, &mut options.after]
        .into_iter()
        .enumerate()
    {
        let _id = ui.push_id(index as i32);
        if index > 0 {
            ui.same_line();
        }
        ui.set_next_item_width(each);
        if let Some(_combo) = ui.begin_combo(
            "##outside",
            choices.iter().find(|(p, _)| p == value).unwrap().1,
        ) {
            for (policy, label) in choices {
                if ui
                    .selectable_config(label)
                    .selected(*value == policy)
                    .build()
                {
                    *value = policy;
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
    }
    if let Source::Exr {
        options: sequence_options,
        sequence: Some(_),
        ..
    } = source
    {
        combo(
            ui,
            "Missing",
            &mut sequence_options.missing,
            &choices,
            response,
        );
    }
    if !matches!(source, Source::Document { .. }) {
        let exr = matches!(source, Source::Exr { .. });
        let mut space = fold_platform::color::input(&node.extensions)
            .ok()
            .flatten()
            .or_else(|| {
                let asset = match source {
                    Source::Asset { asset, .. } | Source::Exr { asset, .. } => asset,
                    _ => return None,
                };
                host.snapshot().and_then(|s| {
                    s.state()
                        .assets
                        .get(asset)
                        .and_then(|a| fold_platform::color::input(&a.extensions).ok().flatten())
                })
            })
            .unwrap_or_else(|| {
                if aces {
                    if exr {
                        "ACEScg"
                    } else {
                        fold_platform::color::VIDEO_INPUT
                    }
                } else {
                    "Linear Rec.709 (sRGB)"
                }
                .into()
            });
        row(ui, "Color space");
        let _disabled = ui.begin_disabled_with_cond(options.raw || !aces);
        if let Some(_combo) = ui.begin_combo("##color-space", &space) {
            for name in &host.state().color_choices.inputs {
                if ui.selectable_config(name).selected(*name == space).build() {
                    space = name.clone();
                    node.extensions.insert(
                        fold_platform::color::INPUT_KEY.into(),
                        serde_json::Value::String(space.clone()),
                    );
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
        drop(_disabled);
        if exr || matches!(source, Source::Unassigned { .. }) {
            combo(
                ui,
                "Alpha",
                &mut options.alpha,
                &[
                    (Alpha::Premultiplied, "Premultiplied"),
                    (Alpha::Straight, "Straight"),
                    (Alpha::Ignore, "Ignore"),
                ],
                response,
            );
        }
        check(ui, "Raw data", &mut options.raw, response);
    }
    if !matches!(source, Source::Document { .. })
        && let Some(_proxy) = ui.tree_node("Proxy")
    {
        let mut path = options
            .proxy
            .as_ref()
            .and_then(|proxy| match proxy {
                Source::Asset { asset, .. } | Source::Exr { asset, .. } => host
                    .snapshot()
                    .and_then(|s| s.state().assets.get(asset).map(|a| a.location.clone())),
                _ => None,
            })
            .unwrap_or_default();
        let width = row(ui, "Source");
        ui.set_next_item_width((width - button - spacing).max(30.));
        ui.input_text("##proxy", &mut path)
            .read_only(true)
            .hint("none")
            .build();
        ui.same_line();
        actions.choose_proxy = ui.button_with_size("…##proxy", [button, button]);
        let _disabled = ui.begin_disabled_with_cond(options.proxy.is_none());
        check(ui, "Use proxy", &mut options.use_proxy, response);
        if options.proxy.is_some() && ui.small_button("Clear") {
            options.proxy = None;
            options.use_proxy = false;
            response.changed = true;
            response.finished = true;
        }
    }
    if let Some(_details) = ui.tree_node("Source settings") {
        let info = source.info();
        ui.text(format!(
            "{} × {} · {}/{} fps",
            info.width, info.height, info.rate[0], info.rate[1]
        ));
        if let Source::Exr { info, .. } = source {
            let old = info.rate;
            let mut rate = old.map(|v| v as i32);
            let width = row(ui, "Frame rate");
            ui.set_next_item_width((width - spacing) * 0.5);
            let a = ui.input_int("##rate-n", &mut rate[0]);
            response.item(ui, a);
            ui.same_line();
            ui.set_next_item_width((width - spacing) * 0.5);
            let b = ui.input_int("##rate-d", &mut rate[1]);
            response.item(ui, b);
            if (a || b) && rate.iter().all(|v| *v > 0) {
                let remap = |time: Time| {
                    time.to_ticks(old[0], old[1], fold_foundation::Rounding::Floor)
                        .ok()
                        .and_then(|f| f.checked_mul(i64::from(rate[1])))
                        .and_then(|n| Time::new(n, rate[0] as u32).ok())
                };
                if let (Some(at), Some(first), Some(length)) =
                    (remap(*start), remap(*source_start), remap(*duration))
                {
                    *start = at;
                    *source_start = first;
                    *duration = length;
                    info.rate = rate.map(|v| v as u32);
                    if let Some(Source::Exr { info: proxy, .. }) = &mut options.proxy {
                        proxy.rate = info.rate;
                    }
                }
            }
        }
        if let Source::Exr { options, .. } = source {
            check(ui, "Read as still", &mut options.still, response);
        }
        if let Source::Exr {
            image,
            color_layers,
            ..
        } = source
        {
            let layers: std::collections::BTreeSet<_> = image
                .channels
                .iter()
                .filter(|c| c.color)
                .filter_map(|c| c.name.rsplit_once('.').map(|(layer, _)| layer))
                .collect();
            for layer in layers {
                if !["red", "green", "blue"].iter().all(|c| {
                    image
                        .channels
                        .iter()
                        .any(|n| n.name == format!("{layer}.{c}"))
                }) {
                    continue;
                }
                let mut enabled = color_layers.iter().any(|n| n == layer);
                if ui.checkbox(layer, &mut enabled) {
                    color_layers.retain(|n| n != layer);
                    if enabled {
                        color_layers.push(layer.into());
                    }
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
    }
    if let Err(error) = node.set_configuration(config) {
        ui.text_wrapped(error);
    }
    actions
}
pub(super) fn source_location(node: &Node, time: Time) -> Option<ViewLocation> {
    let prepared = node.prepare().ok()?;
    let OperationConfig::Read {
        source: Source::Document { source, info },
        start,
        source_start,
        duration,
        options,
    } = prepared.config()
    else {
        return None;
    };
    let time = options
        .mapped_time(time, *start, *source_start, *duration, info.rate)
        .ok()??;
    Some(ViewLocation {
        document: source.document,
        time,
        label: "Source document".into(),
    })
}
