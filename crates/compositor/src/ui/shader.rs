//! A local interface draft; Apply uses the same atomic migration as public tools.
use crate::shader::Shader;
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::effects::Control;
use fold_ui::sdk::imgui::Ui;
use std::collections::BTreeMap;

#[derive(Default)]
pub(super) struct Draft {
    selection: Option<(DocumentId, ObjectId)>,
    editing: bool,
    images: Vec<(Option<String>, String)>,
    controls: Vec<(Option<String>, Control)>,
    original: Vec<String>,
    error: String,
}
pub(super) struct Migration {
    pub settings: Shader,
    pub renames: BTreeMap<String, String>,
    pub remove: Vec<String>,
}
impl Draft {
    pub fn draw(
        &mut self,
        ui: &Ui,
        selection: Option<(DocumentId, ObjectId)>,
        shader: &Shader,
    ) -> Option<Migration> {
        if self.selection != selection {
            *self = Self {
                selection,
                ..Default::default()
            };
        }
        let _tree = ui.tree_node("Inputs and controls")?;
        if !self.editing {
            for name in &shader.images {
                ui.text(format!("{name} · image"));
            }
            for c in &shader.controls {
                ui.text(format!("{} · {}", c.label, type_name(c.default.len())));
            }
            if ui.small_button("Edit interface") {
                self.editing = true;
                self.images = shader
                    .images
                    .iter()
                    .map(|s| (Some(s.clone()), s.clone()))
                    .collect();
                self.controls = shader
                    .controls
                    .iter()
                    .map(|c| (Some(c.id.clone()), c.clone()))
                    .collect();
                self.original = shader
                    .images
                    .iter()
                    .chain(shader.controls.iter().map(|c| &c.id))
                    .cloned()
                    .collect();
                self.error.clear();
            }
            return None;
        }
        let mut remove_image = None;
        for (i, (_, name)) in self.images.iter_mut().enumerate() {
            let _id = ui.push_id(&format!("image-{i}"));
            ui.input_text("Image name", name).build();
            if ui.small_button("Remove image") {
                remove_image = Some(i);
            }
        }
        if let Some(i) = remove_image {
            self.images.remove(i);
        }
        if self.images.len() < 8 && ui.small_button("Add image") {
            self.images
                .push((None, format!("image_{}", self.images.len() + 1)));
        }
        let mut remove_control = None;
        for (i, (old, c)) in self.controls.iter_mut().enumerate() {
            let _id = ui.push_id(&format!("control-{i}"));
            ui.separator();
            ui.input_text("Name", &mut c.id).build();
            ui.input_text("Label", &mut c.label).build();
            // Existing types retain compatible animation and values. Replacing a
            // type is explicitly expressed by removing and adding a control.
            if old.is_none() {
                if let Some(_combo) = ui.begin_combo("Type", type_name(c.default.len())) {
                    for n in 1..=4 {
                        if ui.selectable(type_name(n)) {
                            c.default.resize(n, 0.);
                        }
                    }
                }
            } else {
                ui.text(type_name(c.default.len()));
            }
            if ui.small_button("Remove control") {
                remove_control = Some(i);
            }
        }
        if let Some(i) = remove_control {
            self.controls.remove(i);
        }
        if self.controls.len() < 32 && ui.small_button("Add control") {
            let id = format!("control_{}", self.controls.len() + 1);
            self.controls.push((
                None,
                Control {
                    label: id.clone(),
                    id,
                    default: vec![0.],
                    range: None,
                    extensions: Default::default(),
                },
            ));
        }
        ui.separator();
        if ui.small_button("Apply interface") {
            let pairs: Vec<_> = self
                .images
                .iter()
                .map(|(old, n)| (old, n))
                .chain(self.controls.iter().map(|(old, c)| (old, &c.id)))
                .collect();
            let migration = Migration {
                settings: Shader {
                    source: shader.source.clone(),
                    images: self.images.iter().map(|(_, s)| s.clone()).collect(),
                    controls: self.controls.iter().map(|(_, c)| c.clone()).collect(),
                },
                renames: pairs
                    .iter()
                    .filter_map(|(old, n)| {
                        old.as_ref()
                            .filter(|s| *s != *n)
                            .map(|s| (s.clone(), (*n).clone()))
                    })
                    .collect(),
                remove: self
                    .original
                    .iter()
                    .filter(|id| !pairs.iter().any(|(old, _)| old.as_ref() == Some(id)))
                    .cloned()
                    .collect(),
            };
            return Some(migration);
        }
        ui.same_line();
        if ui.small_button("Cancel") {
            self.editing = false;
            self.error.clear();
        }
        if !self.error.is_empty() {
            ui.text_wrapped(&self.error);
        }
        None
    }
    pub fn result(&mut self, result: Result<(), String>) -> bool {
        match result {
            Ok(()) => {
                self.editing = false;
                self.error.clear();
                true
            }
            Err(e) => {
                self.error = e;
                false
            }
        }
    }
}
fn type_name(n: usize) -> &'static str {
    match n {
        1 => "Scalar",
        2 => "Vector 2",
        3 => "Vector 3",
        _ => "Vector 4",
    }
}
