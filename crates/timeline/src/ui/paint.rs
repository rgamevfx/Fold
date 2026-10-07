//! Timeline presentation, separate from gesture interpretation.
use super::*;
impl Canvas {
    pub(super) fn paint(
        &self,
        ui: &Ui,
        model: &CanvasModel<'_>,
        shown: &Sequence,
        layout: Layout,
        total: f64,
        visible: f64,
    ) {
        let colors = EditorColors::from_ui(ui);
        let draw = ui.get_window_draw_list();
        let _clip = draw.push_clip_rect(layout.area.min, layout.area.max, true);
        draw.add_rect(layout.area.min, layout.area.max, colors.background)
            .filled(true)
            .build();
        draw.add_rect(
            layout.area.min,
            [layout.body.min[0], layout.body.max[1]],
            colors.header,
        )
        .filled(true)
        .build();
        let step = interaction::ruler_step(
            self.view.pixels_per_frame,
            shown.rate,
            100. * f64::from(layout.scale),
        );
        let first_tick = (self.view.first / step).ceil() * step;
        let last_frame = self.view.first + visible;
        {
            let _body_clip = draw.push_clip_rect(layout.ruler.min, layout.body.max, true);
            let mut tick = first_tick;
            while tick <= last_frame {
                let x = self.view.x(tick, layout.body.min[0]);
                draw.add_line(
                    [x, layout.ruler.max[1] - 6.0],
                    [x, layout.body.max[1]],
                    colors.grid,
                )
                .build();

                draw.add_text(
                    [x + 4.0, layout.ruler.min[1] + 6.0],
                    colors.muted,
                    interaction::timecode(tick.round() as i64, shown.rate),
                );
                let minor = (step / 4.).floor().max(1.);
                for n in 1..4 {
                    let f = tick + minor * f64::from(n);
                    if f < tick + step {
                        let x = self.view.x(f, layout.body.min[0]);
                        draw.add_line(
                            [x, layout.ruler.max[1] - 3. * layout.scale],
                            [x, layout.ruler.max[1]],
                            colors.muted,
                        )
                        .build();
                    }
                }
                tick += step;
            }
        }
        for (index, track) in rows(shown).iter().enumerate() {
            let row = layout.row_rect(index, self.view.scroll_y);
            if row.max[1] <= layout.body.min[1] || row.min[1] >= layout.body.max[1] {
                continue;
            }
            let _row_clip = draw.push_clip_rect(
                [layout.area.min[0], row.min[1].max(layout.body.min[1])],
                [row.max[0], row.max[1].min(layout.body.max[1])],
                true,
            );
            if track.kind == TrackKind::Audio
                && (index == 0 || rows(shown)[index - 1].kind == TrackKind::Video)
            {
                draw.add_line([layout.area.min[0], row.min[1]], row.min, colors.muted)
                    .thickness(2.)
                    .build();
            }
            let mut lane = colors.lane;
            lane[3] = if index % 2 == 0 { 0.34 } else { 0.20 };
            draw.add_rect(row.min, row.max, lane).filled(true).build();
            draw.add_line([layout.area.min[0], row.max[1]], row.max, colors.grid)
                .build();
            for (control, text, on) in [
                (
                    0,
                    if track.kind == TrackKind::Video {
                        ""
                    } else {
                        "M"
                    },
                    if track.kind == TrackKind::Video {
                        track.enabled
                    } else {
                        !track.enabled
                    },
                ),
                (1, "S", track.solo),
                (2, "", track.locked),
            ] {
                if control == 1 && track.kind == TrackKind::Video {
                    continue;
                }
                let rect = layout.control(row, control);
                draw.add_rect(
                    rect.min,
                    rect.max,
                    if on && (control != 0 || track.kind == TrackKind::Audio) {
                        colors.selected
                    } else {
                        colors.lane
                    },
                )
                .filled(true)
                .rounding(2.0)
                .build();
                if text.is_empty() {
                    fold_ui::sdk::toolbar::draw_icon_on(
                        ui,
                        &draw,
                        if control == 2 {
                            if track.locked {
                                fold_ui::sdk::toolbar::ToolbarIcon::Lock
                            } else {
                                fold_ui::sdk::toolbar::ToolbarIcon::Unlock
                            }
                        } else {
                            fold_ui::sdk::toolbar::ToolbarIcon::View
                        },
                        rect.min,
                        21. * layout.scale,
                        if track.enabled || control == 2 {
                            colors.text
                        } else {
                            colors.muted
                        },
                    );
                }
                if rect.contains(ui.mouse_pos()) {
                    ui.tooltip_text(match control {
                        0 => {
                            if track.kind == TrackKind::Video {
                                "Toggle video visibility"
                            } else {
                                "Mute audio"
                            }
                        }
                        1 => "Solo audio",
                        _ => "Lock track",
                    });
                }
                draw.add_text(
                    [
                        rect.min[0] + 5.0 * layout.scale,
                        rect.min[1] + 4.0 * layout.scale,
                    ],
                    colors.text,
                    text,
                );
            }
            let _lane_clip = draw.push_clip_rect(row.min, row.max, true);
            for clip in shown.clips.iter().filter(|c| c.track == track.id) {
                let rect = clip_rect(clip, row, self.view, shown.rate, layout.scale);
                if rect.max[0] <= row.min[0] || rect.min[0] >= row.max[0] {
                    continue;
                }
                let selected = model.selected.contains(&clip.id)
                    || self.drag.as_ref().is_some_and(|d| {
                        d.original
                            .linked_ids(d.clip, d.linked)
                            .is_ok_and(|ids| ids.contains(&clip.id))
                    });
                let mut color = if track.kind == TrackKind::Video {
                    colors.video
                } else {
                    colors.audio
                };
                if !track.enabled || track.locked {
                    color[3] = 0.45;
                }
                draw.add_rect(rect.min, rect.max, color)
                    .filled(true)
                    .rounding(3.0 * layout.scale)
                    .build();
                if selected {
                    draw.add_rect(rect.min, rect.max, colors.selected)
                        .thickness(2.0 * layout.scale)
                        .rounding(3.0 * layout.scale)
                        .build();
                }
                if rect.max[0].min(row.max[0]) - rect.min[0].max(row.min[0]) < 12.0 * layout.scale {
                    continue;
                }
                let _label_clip = draw.push_clip_rect(
                    [rect.min[0].max(row.min[0]) + 3.0, rect.min[1]],
                    [rect.max[0].min(row.max[0]) - 2.0, rect.max[1]],
                    true,
                );
                let x = rect.min[0].max(row.min[0]) + 8.0 * layout.scale;
                if let Some(visual) = self.visuals.get(&clip.id) {
                    fold_ui::sdk::toolbar::draw_icon_on(
                        ui,
                        &draw,
                        visual.icon,
                        [x - 3., rect.min[1] + 3. * layout.scale],
                        16. * layout.scale,
                        colors.text,
                    );
                    draw.add_text(
                        [x + 20. * layout.scale, rect.min[1] + 6. * layout.scale],
                        colors.text,
                        &visual.name,
                    );
                    if let Some(error) = &visual.error {
                        draw.add_text(
                            [x, rect.max[1] - 17. * layout.scale],
                            colors.text,
                            "! Preview unavailable",
                        );
                        if rect.contains(ui.mouse_pos()) {
                            ui.tooltip_text(error);
                        }
                    }
                    if let Some(texture) = visual.texture
                        && rect.max[1] - rect.min[1] >= 50. * layout.scale
                    {
                        let top = rect.min[1] + 24. * layout.scale;
                        let height = rect.max[1] - top - 2.;
                        let right = if track.kind == TrackKind::Audio {
                            rect.max[0] - 3.
                        } else {
                            (rect.min[0] + height * 128. / 72.).min(rect.max[0] - 3.)
                        };
                        draw.add_image(
                            texture,
                            [rect.min[0] + 3., top],
                            [right, rect.max[1] - 2.],
                            [0., 0.],
                            [1., 1.],
                            [1.; 4],
                        );
                    }
                    if rect.contains(ui.mouse_pos()) && self.drag.is_none() {
                        ui.tooltip_text(format!(
                            "{} · {}{}",
                            visual.name,
                            visual.kind,
                            if clip.link.is_some() {
                                " · Linked A/V"
                            } else {
                                ""
                            }
                        ));
                    }
                } else {
                    draw.add_text(
                        [x, rect.min[1] + 6. * layout.scale],
                        colors.text,
                        model
                            .labels
                            .get(&clip.asset)
                            .map(String::as_str)
                            .unwrap_or("Clip"),
                    );
                }
                if clip.link.is_some() && rect.max[0] - rect.min[0] > 55. * layout.scale {
                    fold_ui::sdk::toolbar::draw_icon_on(
                        ui,
                        &draw,
                        fold_ui::sdk::toolbar::ToolbarIcon::Link,
                        [
                            rect.max[0] - 22. * layout.scale,
                            rect.max[1] - 18. * layout.scale,
                        ],
                        14. * layout.scale,
                        colors.text,
                    );
                }
                if rect.contains(ui.mouse_pos()) && !track.locked {
                    for x in [
                        rect.min[0] + 3. * layout.scale,
                        rect.max[0] - 3. * layout.scale,
                    ] {
                        draw.add_line([x, rect.min[1] + 4.], [x, rect.max[1] - 4.], colors.text)
                            .thickness(2.)
                            .build();
                    }
                }
            }
        }
        if let Some(drag) = &self.drag {
            for clip in model
                .sequence
                .clips
                .iter()
                .filter(|c| drag.selected.contains(&c.id))
            {
                if let Some(index) = rows(model.sequence).iter().position(|t| t.id == clip.track) {
                    let r = clip_rect(
                        clip,
                        layout.row_rect(index, self.view.scroll_y),
                        self.view,
                        shown.rate,
                        layout.scale,
                    );
                    draw.add_rect(r.min, r.max, colors.muted).build();
                }
            }
            if self.snapping
                && !ui.io().key_shift()
                && let Some(c) = shown.clips.iter().find(|c| c.id == drag.clip)
            {
                for edge in [c.start, c.end().unwrap()] {
                    if interaction::time(i64::from(model.playhead), shown.rate).ok() == Some(edge)
                        || shown.clips.iter().any(|other| {
                            other.id != c.id
                                && !drag.selected.contains(&other.id)
                                && (c.link.is_none() || other.link != c.link)
                                && (other.start == edge || other.end().ok() == Some(edge))
                        })
                    {
                        let x = self
                            .view
                            .x(interaction::frame(edge, shown.rate), layout.body.min[0]);
                        draw.add_line(
                            [x, layout.body.min[1]],
                            [x, layout.body.max[1]],
                            colors.selected,
                        )
                        .build();
                    }
                }
            }
        }
        if let Some((start, _)) = &self.marquee {
            let p = ui.mouse_pos();
            draw.add_rect(
                [start[0].min(p[0]), start[1].min(p[1])],
                [start[0].max(p[0]), start[1].max(p[1])],
                colors.selected,
            )
            .build();
        }
        {
            let _body = draw.push_clip_rect(layout.ruler.min, layout.body.max, true);
            let x = self.view.x(f64::from(model.playhead), layout.body.min[0]);
            draw.add_line(
                [x, layout.ruler.min[1]],
                [x, layout.body.max[1]],
                colors.playhead,
            )
            .thickness(2.0 * layout.scale)
            .build();
            draw.add_rect(
                [x - 4.0 * layout.scale, layout.ruler.min[1]],
                [
                    x + 4.0 * layout.scale,
                    layout.ruler.min[1] + 8.0 * layout.scale,
                ],
                colors.playhead,
            )
            .filled(true)
            .build();
        }
        draw.add_text(
            [layout.area.min[0] + 8., layout.area.min[1] + 6.],
            colors.text,
            interaction::timecode(i64::from(model.playhead), shown.rate),
        );
        for (index, track) in rows(shown).iter().enumerate() {
            let row = layout.row_rect(index, self.view.scroll_y);
            if row.max[1] <= layout.body.min[1] || row.min[1] >= layout.body.max[1] {
                continue;
            }
            {
                let _name_clip = draw.push_clip_rect(
                    [layout.area.min[0], row.min[1].max(layout.body.min[1])],
                    [
                        layout.control(row, 0).min[0] - 4.,
                        row.max[1].min(layout.body.max[1]),
                    ],
                    true,
                );
                draw.add_text(
                    [
                        layout.area.min[0] + 9.0 * layout.scale,
                        row.min[1] + 10.0 * layout.scale,
                    ],
                    if model.selected.contains(&track.id) {
                        colors.selected
                    } else {
                        colors.text
                    },
                    &track.name,
                );
            }
        }
        drop(_clip);
        drop(draw);
        fold_ui::sdk::cache_indicator::strip(
            ui,
            [layout.ruler.min[0], layout.ruler.max[1] - 3.],
            layout.ruler.max[0] - layout.ruler.min[0],
            self.view.first,
            self.view.pixels_per_frame,
            &self.cache,
        );
        let draw = ui.get_window_draw_list();
        draw.add_line(
            [layout.body.min[0], layout.area.min[1]],
            [layout.body.min[0], layout.body.max[1]],
            colors.grid,
        )
        .build();
        draw.add_rect(layout.bar.min, layout.bar.max, colors.header)
            .filled(true)
            .build();
        let width = layout.bar.max[0] - layout.bar.min[0];
        let left = layout.bar.min[0] + (self.view.first / total) as f32 * width;
        let right = left + (visible / total) as f32 * width;
        draw.add_rect(
            [left, layout.bar.min[1] + 2.0],
            [right, layout.bar.max[1] - 2.0],
            colors.muted,
        )
        .filled(true)
        .rounding(3.0)
        .build();
    }
}
