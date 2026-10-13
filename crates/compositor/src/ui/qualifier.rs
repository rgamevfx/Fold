//! Temporary input sampling and matte display; never serialized into artwork.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_ui::sdk::{
    ExtensionUi, ViewerRect,
    imgui::{
        self,
        texture::{ManagedTextureId, OwnedTextureData, TextureFormat},
    },
};
#[derive(Clone, Copy, PartialEq, Eq)]
enum Mode {
    Pick,
    Matte,
}
#[derive(Default)]
pub(super) struct Editor {
    mode: Option<(ObjectId, Mode)>,
    key: Option<(DocumentId, ObjectId, u64, Time, Mode)>,
    pending: Option<fold_platform::image_probe::Receiver>,
    cancel: fold_media::Cancel,
    image: Option<fold_platform::image_probe::Image>,
    texture: Option<ManagedTextureId>,
    error: String,
    retire: bool,
}
impl Drop for Editor {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl Editor {
    pub fn reset(&mut self) {
        self.mode = None;
        self.invalidate();
    }
    fn invalidate(&mut self) {
        self.cancel.cancel();
        self.image = None;
        self.key = None;
        self.error.clear();
        self.retire = true;
    }
    pub fn controls(&mut self, ui: &imgui::Ui, id: ObjectId, can_pick: bool) {
        let picking = self.mode == Some((id, Mode::Pick));
        {
            let _disabled = ui.begin_disabled_with_cond(!can_pick);
            if ui.button(if picking { "Cancel pick" } else { "Pick color" }) {
                self.reset();
                if !picking {
                    self.mode = Some((id, Mode::Pick));
                }
            }
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Sample an averaged color from this node's input in the viewer");
        }
        ui.same_line();
        let mut matte = self.mode == Some((id, Mode::Matte));
        if ui.checkbox("Show matte", &mut matte) {
            self.reset();
            if matte {
                self.mode = Some((id, Mode::Matte));
            }
        }
    }
    pub fn prepare(&mut self, context: &mut imgui::Context) {
        if self.retire {
            if let Some(texture) = self.texture.take() {
                let _ = context.remove_texture(texture);
            }
            self.retire = false;
        }
        let Some(rx) = &self.pending else {
            return;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(std::sync::mpsc::TryRecvError::Empty) => return,
            Err(_) => Err("Image sampling stopped".into()),
        };
        self.pending = None;
        if self.cancel.check().is_err() {
            return;
        }
        match result {
            Ok(image) => {
                let [w, h] = image.display.dimensions();
                match OwnedTextureData::from_pixels(
                    TextureFormat::RGBA32,
                    w,
                    h,
                    image.display.rgba(),
                ) {
                    Ok(texture) => {
                        self.texture = Some(context.register_texture(texture));
                        self.image = Some(image);
                    }
                    Err(e) => self.error = e.to_string(),
                }
            }
            Err(e) => self.error = e,
        }
    }
    pub fn draw(
        &mut self,
        context: ExtensionUi<'_>,
        rect: ViewerRect,
        state: &mut super::state::State,
    ) {
        let ExtensionUi { ui, host } = context;
        let Some((id, mode)) = self.mode else {
            return;
        };
        if rect.size.iter().any(|v| *v <= 0.) {
            return;
        }
        if state.selected.as_slice() != [id]
            || host.state().playing
            || ui.is_key_pressed(imgui::Key::Escape)
        {
            self.reset();
            return;
        }
        let Some(document) = state.document else {
            self.reset();
            return;
        };
        let time = host
            .state()
            .navigation
            .last()
            .map_or(Time::ZERO, |v| v.time);
        if state.editing {
            if mode == Mode::Pick {
                self.reset();
                return;
            }
            self.invalidate();
            ui.get_window_draw_list().add_text(
                rect.origin,
                [1.; 4],
                "Release control to update matte",
            );
            return;
        }
        let key = (document, id, state.generation, time, mode);
        if self.key.as_ref() != Some(&key) {
            // A pick is tied to one immutable frame. Seeking or editing cancels it.
            if mode == Mode::Pick && self.key.is_some() {
                self.reset();
                return;
            }
            self.invalidate();
            if self.pending.is_some() {
                return;
            }
            self.cancel = Default::default();
            self.key = Some(key);
            let Some(graph) = &state.graph else {
                return;
            };
            let scale = (1024. / f64::from(graph.info.width))
                .min(1024. / f64::from(graph.info.height))
                .min(1.);
            let dimensions = [
                (f64::from(graph.info.width) * scale).round().max(1.) as u32,
                (f64::from(graph.info.height) * scale).round().max(1.) as u32,
            ];
            let output = format!(
                "qualifier-{}/{id}",
                if mode == Mode::Pick { "input" } else { "matte" }
            );
            match host.probe_image(
                fold_platform::image_probe::Request {
                    source: fold_project::DocumentRef {
                        document,
                        output,
                        extensions: Default::default(),
                    },
                    time,
                    dimensions,
                    data_display: mode == Mode::Matte,
                },
                self.cancel.clone(),
            ) {
                Ok(rx) => self.pending = Some(rx),
                Err(e) => self.error = e,
            }
        }
        let end = [rect.origin[0] + rect.size[0], rect.origin[1] + rect.size[1]];
        let draw = ui.get_window_draw_list();
        if let Some(texture) = self
            .texture
            .filter(|_| self.image.is_some() && !self.retire)
        {
            draw.add_image(texture, rect.origin, end, [0., 0.], [1., 1.], [1.; 4]);
        }
        let label = if !self.error.is_empty() {
            self.error.as_str()
        } else if self.pending.is_some() {
            "Loading node input…"
        } else if mode == Mode::Pick {
            "Pick input color · Escape cancels"
        } else {
            "Selection matte · Escape closes"
        };
        draw.add_text([rect.origin[0] + 8., rect.origin[1] + 8.], [1.; 4], label);
        if mode != Mode::Pick
            || !rect.editable
            || self.pending.is_some()
            || !ui.is_mouse_hovering_rect(rect.origin, end)
            || !ui.is_mouse_clicked(imgui::MouseButton::Left)
        {
            return;
        }
        let Some(image) = &self.image else {
            return;
        };
        let mouse = ui.io().mouse_pos();
        let uv = std::array::from_fn(|i| (mouse[i] - rect.origin[i]) / rect.size[i]);
        let result = (|| -> Result<(), String> {
            let color = image.sample(uv)?;
            let graph = state.graph.as_mut().ok_or("Missing composite")?;
            let mut node = graph.node(id)?.clone();
            if node
                .operation
                .parameters
                .get("settings")
                .is_some_and(|p| p.protected)
                || (0..3).any(|i| node.animation.driven(&format!("selected.{i}")))
            {
                return Err("Selected color is protected or driven".into());
            }
            let mut base = node
                .prepare()?
                .animated_properties()
                .into_iter()
                .find(|p| p.id == "selected")
                .ok_or("Missing selected color")?
                .values;
            for (i, value) in color.into_iter().enumerate() {
                let path = format!("selected.{i}");
                let curve = node.animation.curve(&path)?;
                if curve.is_some() || host.state().auto_key {
                    let mut curve = curve.unwrap_or_default();
                    curve.insert(time, value);
                    node.animation.insert(path, curve)?;
                } else {
                    base[i] = value;
                }
            }
            node.set_property("selected", &base)?;
            *graph.node_mut(id)? = node;
            Ok(())
        })();
        match result {
            Ok(()) => {
                state.commit(host);
                self.reset();
            }
            Err(e) => self.error = e,
        }
    }
}
