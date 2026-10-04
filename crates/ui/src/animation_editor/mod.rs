//! Compact shared Dope Sheet / Curve Editor. Features supply channels and apply
//! changed curves through their own transaction lifecycle.
mod canvas;
mod menu;
mod model;
use crate::sdk::{
    EditResponse,
    imgui::{Key, Ui},
    time_view::{View, frame},
    toolbar::{ToolbarIcon, icon_button, icon_menu},
};
use fold_foundation::{DocumentId, ObjectId, Time};
pub use model::Channel;
use model::Clipboard;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct Editor {
    document: Option<DocumentId>,
    generation: u64,
    pub(crate) curves: bool,
    pub(crate) view: View,
    pub(crate) selected: BTreeSet<ObjectId>,
    pub(crate) visible_channels: BTreeSet<(ObjectId, String)>,
    pub(crate) collapsed_nodes: BTreeSet<ObjectId>,
    pub(crate) collapsed_properties: BTreeSet<(ObjectId, String)>,
    pub(crate) selected_only: bool,
    pub(crate) search: String,
    pub(crate) snapping: bool,
    pub(crate) fitted: bool,
    pub(crate) value_range: [f64; 2],
    gesture: Option<canvas::Gesture>,
    clipboard: Clipboard,
    numeric_edit: bool,
    pub(crate) error: String,
}
pub struct Context<'a> {
    pub document: DocumentId,
    pub generation: u64,
    pub time: Time,
    pub rate: [u32; 2],
    pub frames: u32,
    pub nodes: &'a [ObjectId],
    pub auto_key: &'a mut bool,
}
#[derive(Default)]
pub struct Response {
    pub edit: EditResponse,
    pub seek: Option<Time>,
    pub select_node: Option<ObjectId>,
}
impl Editor {
    pub fn reset_gesture(&mut self) {
        self.gesture = None;
        self.numeric_edit = false;
    }
    pub fn draw(&mut self, ui: &Ui, channels: &mut [Channel], context: Context<'_>) -> Response {
        if self.document != Some(context.document) {
            *self = Self {
                document: Some(context.document),
                snapping: true,
                ..Default::default()
            };
        }
        if self.generation != context.generation {
            self.reset_gesture();
            self.generation = context.generation;
        }
        let mut response = Response::default();
        self.selected.retain(|id| {
            channels
                .iter()
                .any(|c| c.curve.keys.iter().any(|k| k.id == *id))
        });
        if icon_button(
            ui,
            "mode",
            if self.curves {
                ToolbarIcon::Curve
            } else {
                ToolbarIcon::DopeSheet
            },
            if self.curves {
                "Curve Editor — switch to Dope Sheet"
            } else {
                "Dope Sheet — switch to Curve Editor"
            },
        ) {
            self.curves = !self.curves;
        }
        let wide = ui.content_region_avail()[0] > 170.;
        if wide {
            ui.same_line();
            if icon_button(
                ui,
                "insert",
                ToolbarIcon::Add,
                "Key displayed channels at playhead (I)",
            ) {
                self.insert(channels, &context, &mut response);
            }
            ui.same_line();
            if icon_button(ui, "fit", ToolbarIcon::FrameAll, "Frame all (F)") {
                self.fitted = false;
            }
        }
        ui.same_line();
        if let Some(_menu) = icon_menu(ui, "animation-options", "Animation tools and filters") {
            if ui.menu_item("Key displayed channels (I)") {
                self.insert(channels, &context, &mut response);
            }
            if ui.menu_item("Frame all (F)") {
                self.fitted = false;
            }
            ui.checkbox("Auto Key", context.auto_key);
            ui.checkbox("Snap to frames", &mut self.snapping);
            ui.checkbox("Selected nodes only", &mut self.selected_only);
            ui.input_text("Search", &mut self.search).build();
            ui.separator();
            if ui.menu_item("Copy keys (Ctrl+C)") {
                self.clipboard = Clipboard::copy(channels, &self.selected);
            }
            if ui.menu_item("Paste keys at playhead (Ctrl+V)") {
                self.paste(channels, context.time, &mut response);
            }
            if ui.menu_item("Select all keys (Ctrl+A)") {
                self.select_all(channels, &context);
            }
            if ui.menu_item("Delete keys (Delete)") {
                self.delete(channels, &mut response);
            }
            ui.separator();
            self.key_menu(ui, channels, &mut response);
        }
        if wide {
            ui.same_line();
            let label = if *context.auto_key {
                "Auto Key"
            } else if self.curves {
                "Curves"
            } else {
                "Dope Sheet"
            };
            if ui.content_region_avail()[0] >= ui.calc_text_size(label)[0] {
                ui.text_disabled(label);
            } else {
                ui.new_line();
            }
        }
        let focused = ui.is_window_focused() && !ui.io().want_text_input();
        if focused && ui.io().key_shift() && ui.is_key_pressed(Key::F10) {
            ui.open_popup("animation-key-menu");
        }
        if focused && self.gesture.is_none() {
            if ui.is_key_pressed(Key::I) {
                self.insert(channels, &context, &mut response);
            }
            if ui.is_key_pressed(Key::F) {
                self.fitted = false;
            }
            if ui.io().key_ctrl() {
                if ui.is_key_pressed(Key::C) {
                    self.clipboard = Clipboard::copy(channels, &self.selected);
                }
                if ui.is_key_pressed(Key::V) {
                    self.paste(channels, context.time, &mut response);
                }
                if ui.is_key_pressed(Key::A) {
                    self.select_all(channels, &context);
                }
            }
            if ui.is_key_pressed(Key::Delete) {
                self.delete(channels, &mut response);
            }
        }
        if !self.fitted {
            self.view.first = channels
                .iter()
                .flat_map(|c| c.curve.keys.iter().map(|k| frame(k.time, context.rate)))
                .fold(0., f64::min);
            let last = channels
                .iter()
                .flat_map(|c| c.curve.keys.iter().map(|k| frame(k.time, context.rate)))
                .fold(f64::from(context.frames), f64::max);
            let width = ui.content_region_avail()[0].max(1.);
            let header = canvas::header_width(width);
            self.view.pixels_per_frame = (f64::from((width - header - 8.).max(1.))
                / (last - self.view.first).max(1.))
            .clamp(0.05, 80.);
            let mut low = f64::INFINITY;
            let mut high = f64::NEG_INFINITY;
            for c in channels.iter().filter(|c| self.matches(c, &context)) {
                if self.curves
                    && !self.visible_channels.is_empty()
                    && !self.visible_channels.contains(&(c.object, c.path.clone()))
                {
                    continue;
                }
                for k in &c.curve.keys {
                    low = low.min(k.value);
                    high = high.max(k.value);
                }
            }
            if !low.is_finite() {
                low = 0.;
                high = 1.;
            }
            let padding = ((high - low) * 0.15).max(0.5);
            self.value_range = [low - padding, high + padding];
            self.fitted = true;
        }
        self.visible_channels.retain(|(object, path)| {
            channels
                .iter()
                .any(|c| c.object == *object && c.path == *path)
        });
        if self.curves && self.visible_channels.is_empty() {
            let channel = channels
                .iter()
                .find(|c| {
                    self.matches(c, &context)
                        && c.curve.keys.iter().any(|k| self.selected.contains(&k.id))
                })
                .or_else(|| {
                    channels
                        .iter()
                        .find(|c| self.matches(c, &context) && context.nodes.contains(&c.object))
                })
                .or_else(|| channels.iter().find(|c| self.matches(c, &context)));
            if let Some(c) = channel {
                self.visible_channels.insert((c.object, c.path.clone()));
            }
        }
        self.canvas(ui, channels, &context, &mut response);
        if channels.is_empty() {
            self.fitted = false;
        }
        if let Some(_popup) = ui.begin_popup("animation-key-menu") {
            self.key_menu(ui, channels, &mut response);
        }
        if self.numeric_edit && ui.is_key_pressed(Key::Escape) {
            response.edit.cancelled = true;
            self.numeric_edit = false;
        } else if self.numeric_edit && !ui.is_any_item_active() {
            response.edit.finished = true;
            self.numeric_edit = false;
        }
        if !self.error.is_empty() {
            ui.text_wrapped(&self.error);
        }
        response
    }
    fn matches(&self, channel: &Channel, context: &Context<'_>) -> bool {
        (!self.selected_only || context.nodes.contains(&channel.object))
            && (self.search.is_empty()
                || format!(
                    "{} {} {}",
                    channel.node_label, channel.property_label, channel.component
                )
                .to_lowercase()
                .contains(&self.search.to_lowercase()))
    }
    fn select_all(&mut self, channels: &[Channel], context: &Context<'_>) {
        self.selected = channels
            .iter()
            .filter(|c| self.matches(c, context))
            .flat_map(|c| c.curve.keys.iter().map(|k| k.id))
            .collect();
    }
    fn paste(&mut self, channels: &mut [Channel], time: Time, response: &mut Response) {
        match self.clipboard.paste(channels, time) {
            Ok(ids) => {
                self.selected = ids;
                response.edit.changed = true;
                response.edit.finished = true;
                self.error.clear();
            }
            Err(e) => self.error = e,
        }
    }
    fn insert(&mut self, channels: &mut [Channel], context: &Context<'_>, response: &mut Response) {
        for c in channels.iter_mut() {
            if self.matches(c, context)
                && (self.visible_channels.is_empty()
                    || self.visible_channels.contains(&(c.object, c.path.clone())))
                && let Some(value) = c.curve.sample(context.time)
            {
                self.selected.insert(c.curve.insert(context.time, value));
                response.edit.changed = true;
            }
        }
        response.edit.finished |= response.edit.changed;
    }
    fn delete(&mut self, channels: &mut [Channel], response: &mut Response) {
        for channel in channels {
            let before = channel.curve.keys.len();
            channel
                .curve
                .keys
                .retain(|k| !self.selected.contains(&k.id));
            response.edit.changed |= before != channel.curve.keys.len();
        }
        response.edit.finished |= response.edit.changed;
        self.selected.clear();
    }
}

/// Shell-owned surface, dispatched to the linked editor's animation capability.
pub(crate) const PANEL_ID: &str = "fold.ui.animation";
pub(crate) struct Surface;
impl crate::sdk::Panel for Surface {
    fn id(&self) -> &'static str {
        PANEL_ID
    }
    fn supports_document_type(&self, _: &str) -> bool {
        true
    }
    fn draw(&mut self, context: crate::sdk::ExtensionUi<'_>) {
        context
            .ui
            .text_disabled("Choose a source editor for this group.");
    }
}

#[cfg(test)]
mod tests;
