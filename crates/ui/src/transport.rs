//! The single Viewer transport. No feature models or playback clocks live here.
#[cfg(test)]
#[path = "transport_tests.rs"]
mod tests;
use dear_imgui_rs::{Key, MouseButton, StyleColor, Ui};
use fold_platform::desktop::{DesktopClient, DesktopCommand, TransportAction as Action};

#[derive(Default)]
pub(crate) struct Transport {
    frame: String,
    editing: bool,
    document: Option<fold_foundation::DocumentId>,
}
impl Transport {
    pub fn height(ui: &Ui) -> f32 {
        // One scrub row and one button row, including their trailing spacing.
        2. * ui.frame_height_with_spacing()
    }
    pub fn draw(&mut self, ui: &Ui, host: &mut dyn DesktopClient) {
        let state = host.state().clone();
        if self.document != state.viewer_document {
            self.document = state.viewer_document;
            self.editing = false;
        }
        let scope = format!("transport-{:?}", self.document);
        let _scope = ui.push_id(&scope);
        let _disabled =
            (state.viewer_document.is_none() || state.frames == 0).then(|| ui.begin_disabled());
        scrub_strip(ui, host);
        // A scrub can change both play state and frame within this draw.
        let state = host.state().clone();
        for (index, (action, tip)) in [
            (Action::MarkIn, "Mark In"),
            (Action::MarkOut, "Mark Out"),
            (Action::GoToIn, "Go to In"),
            (Action::PreviousFrame, "Previous frame"),
            (
                Action::TogglePlay,
                if state.playing { "Pause" } else { "Play" },
            ),
            (Action::Stop, "Stop"),
            (Action::NextFrame, "Next frame"),
            (Action::GoToOut, "Go to Out"),
        ]
        .into_iter()
        .enumerate()
        {
            if index > 0 {
                ui.same_line();
            }
            if button(ui, index, action, state.playing, tip) {
                host.command(DesktopCommand::Transport(action));
            }
        }
        ui.same_line();
        if !self.editing {
            self.frame = state.frame.to_string();
        }
        ui.set_next_item_width(80.);
        let submit = ui
            .input_text("##frame", &mut self.frame)
            .chars_decimal(true)
            .auto_select_all(true)
            .enter_returns_true(true)
            .build();
        self.editing = ui.is_item_active();
        if submit {
            if let Ok(frame) = self.frame.parse::<u32>() {
                host.command(DesktopCommand::Transport(Action::Jump(frame)));
            }
            self.editing = false;
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Frame — Enter to jump, Escape to cancel");
        }
    }
}
/// The document's full duration, independent of the marked playback range.
/// Active ImGui capture keeps a drag working outside the strip; the document
/// ID scope in draw() prevents transferring that drag into a different source.
fn scrub_strip(ui: &Ui, host: &mut dyn DesktopClient) {
    let origin = ui.cursor_screen_pos();
    let size = [ui.content_region_avail()[0].max(1.), ui.frame_height()];
    let inset = 5_f32.min(size[0] * 0.25);
    let left = origin[0] + inset;
    let width = size[0] - 2. * inset;
    ui.invisible_button("##scrub", size);
    let enabled = host.state().viewer_document.is_some() && host.state().frames > 0;
    let scrubbing = (ui.is_item_active() && ui.is_mouse_down(MouseButton::Left))
        || (ui.is_item_deactivated() && ui.is_mouse_released(MouseButton::Left));
    if enabled && scrubbing {
        let frame = scrub_frame(ui.io().mouse_pos()[0], left, width, host.state().frames);
        // Activation must pause even when clicking the current frame. Otherwise
        // avoid issuing the same seek repeatedly while the pointer is still.
        if ui.is_item_activated() || frame != host.state().frame || host.state().playing {
            host.command(DesktopCommand::Transport(Action::Jump(frame)));
        }
    }
    let state = host.state();
    let draw = ui.get_window_draw_list();
    let y = origin[1] + size[1] * 0.5;
    draw.add_rect(
        [left, y - 3.],
        [left + width, y + 3.],
        ui.style_color(StyleColor::FrameBg),
    )
    .filled(true)
    .build();
    if !enabled {
        return;
    }
    let last = state.frames.saturating_sub(1);
    let x = |frame: u32| {
        left + (f64::from(frame.min(last)) / f64::from(last.max(1)) * f64::from(width)) as f32
    };
    let range = state.playback_range;
    let (start, end) = range.bounds(state.frames);
    if range.start.is_some() || range.end.is_some() {
        draw.add_rect(
            [x(start), y - 3.],
            [x(end), y + 3.],
            ui.style_color(StyleColor::Header),
        )
        .filled(true)
        .build();
        for frame in [range.start.map(|_| start), range.end.map(|_| end)]
            .into_iter()
            .flatten()
        {
            draw.add_line(
                [x(frame), y - 5.],
                [x(frame), y + 5.],
                ui.style_color(StyleColor::TextDisabled),
            )
            .thickness(2.)
            .build();
        }
    }
    let p = x(state.frame);
    let color = ui.style_color(StyleColor::SliderGrabActive);
    draw.add_line([p, origin[1] + 2.], [p, origin[1] + size[1] - 2.], color)
        .thickness(2.)
        .build();
    draw.add_triangle(
        [p - 4., origin[1] + 1.],
        [p + 4., origin[1] + 1.],
        [p, origin[1] + 6.],
        color,
    )
    .filled(true)
    .build();
}
fn scrub_frame(x: f32, left: f32, width: f32, frames: u32) -> u32 {
    if width <= 0. || frames <= 1 {
        return 0;
    }
    let fraction = ((f64::from(x) - f64::from(left)) / f64::from(width)).clamp(0., 1.);
    (fraction * f64::from(frames - 1)).round() as u32
}
fn button(ui: &Ui, id: usize, action: Action, playing: bool, tip: &str) -> bool {
    let _id = ui.push_id(id as i32);
    let p = ui.cursor_screen_pos();
    let size = ui.frame_height();
    let clicked = ui.button_with_size("##button", [size + 4., size]);
    if ui.is_item_hovered() {
        ui.tooltip_text(tip);
    }
    let draw = ui.get_window_draw_list();
    let color = ui.style_color(dear_imgui_rs::StyleColor::Text);
    let center = [p[0] + (size + 4.) / 2., p[1] + size / 2.];
    let line = |a: [f32; 2], b: [f32; 2]| {
        draw.add_line(
            [center[0] + a[0], center[1] + a[1]],
            [center[0] + b[0], center[1] + b[1]],
            color,
        )
        .thickness(1.5)
        .build();
    };
    match action {
        Action::MarkIn | Action::MarkOut => {
            let sign = if matches!(action, Action::MarkIn) {
                1.
            } else {
                -1.
            };
            line([-3. * sign, -5.], [-3. * sign, 5.]);
            line([-3. * sign, -5.], [4. * sign, -5.]);
            line([-3. * sign, 5.], [4. * sign, 5.]);
        }
        Action::Stop => {
            draw.add_rect(
                [center[0] - 4., center[1] - 4.],
                [center[0] + 4., center[1] + 4.],
                color,
            )
            .filled(true)
            .build();
        }
        Action::TogglePlay if playing => {
            line([-3., -5.], [-3., 5.]);
            line([3., -5.], [3., 5.]);
        }
        _ => {
            let backward = matches!(action, Action::GoToIn | Action::PreviousFrame);
            let sign = if backward { -1. } else { 1. };
            draw.add_triangle(
                [center[0] + 4. * sign, center[1]],
                [center[0] - 3. * sign, center[1] - 5.],
                [center[0] - 3. * sign, center[1] + 5.],
                color,
            )
            .filled(true)
            .build();
            if !matches!(action, Action::TogglePlay) {
                line([6. * sign, -5.], [6. * sign, 5.]);
            }
            if matches!(action, Action::GoToIn | Action::GoToOut) {
                line([-6. * sign, -5.], [-6. * sign, 5.]);
            }
        }
    }
    clicked
}

/// Shared keyboard mapping; feature editors keep only their editing shortcuts.
/// The shell calls this once for a focused Viewer/editor, never while typing.
pub(crate) fn shortcuts(ui: &Ui, host: &mut dyn DesktopClient) {
    if ui.io().want_text_input()
        || ui.is_any_item_active()
        || ui.io().key_ctrl()
        || ui.io().key_alt()
    {
        return;
    }
    for (key, action) in [
        (Key::Space, Action::TogglePlay),
        (Key::LeftArrow, Action::PreviousFrame),
        (Key::RightArrow, Action::NextFrame),
        (Key::Home, Action::GoToIn),
        (Key::End, Action::GoToOut),
        (Key::I, Action::MarkIn),
        (Key::O, Action::MarkOut),
    ] {
        if ui.is_key_pressed(key) {
            host.command(DesktopCommand::Transport(action));
        }
    }
}
