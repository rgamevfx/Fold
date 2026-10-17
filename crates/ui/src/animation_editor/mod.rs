//! Compact shared Track view / Curve Editor. Features supply channels and apply
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

#[derive(Clone)]
pub struct LayerControls {
    /// Whether dropping an object on this row can make it a child.
    pub accepts_children: bool,
    pub visible: bool,
    pub icon: &'static str,
}

/// Provider-owned hierarchy, including static objects and modifier rows.
#[derive(Clone)]
pub struct Track {
    pub id: ObjectId,
    pub parent: Option<ObjectId>,
    pub label: String,
    pub locked: bool,
    pub range: Option<(Time, Time)>,
    pub layer: Option<LayerControls>,
}

use model::Clipboard;
use std::collections::BTreeSet;

#[derive(Default)]
pub struct Editor {
    pub(crate) layer_drop_rows: Vec<(ObjectId, [f32; 2], [f32; 2])>,
    document: Option<DocumentId>,
    generation: u64,
    pub(crate) curves: bool,
    pub(crate) linear_only: bool,
    hide_auto_key: bool,
    pub(crate) hide_timing: bool,
    pub(crate) header_pixels: Option<f32>,
    pub(crate) view: View,
    pub(crate) selected: BTreeSet<ObjectId>,
    viewport_width: f32,
    known_channels: BTreeSet<(ObjectId, String)>,
    pub(crate) visible_channels: BTreeSet<(ObjectId, String)>,
    pub(crate) collapsed_nodes: BTreeSet<ObjectId>,
    pub(crate) collapsed_properties: BTreeSet<(ObjectId, String)>,
    pub(crate) selected_only: bool,
    pub(crate) search: String,
    pub(crate) snapping: bool,
    pub(crate) fitted: bool,
    pub(crate) value_range: [f64; 2],
    gesture: Option<canvas::Gesture>,
    drop_hover: Option<(ObjectId, f32)>,
    clipboard: Clipboard,
    numeric_edit: bool,
    pub(crate) error: String,
    retime_frames: i32,
    time_scale: [i32; 2],
}
pub struct Context<'a> {
    pub cache: &'a std::collections::BTreeMap<u32, crate::sdk::cache_indicator::Coverage>,
    pub document: DocumentId,
    pub generation: u64,
    pub time: Time,
    pub rate: [u32; 2],
    pub frames: u32,
    pub nodes: &'a [ObjectId],
    pub tracks: &'a [Track],
    pub auto_key: &'a mut bool,
}
#[derive(Default)]
pub struct Response {
    pub edit: EditResponse,
    pub seek: Option<Time>,
    pub select_node: Option<ObjectId>,
    pub open_node: Option<ObjectId>,
    pub delete_layers: bool,
    pub range: Option<(ObjectId, Time, Time)>,
    pub reorder: Option<(ObjectId, ObjectId, bool)>,
    pub reparent: Option<(ObjectId, Option<ObjectId>)>,
    pub visibility: Option<(ObjectId, bool)>,
    pub lock: Option<(ObjectId, bool)>,
}
impl Editor {
    /// Restrict menus to the interpolation modes qualified by the provider.
    pub fn set_linear_only(&mut self, linear_only: bool) {
        self.linear_only = linear_only;
    }
    pub fn show_auto_key(&mut self, show: bool) {
        self.hide_auto_key = !show;
    }

    pub fn layer_drop_rows(&self) -> &[(ObjectId, [f32; 2], [f32; 2])] {
        &self.layer_drop_rows
    }

    pub fn reset_gesture(&mut self) {
        self.gesture = None;
        self.drop_hover = None;
        self.numeric_edit = false;
    }
    pub fn draw(&mut self, ui: &Ui, channels: &mut [Channel], context: Context<'_>) -> Response {
        self.layer_drop_rows.clear();
        if self.document != Some(context.document) {
            *self = Self {
                document: Some(context.document),
                linear_only: self.linear_only,
                hide_auto_key: self.hide_auto_key,
                snapping: true,
                time_scale: [1, 1],
                ..Default::default()
            };
        }
        if self.generation != context.generation {
            self.reset_gesture();
            self.generation = context.generation;
        }
        let locked: Vec<_> = channels
            .iter()
            .enumerate()
            .filter(|(_, c)| context.tracks.iter().any(|t| t.id == c.object && t.locked))
            .map(|(i, c)| (i, c.curve.clone()))
            .collect();
        let mut response = Response::default();
        self.selected.retain(|id| {
            channels.iter().any(|c| {
                !context.tracks.iter().any(|t| t.id == c.object && t.locked)
                    && c.curve.keys.iter().any(|k| k.id == *id)
            })
        });
        if icon_button(
            ui,
            "mode",
            if self.curves {
                ToolbarIcon::Curve
            } else {
                ToolbarIcon::Tracks
            },
            if self.curves {
                "Curve Editor — switch to Track view"
            } else {
                "Track view — switch to Curve Editor"
            },
        ) {
            self.curves = !self.curves;
            if self.curves {
                let keyed: BTreeSet<_> = channels
                    .iter()
                    .filter(|c| c.curve.keys.iter().any(|k| self.selected.contains(&k.id)))
                    .map(|c| (c.object, c.path.clone()))
                    .collect();
                if !keyed.is_empty() {
                    self.visible_channels = keyed;
                }
            }
            self.hide_timing = false;
            self.fitted = false;
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
            if !self.hide_auto_key {
                ui.checkbox("Auto Key", context.auto_key);
            }
            ui.checkbox("Snap to frames", &mut self.snapping);
            ui.checkbox("Hide timing area", &mut self.hide_timing);
            ui.checkbox("Selected objects only", &mut self.selected_only);
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
            self.key_menu(ui, channels, context.rate, &mut response);
        }
        if wide {
            ui.same_line();
            let label = if *context.auto_key {
                "Auto Key"
            } else if self.curves {
                "Curve Editor"
            } else {
                "Track view"
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
            if ui.is_key_pressed_with_repeat(Key::Delete, false) {
                if self.selected.is_empty() {
                    response.delete_layers = context
                        .tracks
                        .iter()
                        .any(|track| track.layer.is_some() && context.nodes.contains(&track.id));
                } else {
                    self.delete(channels, &mut response);
                }
            }
        }
        let width = ui.content_region_avail()[0];
        if self.viewport_width != width {
            self.fitted = false;
            self.viewport_width = width;
        }
        let known: BTreeSet<_> = channels
            .iter()
            .map(|c| (c.object, c.path.clone()))
            .collect();
        if known != self.known_channels {
            self.fitted = false;
            self.known_channels = known;
        }
        self.visible_channels.retain(|(object, path)| {
            channels
                .iter()
                .any(|c| c.object == *object && c.path == *path)
        });
        if self.curves && self.visible_channels.is_empty() {
            let selected: BTreeSet<_> = channels
                .iter()
                .filter(|c| {
                    self.matches(c, &context)
                        && (context.nodes.contains(&c.object)
                            || c.curve.keys.iter().any(|k| self.selected.contains(&k.id)))
                })
                .map(|c| (c.object, c.path.clone()))
                .collect();
            self.visible_channels = if selected.is_empty() {
                channels
                    .iter()
                    .filter(|c| self.matches(c, &context))
                    .map(|c| (c.object, c.path.clone()))
                    .collect()
            } else {
                selected
            };
        }
        if !self.fitted {
            self.view.first = channels
                .iter()
                .flat_map(|c| c.curve.keys.iter().map(|k| frame(k.time, context.rate)))
                .chain(
                    context
                        .tracks
                        .iter()
                        .filter_map(|t| t.range.map(|(start, _)| frame(start, context.rate))),
                )
                .fold(0., f64::min);
            let last = channels
                .iter()
                .flat_map(|c| c.curve.keys.iter().map(|k| frame(k.time, context.rate)))
                .chain(
                    context
                        .tracks
                        .iter()
                        .filter_map(|t| t.range.map(|(_, end)| frame(end, context.rate))),
                )
                .fold(f64::from(context.frames), f64::max);
            let width = ui.content_region_avail()[0].max(1.);
            let header = self
                .header_pixels
                .unwrap_or_else(|| canvas::header_width(width))
                .clamp(40., width.max(40.));
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
                for (index, k) in c.curve.keys.iter().enumerate() {
                    low = low.min(k.value);
                    high = high.max(k.value);
                    for incoming in [true, false] {
                        let segment = if incoming {
                            index.checked_sub(1)
                        } else {
                            (index + 1 < c.curve.keys.len()).then_some(index)
                        };
                        if segment.is_some_and(|i| {
                            c.curve.keys[i].interpolation == fold_animation::Interpolation::Bezier
                        }) {
                            let value = k.value + c.curve.handle(index, incoming).value;
                            low = low.min(value);
                            high = high.max(value);
                        }
                    }
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
        self.canvas(ui, channels, &context, &mut response);
        if channels.is_empty() && context.tracks.is_empty() {
            self.fitted = false;
        }
        if let Some(_popup) = ui.begin_popup("animation-key-menu") {
            self.key_menu(ui, channels, context.rate, &mut response);
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
        for (index, curve) in locked {
            channels[index].curve = curve;
        }
        response
    }
    fn matches(&self, channel: &Channel, context: &Context<'_>) -> bool {
        (!self.selected_only
            || context.nodes.contains(&channel.object)
            || context.tracks.iter().any(|t| {
                t.id == channel.object && t.parent.is_some_and(|p| context.nodes.contains(&p))
            }))
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
            .filter(|c| {
                self.matches(c, context)
                    && (!self.curves || self.visible_channels.contains(&(c.object, c.path.clone())))
                    && !context.tracks.iter().any(|t| t.id == c.object && t.locked)
            })
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
                && !context.tracks.iter().any(|t| t.id == c.object && t.locked)
                && (self.visible_channels.is_empty()
                    || self.visible_channels.contains(&(c.object, c.path.clone())))
                && let Some(value) = c.curve.sample(context.time)
            {
                let inherited = if self.linear_only && c.curve.at(context.time).is_none() {
                    c.curve
                        .keys
                        .iter()
                        .rev()
                        .find(|key| key.time < context.time)
                        .map(|key| key.interpolation)
                } else {
                    None
                };
                let id = c.curve.insert(context.time, value);
                if let Some(interpolation) = inherited
                    && let Some(key) = c.curve.keys.iter_mut().find(|key| key.id == id)
                {
                    key.interpolation = interpolation;
                }
                self.selected.insert(id);
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

#[cfg(test)]
mod tests;
