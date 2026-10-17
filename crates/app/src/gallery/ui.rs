//! One Gallery panel for all workspaces. Native inspectors edit placed instances.
use super::{operate, storage::Store};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_platform::{
    browser::Entry,
    desktop::DesktopClient,
    gallery::{Kind, Place, Reference, Request},
};
use fold_ui::sdk::{
    self, ExtensionUi, Panel, PanelRegistry,
    imgui::{
        self, MouseButton, Ui,
        texture::{ManagedTextureId, OwnedTextureData, TextureFormat},
    },
    project_drop,
};
use serde::Deserialize;
use serde_json::Value;
use std::{collections::BTreeMap, sync::mpsc, time::Instant};

pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    registry.register("fold.gallery", Gallery::default())
}
#[derive(Clone, Deserialize)]
struct Summary {
    reference: Reference,
    name: String,
    category: String,
    kind: Kind,
    built_in: bool,
    favorite: bool,
}
struct Textures {
    frames: Vec<ManagedTextureId>,
    fps: u32,
}
struct Save {
    document: DocumentId,
    objects: Vec<ObjectId>,
    name: String,
    category: String,
    kind: usize,
    update: Option<Reference>,
}
enum Action {
    Request(Request),
    Import,
    Export(Reference),
}
type Reply = (Option<Request>, Result<Value, String>);
struct Gallery {
    entries: Vec<Summary>,
    section: usize,
    search: String,
    category: String,
    favorites: bool,
    selected: Option<Reference>,
    save: Option<Save>,
    pending: Option<mpsc::Receiver<Reply>>,
    cancel: Cancel,
    reload: bool,
    error: String,
    textures: BTreeMap<Reference, Textures>,
    preview_errors: BTreeMap<Reference, String>,
    started: Instant,
    rect: Option<([f32; 2], [f32; 2])>,
    pointer: Option<[f32; 2]>,
}
impl Default for Gallery {
    fn default() -> Self {
        Self {
            entries: vec![],
            section: 0,
            search: String::new(),
            category: String::new(),
            favorites: false,
            selected: None,
            save: None,
            pending: None,
            cancel: Cancel::default(),
            reload: true,
            error: String::new(),
            textures: Default::default(),
            preview_errors: Default::default(),
            started: Instant::now(),
            rect: None,
            pointer: None,
        }
    }
}
impl Drop for Gallery {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
impl Gallery {
    fn start(&mut self, host: &mut dyn DesktopClient, action: Action) {
        if self.pending.is_some() {
            self.error = "Gallery is busy. Try again when the current operation finishes.".into();
            return;
        }
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        let (tx, rx) = mpsc::sync_channel(1);
        self.cancel = Cancel::default();
        let cancel = self.cancel.clone();
        let result = std::thread::Builder::new()
            .name("fold-gallery".into())
            .spawn(move || {
                let request = match action {
                    Action::Request(r) => Some(r),
                    Action::Import => rfd::FileDialog::new()
                        .set_title("Import component")
                        .add_filter("Fold component", &["foldcomponent"])
                        .pick_file()
                        .map(|p| Request::Import {
                            path: p.to_string_lossy().into(),
                        }),
                    Action::Export(component) => rfd::FileDialog::new()
                        .set_title("Export component")
                        .add_filter("Fold component", &["foldcomponent"])
                        .set_file_name("component.foldcomponent")
                        .save_file()
                        .map(|p| Request::Export {
                            component,
                            path: p.to_string_lossy().into(),
                        }),
                };
                let result = match &request {
                    Some(r) => operate(&Store::user(), &snapshot, r.clone(), &cancel),
                    None => Ok(Value::Null),
                };
                let _ = tx.send((request, result));
            });
        match result {
            Ok(_) => {
                self.pending = Some(rx);
                self.error.clear();
            }
            Err(e) => self.error = e.to_string(),
        }
    }
    fn begin_save(&mut self, host: &mut dyn DesktopClient, update: Option<Reference>) {
        let selection = &host.state().selection;
        if let Some(document) = selection.document {
            self.save = Some(Save {
                document,
                objects: selection.objects.clone(),
                name: String::new(),
                category: "Graphics".into(),
                kind: 0,
                update,
            });
        } else {
            self.error =
                "Select artwork, a source layer, a clip or an effect definition to save".into();
        }
    }
    fn place(&mut self, host: &mut dyn DesktopClient, reference: Reference, update: bool) {
        let selection = host.state().selection.clone();
        let Some(document) = selection.document else {
            self.error = "Open a destination document first".into();
            return;
        };
        let at = host
            .state()
            .navigation
            .last()
            .filter(|n| n.document == document)
            .map_or(Time::ZERO, |n| n.time);
        if update && selection.objects.len() != 1 {
            self.error = "Select one placed instance to update".into();
            return;
        }
        project_drop::gallery_place(
            host,
            Place {
                component: reference,
                document,
                objects: selection.objects.clone(),
                at,
                position: None,
                track: None,
                update: if update {
                    selection.objects.first().copied()
                } else {
                    None
                },
            },
        );
    }
    fn save_dialog(&mut self, ui: &Ui, host: &mut dyn DesktopClient) {
        if self.save.is_none() {
            return;
        }
        ui.open_popup("Save component");
        if let Some(_popup) = ui
            .begin_modal_popup_config("Save component")
            .flags(imgui::WindowFlags::ALWAYS_AUTO_RESIZE)
            .begin()
        {
            let draft = self.save.as_mut().unwrap();
            ui.input_text("Name", &mut draft.name).build();
            ui.input_text("Category", &mut draft.category).build();
            ui.combo_simple_string(
                "Type",
                &mut draft.kind,
                &["Graphic", "Motion behavior", "Image effect", "Transition"],
            );
            ui.text_wrapped("Native properties and exposed behavior controls remain editable. Save one prepared behavior for a Motion preset.");
            let _disabled =
                ui.begin_disabled_with_cond(draft.name.trim().is_empty() || self.pending.is_some());
            let save = ui.button(if draft.update.is_some() {
                "Save new version"
            } else {
                "Save to My Library"
            });
            drop(_disabled);
            ui.same_line();
            let close = ui.button("Cancel");
            if save {
                let d = self.save.take().unwrap();
                let request = Request::Save {
                    document: d.document,
                    objects: d.objects,
                    name: d.name,
                    category: d.category,
                    kind: [Kind::Graphic, Kind::Motion, Kind::Effect, Kind::Transition][d.kind],
                    update: d.update,
                };
                self.section = 1;
                self.start(host, Action::Request(request));
                ui.close_current_popup();
            } else if close {
                self.save = None;
                ui.close_current_popup();
            }
        }
    }
}
impl Panel for Gallery {
    fn id(&self) -> &'static str {
        "fold.gallery.browser"
    }
    fn prepare_frame(&mut self, context: &mut imgui::Context) {
        let Some(rx) = &self.pending else {
            return;
        };
        let (request, result) = match rx.try_recv() {
            Ok(r) => r,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(_) => {
                self.pending = None;
                self.error = "Gallery worker stopped".into();
                return;
            }
        };
        self.pending = None;
        match (request, result) {
            (Some(Request::List), Ok(v)) => {
                match serde_json::from_value::<Vec<Summary>>(v["entries"].clone()) {
                    Ok(mut entries) => {
                        entries.sort_by_key(|e| e.name.to_lowercase());
                        self.entries = entries;
                    }
                    Err(e) => self.error = e.to_string(),
                }
            }
            (Some(Request::Preview { component }), Ok(v)) => {
                let result = (|| -> Result<Textures, String> {
                    let p: super::storage::Preview =
                        serde_json::from_value(v).map_err(|e| e.to_string())?;
                    let mut frames = vec![];
                    for pixels in p.frames {
                        let texture = OwnedTextureData::from_pixels(
                            TextureFormat::RGBA32,
                            p.size[0],
                            p.size[1],
                            &pixels,
                        )
                        .map_err(|e| e.to_string())?;
                        frames.push(context.register_texture(texture));
                    }
                    Ok(Textures { frames, fps: p.fps })
                })();
                match result {
                    Ok(textures) => {
                        if self.textures.len() >= 16 {
                            let oldest = *self.textures.keys().next().unwrap();
                            if let Some(old) = self.textures.remove(&oldest) {
                                for id in old.frames {
                                    let _ = context.remove_texture(id);
                                }
                            }
                        }
                        self.textures.insert(component, textures);
                    }
                    Err(e) => {
                        self.preview_errors.insert(component, e);
                    }
                }
            }
            (Some(Request::Preview { component }), Err(e)) => {
                self.preview_errors.insert(component, e);
            }
            (_, Err(e)) => self.error = e,
            (Some(_), Ok(v)) => {
                if let Ok(r) = serde_json::from_value(v["reference"].clone()) {
                    self.selected = Some(r);
                }
                self.reload = true;
            }
            _ => {}
        }
    }
    fn external_drag(&mut self, position: Option<[f32; 2]>) {
        self.pointer = position;
        self.rect = None;
    }
    fn files_dropped(
        &mut self,
        position: [f32; 2],
        paths: &[std::path::PathBuf],
        host: &mut dyn DesktopClient,
    ) -> bool {
        if !self
            .rect
            .is_some_and(|(a, b)| project_drop::inside(position, a, b))
        {
            return false;
        }
        if let [path] = paths {
            self.start(
                host,
                Action::Request(Request::Import {
                    path: path.to_string_lossy().into(),
                }),
            );
        } else {
            self.error = "Import one component package at a time".into();
        }
        true
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let origin = ui.cursor_screen_pos();
        let size = ui.content_region_avail();
        let max = [origin[0] + size[0], origin[1] + size[1]];
        self.rect = Some((origin, max));
        if self
            .pointer
            .is_some_and(|p| project_drop::inside(p, origin, max))
        {
            ui.get_window_draw_list()
                .add_rect(
                    origin,
                    max,
                    ui.style_color(imgui::StyleColor::DragDropTarget),
                )
                .thickness(2.)
                .build();
        }
        if self.reload && self.pending.is_none() {
            self.reload = false;
            self.start(host, Action::Request(Request::List));
        }
        ui.set_next_item_width(125.);
        ui.combo_simple_string("##library", &mut self.section, &["Built-in", "My Library"]);
        ui.same_line();
        if let Some(_menu) =
            sdk::toolbar::menu_button(ui, "Add", "Save artwork or import a component")
        {
            if ui.menu_item("Save selection…") {
                self.begin_save(host, None);
            }
            if ui.menu_item("Import component…") {
                self.start(host, Action::Import);
            }
            if size[0] < 320. {
                ui.checkbox("Favorites only", &mut self.favorites);
            }
            if ui.menu_item("Refresh") {
                self.reload = true;
                self.preview_errors.clear();
            }
        }
        if size[0] >= 320. {
            ui.same_line();
            ui.checkbox("Favorites", &mut self.favorites);
        }
        ui.set_next_item_width((size[0] - 110.).max(70.));
        ui.input_text("##search", &mut self.search)
            .hint("Search components")
            .build();
        ui.same_line();
        let categories: Vec<_> = std::iter::once("All categories".to_string())
            .chain(
                self.entries
                    .iter()
                    .map(|e| e.category.clone())
                    .collect::<std::collections::BTreeSet<_>>(),
            )
            .collect();
        let mut category = categories
            .iter()
            .position(|c| *c == self.category)
            .unwrap_or(0);
        ui.set_next_item_width(100.);
        if ui.combo_simple_string("##category", &mut category, &categories) {
            self.category = if category == 0 {
                String::new()
            } else {
                categories[category].clone()
            };
        }
        if !self.error.is_empty() {
            ui.text_wrapped(&self.error);
        }
        let query = self.search.to_lowercase();
        let entries: Vec<_> = self
            .entries
            .iter()
            .filter(|e| {
                e.built_in == (self.section == 0)
                    && (!self.favorites || e.favorite)
                    && (self.category.is_empty() || e.category == self.category)
                    && (e.name.to_lowercase().contains(&query)
                        || e.category.to_lowercase().contains(&query))
            })
            .cloned()
            .collect();
        if entries.is_empty() {
            ui.text_disabled(if self.section == 1 {
                "Save a selection or drop a .foldcomponent file here."
            } else {
                "No matching components."
            });
        }
        let width = ((size[0] / 170.).floor().max(1.) as usize).min(6);
        let tile_width = (size[0] / width as f32 - 6.).max(80.);
        let tile_height = tile_width * 9. / 16. + ui.text_line_height() + 14.;
        let grid = ui.cursor_screen_pos();
        let mut wanted = None;
        for (index, e) in entries.iter().enumerate() {
            let p = [
                grid[0] + (index % width) as f32 * (tile_width + 6.),
                grid[1] + (index / width) as f32 * (tile_height + 6.),
            ];
            ui.set_cursor_screen_pos(p);
            let _id = ui.push_id(&format!("{}-{}", e.reference.id, e.reference.version));
            if ui
                .selectable_config("##component")
                .selected(self.selected == Some(e.reference))
                .size([tile_width, tile_height])
                .build()
            {
                self.selected = Some(e.reference);
            }
            let hovered = ui.is_item_hovered();
            project_drop::source(ui, Entry::Component(e.reference, e.kind), &e.name);
            let draw = ui.get_window_draw_list();
            let image_max = [p[0] + tile_width, p[1] + tile_width * 9. / 16.];
            if let Some(t) = self.textures.get(&e.reference) {
                let frame = if hovered || self.selected == Some(e.reference) {
                    (self.started.elapsed().as_secs_f64() * t.fps as f64) as usize % t.frames.len()
                } else {
                    t.frames.len() - 1
                };
                draw.add_image(t.frames[frame], p, image_max, [0., 0.], [1., 1.], [1.; 4]);
            } else {
                draw.add_rect(p, image_max, ui.style_color(imgui::StyleColor::FrameBg))
                    .filled(true)
                    .build();
                draw.add_text(
                    [p[0] + 8., p[1] + 8.],
                    ui.style_color(imgui::StyleColor::TextDisabled),
                    if self.preview_errors.contains_key(&e.reference) {
                        "Preview unavailable".into()
                    } else {
                        format!("{:?}", e.kind)
                    },
                );
                if (hovered || self.selected == Some(e.reference) || index == 0)
                    && !self.preview_errors.contains_key(&e.reference)
                {
                    wanted = Some(e.reference);
                }
            }
            draw.with_clip_rect(p, [p[0] + tile_width, p[1] + tile_height], || {
                draw.add_text(
                    [p[0] + 4., image_max[1] + 4.],
                    ui.style_color(imgui::StyleColor::Text),
                    &e.name,
                );
            });
            if hovered {
                ui.tooltip_text(format!(
                    "{} · v{}\nDrag to a compatible editor. Right-click for actions.",
                    e.category, e.reference.version
                ));
                if ui.is_mouse_double_clicked(MouseButton::Left) {
                    self.place(host, e.reference, false);
                }
                if ui.is_mouse_clicked(MouseButton::Right) {
                    ui.open_popup("component-actions");
                }
            }
            if let Some(_popup) = ui.begin_popup("component-actions") {
                if ui.menu_item("Place / apply to selection") {
                    self.place(host, e.reference, false);
                }
                if ui.menu_item(if e.favorite {
                    "Remove favorite"
                } else {
                    "Add favorite"
                }) {
                    self.start(
                        host,
                        Action::Request(Request::Favorite {
                            component: e.reference,
                            value: !e.favorite,
                        }),
                    );
                }
                if ui.menu_item("Export package…") {
                    self.start(host, Action::Export(e.reference));
                }
                if !e.built_in && ui.menu_item("Save selection as new version…") {
                    self.begin_save(host, Some(e.reference));
                }
                if ui.menu_item("Update selected instance") {
                    self.place(host, e.reference, true);
                }
                ui.text_disabled("Update replaces contents; placement stays unchanged.");
                if let Some(error) = self.preview_errors.get(&e.reference) {
                    ui.text_wrapped(error);
                }
            }
        }
        ui.set_cursor_screen_pos([
            grid[0],
            grid[1] + entries.len().div_ceil(width) as f32 * (tile_height + 6.),
        ]);
        ui.dummy([0., 0.]);
        if let Some(component) = wanted
            && self.pending.is_none()
        {
            self.start(host, Action::Request(Request::Preview { component }));
        }
        self.save_dialog(ui, host);
    }
}
