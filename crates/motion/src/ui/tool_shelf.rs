//! Fixed Motion shelf and contextual options. Interaction state lives in Tools.
use super::*;
impl Tools {
    pub(super) fn shelf(
        &mut self,
        ui: &Ui,
        state: &mut State,
        host: &mut dyn DesktopClient,
        rect: ViewerRect,
    ) {
        let _disabled = ui.begin_disabled_with_cond(!rect.editable);
        let inset = insets(ui);
        let bg = ui.style_color(imgui::StyleColor::WindowBg);
        {
            let d = ui.get_window_draw_list();
            d.add_rect(
                rect.canvas_origin,
                [
                    rect.canvas_origin[0] + inset[0],
                    rect.canvas_origin[1] + rect.canvas_size[1],
                ],
                bg,
            )
            .filled(true)
            .build();
            d.add_rect(
                rect.canvas_origin,
                [
                    rect.canvas_origin[0] + rect.canvas_size[0],
                    rect.canvas_origin[1] + inset[1],
                ],
                bg,
            )
            .filled(true)
            .build();
        }
        for (i, (tool, icon, key)) in [
            (Tool::Select, ToolbarIcon::Select, "V"),
            (Tool::Move, ToolbarIcon::Move, "W"),
            (Tool::Text, ToolbarIcon::Text, "T"),
            (Tool::Rectangle, ToolbarIcon::Rectangle, "U"),
            (Tool::Pen, ToolbarIcon::Pen, "P"),
        ]
        .into_iter()
        .enumerate()
        {
            let (tool, icon) = match (tool, self.tool) {
                (Tool::Move, Tool::Rotate) => (Tool::Rotate, ToolbarIcon::Rotate),
                (Tool::Move, Tool::Scale) => (Tool::Scale, ToolbarIcon::Scale),
                (Tool::Rectangle, Tool::Ellipse) => (Tool::Ellipse, ToolbarIcon::Ellipse),
                _ => (tool, icon),
            };
            let key = match tool {
                Tool::Rotate => "E",
                Tool::Scale => "R",
                Tool::Ellipse => "O",
                _ => key,
            };
            ui.set_cursor_screen_pos([
                rect.canvas_origin[0] + 4.,
                rect.canvas_origin[1] + inset[1] + 4. + i as f32 * (ui.frame_height() + 5.),
            ]);
            let active = (tool == self.tool).then(|| {
                ui.push_style_color(
                    imgui::StyleColor::Button,
                    ui.style_color(imgui::StyleColor::ButtonActive),
                )
            });
            if toolbar::icon_button(ui, tool.name(), icon, &format!("{} ({key})", tool.name())) {
                self.switch(tool, state, host);
            }
            drop(active);
            if matches!(
                tool,
                Tool::Move | Tool::Rotate | Tool::Scale | Tool::Rectangle | Tool::Ellipse
            ) {
                if ui.is_item_hovered() && ui.is_mouse_clicked(MouseButton::Right) {
                    ui.open_popup(tool.name());
                }
                if let Some(_popup) = ui.begin_popup(tool.name()) {
                    let choices: &[Tool] =
                        if matches!(tool, Tool::Move | Tool::Rotate | Tool::Scale) {
                            &[Tool::Move, Tool::Rotate, Tool::Scale]
                        } else {
                            &[Tool::Rectangle, Tool::Ellipse]
                        };
                    for choice in choices {
                        if ui.menu_item(choice.name()) {
                            self.switch(*choice, state, host);
                        }
                    }
                }
            }
        }
        ui.set_cursor_screen_pos([
            rect.canvas_origin[0] + 4.,
            rect.canvas_origin[1] + inset[1] + 4. + 5. * (ui.frame_height() + 5.),
        ]);
        if toolbar::icon_button(ui, "tools", ToolbarIcon::Add, "Create and arrange") {
            ui.open_popup("motion-tools");
        }
        if let Some(_popup) = ui.begin_popup("motion-tools") {
            super::super::tool_actions::draw(ui, state, host);
        }
        ui.set_cursor_screen_pos([
            rect.canvas_origin[0] + inset[0] + 6.,
            rect.canvas_origin[1] + 5.,
        ]);
        if !rect.editable {
            ui.text_disabled("Playback");
            return;
        }
        ui.set_next_item_width(100.);
        if let Some(_combo) = ui.begin_combo("##active-tool", self.tool.name()) {
            for tool in [
                Tool::Select,
                Tool::Move,
                Tool::Rotate,
                Tool::Scale,
                Tool::Text,
                Tool::Rectangle,
                Tool::Ellipse,
                Tool::Pen,
            ] {
                if ui.selectable(tool.name()) {
                    self.switch(tool, state, host);
                }
            }
        }
        ui.same_line();
        if let Some((_, port)) = &state.reference_pick {
            ui.text(format!("Pick {port}"));
            if ui.is_key_pressed(Key::Escape) {
                state.reference_pick = None;
            }
        } else if let Some(id) = self.text_target {
            if self.focus_text {
                ui.set_keyboard_focus_here();
                self.focus_text = false;
            }
            let mut text = state
                .motion
                .as_ref()
                .and_then(|m| m.graph.node(id).ok())
                .and_then(|n| n.inputs.get("text"))
                .and_then(|v| {
                    if let crate::graph::Input::Value(Datum::Text(t)) = v {
                        Some(t.clone())
                    } else {
                        None
                    }
                })
                .unwrap_or_default();
            ui.set_next_item_width((rect.canvas_size[0] - inset[0] - 130.).clamp(60., 300.));
            let changed = ui.input_text("##edit-text", &mut text).build();
            if changed
                && let Some(m) = state.motion.as_mut()
                && let Ok(n) = node(m, id)
            {
                n.set("text", Datum::Text(text));
                self.text_editing = true;
                state.preview(host);
            }
            if ui.is_key_pressed(Key::Escape) {
                if self.text_editing {
                    state.cancel(host);
                }
                self.text_editing = false;
                self.text_target = None;
            } else if ui.is_item_deactivated_after_edit() || ui.is_key_pressed(Key::Enter) {
                if self.text_editing {
                    state.commit(host);
                }
                self.text_editing = false;
                self.text_target = None;
            }
        } else if self.tool == Tool::Pen && self.busy() {
            if ui.small_button("Finish") {
                self.finish(state, host);
            }
            ui.same_line();
            if ui.small_button("Cancel") {
                self.creation = None;
                state.cancel(host);
            }
        } else if self.tool == Tool::Text {
            ui.set_next_item_width(65.);
            imgui::Drag::new("Size")
                .range(1., 1000.)
                .speed(1.)
                .build(ui, &mut self.text_size);
        } else if self.tool == Tool::Rectangle {
            ui.set_next_item_width(65.);
            imgui::Drag::new("Corners")
                .range(0., 1000.)
                .speed(1.)
                .build(ui, &mut self.radius);
        } else if self.tool == Tool::Pen {
            ui.set_next_item_width(65.);
            imgui::Drag::new("Stroke")
                .range(0.1, 100.)
                .speed(0.1)
                .build(ui, &mut self.stroke);
        } else if !self.tool.creates() {
            ui.checkbox("Points", &mut self.edit_points);
        }
        if self.tool.creates() && rect.canvas_size[0] > 450. {
            ui.same_line();
            if ui.small_button(if self.tool == Tool::Pen {
                "Stroke color"
            } else {
                "Fill"
            }) {
                ui.open_popup("tool-color");
            }
            if let Some(_popup) = ui.begin_popup("tool-color") {
                let aces = host
                    .snapshot()
                    .is_some_and(|s| fold_platform::color::project(&s).ok().flatten().is_some());
                let mut response = fold_ui::sdk::EditResponse::default();
                fold_ui::sdk::color_controls::authored(ui, &mut self.fill, aces, &mut response);
            }
        }
        if rect.canvas_size[0] > 400. && self.text_target.is_none() {
            ui.same_line();
            ui.checkbox("Snap", &mut self.snap);
        }
        if ui.is_window_focused() && !ui.io().want_text_input() && !ui.is_any_item_active() {
            for (key, tool) in [
                (Key::V, Tool::Select),
                (Key::W, Tool::Move),
                (Key::E, Tool::Rotate),
                (Key::R, Tool::Scale),
                (Key::T, Tool::Text),
                (Key::U, Tool::Rectangle),
                (Key::O, Tool::Ellipse),
                (Key::P, Tool::Pen),
            ] {
                if ui.is_key_pressed(key) {
                    self.switch(tool, state, host);
                }
            }
        }
        if state.scene_scope.is_some() || !state.source_history.is_empty() {
            // Breadcrumb is in the shelf's remaining space, never over the artwork.
            ui.set_cursor_screen_pos([
                rect.canvas_origin[0] + 4.,
                rect.canvas_origin[1] + inset[1] + 4. + 6. * (ui.frame_height() + 5.),
            ]);
            if toolbar::icon_button(ui, "leave-source", ToolbarIcon::Back, "Return to scene") {
                if let Some(previous) = state.source_history.pop() {
                    select(state, host, previous);
                    state.scene_scope = None;
                } else {
                    state.scene_scope = state.scene_scope.and_then(|scope| {
                        state
                            .motion
                            .as_ref()?
                            .scene
                            .as_ref()?
                            .objects
                            .iter()
                            .find(|o| o.id == scope)?
                            .parent
                    });
                }
                state.visual_revision += 1;
            }
        }
    }
}
