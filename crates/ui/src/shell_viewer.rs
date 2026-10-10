//! Viewer controls and canvas presentation consume explicit workspace/runtime state and host commands.
use super::*;
impl Shell {
    pub(super) fn playback_mode(
        &self,
        id: PanelInstanceId,
        client: &dyn DesktopClient,
    ) -> fold_platform::desktop::PlaybackMode {
        self.workspace.viewers[&id]
            .playback_mode
            .unwrap_or_else(|| {
                self.workspace
                    .resolve(id)
                    .and_then(|output| {
                        client
                            .outputs(output.document)
                            .into_iter()
                            .find(|descriptor| descriptor.reference.output == output.output)
                            .map(|descriptor| descriptor.playback_mode)
                    })
                    .unwrap_or_default()
            })
    }
    pub(super) fn playback_mode_items(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        client: &mut dyn DesktopClient,
    ) {
        use fold_platform::desktop::PlaybackMode;
        let selected = self.workspace.viewers[&id].playback_mode;
        for (label, mode) in [
            ("Use source default", None),
            ("Real-time", Some(PlaybackMode::RealTime)),
            ("Every-frame (muted)", Some(PlaybackMode::EveryFrame)),
        ] {
            if ui.menu_item_enabled_selected_no_shortcut(label, selected == mode, true) {
                self.workspace.viewers.get_mut(&id).unwrap().playback_mode = mode;
                self.submit_viewer(id, client);
            }
        }
    }
    pub(super) fn review_menu_items(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        client: &dyn DesktopClient,
    ) {
        let viewer = &self.workspace.viewers[&id];
        if ui.menu_item_enabled_selected_no_shortcut("Prepare marked range", false, !viewer.playing)
        {
            self.review_actions
                .push((id, crate::review::Action::Prepare));
        }
        let ready = self
            .viewers
            .get(&id)
            .is_some_and(|runtime| runtime.review.1);
        let realtime =
            self.playback_mode(id, client) == fold_platform::desktop::PlaybackMode::RealTime;
        if ui.menu_item_enabled_selected_no_shortcut(
            "Play cached Real-time preview",
            false,
            ready && realtime,
        ) {
            self.review_actions
                .push((id, crate::review::Action::PlayCached));
        }
        if !realtime {
            ui.text_disabled("Choose Real-time playback to replay cached frames");
        }
        if ui.menu_item("Cancel preparation") {
            self.review_actions
                .push((id, crate::review::Action::Cancel));
        }
    }
    pub(super) fn viewer_channel_selector(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        client: &dyn DesktopClient,
        compact: bool,
    ) {
        use fold_platform::desktop::{ChannelView, DisplayRange};
        let Some(output) = self.workspace.resolve(id) else {
            return;
        };
        ui.same_line();
        ui.set_next_item_width(if compact { 80. } else { 130. });
        let current = self.workspace.viewers[&id].channels.clone();
        if let Some(_combo) = ui.begin_combo("##channels", current.label()) {
            match client.channels(&output) {
                Ok(channels) => {
                    let layers: std::collections::BTreeSet<_> =
                        channels.iter().map(|name| name.layer()).collect();
                    for layer in layers {
                        if ["red", "green", "blue"].iter().all(|component| {
                            channels
                                .iter()
                                .any(|c| c.as_str() == format!("{layer}.{component}"))
                        }) && ui.selectable(format!("{layer} · RGB"))
                        {
                            self.workspace.viewers.get_mut(&id).unwrap().channels =
                                ChannelView::Rgb {
                                    layer: layer.to_owned(),
                                };
                        }
                    }
                    ui.separator();
                    for name in channels {
                        if ui.selectable(name.as_str()) {
                            self.workspace.viewers.get_mut(&id).unwrap().channels =
                                ChannelView::Channel {
                                    name,
                                    range: DisplayRange::default(),
                                };
                        }
                    }
                }
                Err(error) => ui.text_wrapped(error),
            }
            if let ChannelView::Channel { range, .. } =
                &mut self.workspace.viewers.get_mut(&id).unwrap().channels
            {
                ui.separator();
                let mut values = range.values();
                let black = crate::sdk::imgui::Drag::new("Black")
                    .speed(0.01)
                    .build(ui, &mut values[0]);
                let white = crate::sdk::imgui::Drag::new("White")
                    .speed(0.01)
                    .build(ui, &mut values[1]);
                if (black || white)
                    && let Ok(value) = DisplayRange::new(values[0], values[1])
                {
                    *range = value;
                }
            }
        }
        crate::sdk::toolbar::tooltip(
            ui,
            &format!(
                "{} — viewer layer/channel; data uses the selected grayscale range",
                current.label()
            ),
        );
    }
    pub(super) fn viewer_header(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        client: &mut dyn DesktopClient,
    ) {
        let width = ui.content_region_avail()[0];
        let compact = width < 540.;
        let labels = client
            .snapshot()
            .map(|s| workspace::document_labels(&s))
            .unwrap_or_default();
        let fit_width = ui.calc_text_size("Fit")[0]
            + ui.clone_style().frame_padding()[0] * 2.
            + ui.clone_style().item_spacing()[0];
        let reserved = fit_width
            + if compact {
                (ui.frame_height() + 4.)
                    .max(ui.calc_text_size("1/2")[0] + ui.clone_style().frame_padding()[0] * 2.)
                    + ui.clone_style().item_spacing()[0]
            } else {
                190.
            };
        let source_changed = crate::viewer_source::draw(
            ui,
            &mut self.workspace,
            id,
            &labels,
            reserved,
            |document| client.outputs(document),
        );
        if compact {
            ui.same_line();
            let divisor = self.workspace.viewers[&id].divisor;
            let popup = if divisor > 1 {
                let label = if divisor == 2 {
                    "1/2##viewer-options"
                } else {
                    "1/4##viewer-options"
                };
                if ui.button(label) {
                    ui.open_popup("viewer-options");
                }
                crate::sdk::toolbar::tooltip(
                    ui,
                    "Preview resolution — channels and playback options",
                );
                ui.begin_popup("viewer-options")
            } else {
                crate::sdk::toolbar::icon_menu(
                    ui,
                    "viewer-options",
                    "Full resolution — channels and playback options",
                )
            };
            if let Some(_popup) = popup {
                ui.text_disabled("Channels");
                self.viewer_channel_selector(ui, id, client, false);
                ui.separator();
                self.playback_mode_items(ui, id, client);
            }
        } else {
            self.viewer_channel_selector(ui, id, client, false);
            if self.workspace.viewers[&id].divisor > 1 {
                ui.same_line();
                ui.text_disabled(if self.workspace.viewers[&id].divisor == 2 {
                    "1/2"
                } else {
                    "1/4"
                });
                crate::sdk::toolbar::tooltip(ui, "Preview resolution");
            }
        }
        if source_changed {
            self.refresh_viewer_targets(client);
        }
        ui.same_line();
        if ui.button("Fit") {
            self.viewers.get_mut(&id).unwrap().navigation.fit();
        }
        crate::sdk::toolbar::tooltip(
            ui,
            "Fit image — F. Wheel to zoom; left or middle drag to pan.",
        );
    }
    #[cfg(test)]
    pub fn viewer(
        &mut self,
        ui: &Ui,
        preview: &Preview,
        statistics: &str,
        client: &mut dyn DesktopClient,
    ) {
        if let Some(&id) = self.workspace.viewers.keys().next() {
            self.viewer_instance(ui, id, preview, statistics, client);
        }
    }
    pub fn viewer_instance(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        preview: &Preview,
        statistics: &str,
        client: &mut dyn DesktopClient,
    ) {
        let Some(key) = self.viewers.get(&id).map(|runtime| runtime.key.clone()) else {
            return;
        };
        let mut close = false;
        ui.window(&key).flags(dear_imgui_rs::WindowFlags::NO_SCROLLBAR | dear_imgui_rs::WindowFlags::NO_SCROLL_WITH_MOUSE).build(|| {
            self.viewer_header(ui, id, client);
            if ui.is_window_focused_with_flags(dear_imgui_rs::FocusedFlags::ROOT_AND_CHILD_WINDOWS) {
                self.workspace.inspector_viewer = Some(id);
                if let Some(editor) = self.workspace.editor_for_viewer(id) {
                    self.workspace.focused_editor = Some(editor);
                }
            }
            if ui.is_window_hovered_with_flags(dear_imgui_rs::WindowHoveredFlags::CHILD_WINDOWS)
                && ui.is_mouse_released(dear_imgui_rs::MouseButton::Right) {
                ui.open_popup("viewer-context");
            }
            if let Some(_popup) = ui.begin_popup("viewer-context") {
                if let Some(_menu) = ui.begin_menu("Display sampling") {
                    let pixel_exact = &mut self.workspace.viewers.get_mut(&id).unwrap().pixel_exact;
                    if ui.menu_item_enabled_selected_no_shortcut("Smooth", !*pixel_exact, true) { *pixel_exact = false; }
                    if ui.menu_item_enabled_selected_no_shortcut("Pixel exact", *pixel_exact, true) { *pixel_exact = true; }
                }
                for (label, divisor) in [("Full", 1), ("Half", 2), ("Quarter", 4)] {
                    if ui.selectable(label) {
                        self.workspace.viewers.get_mut(&id).unwrap().divisor = divisor;
                    }
                    if ui.is_item_hovered() {
                        ui.tooltip_text("Resolution at the current viewer zoom in physical pixels, capped at document resolution.");
                    }
                }
                if let Some(output) = self.workspace.resolve(id) {
                    if let Some(_menu) = ui.begin_menu("Output") {
                        for descriptor in client.outputs(output.document) {
                            let _disabled = ui.begin_disabled_with_cond(descriptor.info.is_err());
                            if ui.menu_item(descriptor.label) {
                                self.workspace.pin_output(id, descriptor.reference);
                                self.refresh_viewer_targets(client);
                            }
                        }
                    }
                }
                if let Some(_menu) = ui.begin_menu("Playback mode") {
                    self.playback_mode_items(ui, id, client);
                }
                if let Some(_menu) = ui.begin_menu("Review cache") {
                    self.review_menu_items(ui, id, client);
                }
                let viewer = self.workspace.viewers.get_mut(&id).unwrap();
                if ui.checkbox("Loop playback", &mut viewer.looping) {
                    self.submit_viewer(id, client);
                }
                let mut monitored = self.workspace.monitored_viewer == Some(id);
                if ui.checkbox("Monitor audio", &mut monitored) {
                    self.workspace.monitored_viewer = monitored.then_some(id);
                    client.command(DesktopCommand::MonitorViewer(
                        self.workspace.monitored_viewer,
                    ));
                }
                if ui.menu_item("Close viewer") {
                    close = true;
                }
                if let Some(_menu) = ui.begin_menu("Diagnostics") {
                    ui.text_disabled(statistics);
                    ui.text_wrapped(client.resource_statistics());
                }
            }
            let origin = ui.cursor_pos();
            let available = ui.content_region_avail();
            let review_status = self.viewers[&id].review.0.clone();
            let image_height = (available[1] - crate::transport::Transport::height(ui)).max(0.);
            let scale = ui.io().display_framebuffer_scale();
            self.viewers.get_mut(&id).unwrap().viewport_scale = scale;
            self.viewers.get_mut(&id).unwrap().viewport_pixels = Some([
                    available[0].max(1.) * scale[0],
                    image_height.max(1.) * scale[1],
                ]);
            let output = self.workspace.resolve(id);
            let wrong_source = self.viewers.get(&id).map(|runtime| &runtime.presentation)
                .and_then(|(key, _)| key.as_ref())
                .is_some_and(|key| {
                    output.as_ref().is_none_or(|output| {
                        key.target.map(|t| t.0) != Some(output.document)
                            || key.output != output.output
                    })
                });
            let preview = if output.is_none() || wrong_source {
                &Preview::Pending
            } else {
                preview
            };
            let mut messages = Vec::new();
            if let Some((_, Some(error))) = self.viewers.get(&id).map(|runtime| &runtime.presentation) {
                messages.push(format!("Preview failed — displayed image may be stale: {error}"));
            } else if let Preview::Failed(error) = preview {
                messages.push(format!("Preview failed: {error}"));
            }
            if let Some(error) = client.viewer_audio_error(id) {
                messages.push(format!("Audio: {error}"));
            }
            if let Some(output) = &output {
                let state = client.preview_state(output, self.workspace.viewers[&id].time);
                if state.content.is_none() && !state.status.is_empty() {
                    messages.push(state.status.clone());
                }
            }
            if let Some(status) = review_status { messages.push(status); }
            if image_height > 0. && available[0] > 0. {
                ui.child_window("viewer-canvas")
                    .size([available[0], image_height])
                    .flags(dear_imgui_rs::WindowFlags::NO_SCROLLBAR | dear_imgui_rs::WindowFlags::NO_SCROLL_WITH_MOUSE)
                    .build(ui, || {
                        let canvas_origin = ui.cursor_screen_pos();
                        let canvas_size = ui.content_region_avail();
                        let insets = self.workspace.editor_for_viewer(id)
                            .filter(|editor| self.workspace.editors.get(editor).and_then(|b| self.workspace.output(b)).as_ref() == output.as_ref())
                            .and_then(|editor| self.editors.get(&editor))
                            .map(|e| e.panels.editor.viewer_tool_insets(ui)).unwrap_or([0.; 2]);
                        let canvas = [canvas_origin[0] + insets[0], canvas_origin[1] + insets[1]];
                        let area = [canvas_size[0] - insets[0], canvas_size[1] - insets[1]];
                        if area[0] <= 0. || area[1] <= 0. { return; }
                        self.viewers.get_mut(&id).unwrap().viewport_pixels = Some([area[0]*scale[0], area[1]*scale[1]]);
                        let dimensions = output.as_ref().map(|output| client.preview_state(output, self.workspace.viewers[&id].time).dimensions)
                            .unwrap_or([1, 1]);
                        let fitted = fitted_size(dimensions, area);
                        let (image_origin, size) = self.viewers.get_mut(&id).unwrap().navigation.rect(canvas, area, fitted);
                        let rect = crate::sdk::ViewerRect { editable: true, image_current: matches!(preview, Preview::Ready { .. }), canvas_origin, canvas_size, origin: image_origin, size, dimensions };
                        draw_image(ui, preview, &rect, self.viewers[&id].presentation.0.as_ref(), self.workspace.viewers[&id].pixel_exact);
                        self.viewer_overlay(ui, id, rect, client);
                        self.viewers.get_mut(&id).unwrap().navigation.input(ui, canvas, area, fitted);
                        if let Some(presented) = self.viewers.get(&id).map(|runtime| &runtime.presentation).and_then(|p| p.0.as_ref()) {
                            let mut expected = presented.clone(); expected.dimensions = dimensions; expected.region = None;
                            if self.spatial_key(id, expected, self.workspace.viewers[&id].divisor)
                                .is_some_and(|key| key.dimensions != presented.dimensions || key.region != presented.region) {
                                ui.get_window_draw_list().add_text([canvas[0] + 8., canvas[1] + area[1] - ui.text_line_height() - 8.],
                                    ui.style_color(dear_imgui_rs::StyleColor::Text), "Updating preview…");
                            }
                        }
                        if !messages.is_empty() {
                            let width = (area[0] - 16.).max(1.);
                            let message = messages.join("\n");
                            let height = ui.calc_text_size_with_opts(&message, false, width)[1];
                            let draw = ui.get_window_draw_list();
                            draw.add_rect(canvas, [canvas[0] + area[0], canvas[1] + height + 16.],
                                ui.style_color(dear_imgui_rs::StyleColor::WindowBg)).filled(true).build();
                            draw.add_text_with_font(ui.current_font(), ui.current_font_size(),
                                [canvas[0] + 8., canvas[1] + 8.], ui.style_color(dear_imgui_rs::StyleColor::Text),
                                message, width, None);
                        }
                    });
            }
            ui.set_cursor_pos([origin[0], origin[1] + image_height]);
            if let Some(output) = self.workspace.resolve(id) {
                let viewer = &self.workspace.viewers[&id];
                let mut binding = self
                    .workspace
                    .editor_for_viewer(id)
                    .and_then(|e| self.workspace.editors.get(&e).cloned())
                    .unwrap_or(EditorInstance {
                        group: viewer.group,
                        contribution: String::new(),
                        document_type: String::new(),
                        navigation: vec![],
                        selection: Default::default(),
                        mapped_navigation: false,
                    });
                binding.navigation = vec![ViewLocation {
                    document: output.document,
                    time: viewer.time,
                    label: String::new(),
                }];
                binding.selection.document = Some(output.document);
                let cache_reference = self.keys(client).into_iter().find(|(viewer,_)|*viewer==id).and_then(|(_,key)|key);
                let mut context = PanelContext::new(client, &mut binding).with_cache_reference(cache_reference).with_cache(&self.cached_previews)
                    .instance(id)
                    .with_output(&output);
                let before_time = viewer.time;
                let before_range = viewer.range;
                context.transport_context(viewer.playing, viewer.range);
                if ui.is_window_focused_with_flags(dear_imgui_rs::FocusedFlags::ROOT_AND_CHILD_WINDOWS) {
                    crate::transport::shortcuts(ui, &mut context);
                }
                self.viewers.get_mut(&id).unwrap().transport
                    .draw(ui, &mut context);
                let requested = context.playback_requested();
                let range = context.state().playback_range;
                drop(context);
                let viewer = self.workspace.viewers.get_mut(&id).unwrap();
                viewer.time = binding.navigation[0].time;
                viewer.range = range;
                if let Some(playing) = requested {
                    viewer.playing = playing;
                }
                if requested.is_some() || viewer.time != before_time || range != before_range {
                    self.submit_viewer(id, client);
                }
            } else {
                ui.dummy([0., 0.]);
            }
        });
        if close {
            self.close_viewer(id, client);
        }
    }
    pub(super) fn viewer_overlay(
        &mut self,
        ui: &Ui,
        id: PanelInstanceId,
        mut rect: crate::sdk::ViewerRect,
        client: &mut dyn DesktopClient,
    ) {
        let Some(viewer) = self.workspace.viewers.get(&id) else {
            return;
        };
        let Some(editor_id) = self.workspace.editor_for_viewer(id) else {
            return;
        };
        let Some(output) = self.workspace.resolve(id) else {
            return;
        };
        rect.editable = !viewer.playing;
        if let Some((presented, _)) = self.viewers.get(&id).map(|runtime| &runtime.presentation) {
            let state = client.preview_state(&output, viewer.time);
            let expected = state
                .preview_key(state.frame, 1)
                .and_then(|key| self.spatial_key(id, key, viewer.divisor))
                .map(|mut key| {
                    key.channels = viewer.channels.clone();
                    key
                });
            if presented.is_none() || *presented != expected {
                rect.image_current = false;
            }
        }
        let Some(binding) = self.workspace.editors.get(&editor_id) else {
            return;
        };
        if self.workspace.output(binding).as_ref() != Some(&output) {
            return;
        }
        let Some(editor) = self.editors.get_mut(&editor_id) else {
            return;
        };
        let mut binding = binding.clone();
        if let Some(location) = binding.navigation.last_mut() {
            location.time = viewer.time;
        }
        if !self.visible_editors.contains(&editor_id) {
            self.visible_editors.push(editor_id);
        }
        let selected = {
            let mut context = PanelContext::new(client, &mut binding).instance(id);
            editor.panels.editor.draw_viewer_overlay(
                ExtensionUi {
                    ui,
                    host: &mut context,
                },
                rect,
            );
            context.selection_requested()
        };
        if selected {
            self.publish_selection(editor_id, binding.selection, client);
        } else {
            self.workspace
                .editors
                .get_mut(&editor_id)
                .unwrap()
                .selection = binding.selection;
        }
    }
}

/// Pixel drawing has no workspace, project, navigation, or command authority.
fn draw_image(
    ui: &Ui,
    preview: &Preview,
    rect: &crate::sdk::ViewerRect,
    presented: Option<&PreviewKey>,
    pixel_exact: bool,
) {
    if let Preview::Ready {
        texture,
        dimensions: tile_dimensions,
        uv_max,
    } = preview
    {
        debug_assert!(tile_dimensions.iter().all(|n| *n > 0));
        // A held tile remains at its original image-space location
        // while a new demand renders; never stretch it over the frame.
        let mut start = rect.origin;
        let mut end = [rect.origin[0] + rect.size[0], rect.origin[1] + rect.size[1]];
        if let Some(key) = presented
            && let Some(region) = key.region
        {
            for i in 0..2 {
                let low = [region.x, region.y][i] as f32 / key.dimensions[i] as f32;
                let high = ([region.x, region.y][i] + region.dimensions()[i]) as f32
                    / key.dimensions[i] as f32;
                start[i] = rect.origin[i] + low * rect.size[i];
                end[i] = rect.origin[i] + high * rect.size[i];
            }
        }
        let draw = ui.get_window_draw_list();
        if pixel_exact {
            draw.set_sampler_nearest();
        }
        draw.add_image(*texture, start, end, [0., 0.], *uv_max, [1.; 4]);
        if pixel_exact {
            draw.set_sampler_linear();
        }
        // Feature overlays acquire their own draw-list wrapper.
        drop(draw);
    }
}
