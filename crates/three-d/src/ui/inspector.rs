use super::Shared;
use crate::controls::edited_binding;
use crate::{
    editing::{self, Edit},
    model::{Content, Geometry, Material, Projection},
    package,
};
use fold_foundation::{ObjectId, Time};
use fold_graph::{
    binding::{Binding, Combination},
    model::Parameter,
    types::{EulerOrder, Literal, Space},
};
use fold_ui::sdk::{
    EditResponse, ExtensionUi, NumericProperty, Panel, TextDraft, UiId,
    imgui::{TreeNodeFlags, Ui},
};

pub(super) struct Inspector {
    pub state: Shared,
    name: Option<(fold_foundation::DocumentId, ObjectId, TextDraft)>,
}
impl Inspector {
    pub fn new(state: Shared) -> Self {
        Self { state, name: None }
    }
}
impl Panel for Inspector {
    fn id(&self) -> &'static str {
        package::INSPECTOR
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::TYPE)
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let mut state = self.state.borrow_mut();
        state.sync(host);
        let (Some(document), [id]) = (state.document, state.selected.as_slice()) else {
            ui.text_disabled("Select a 3D object.");
            return;
        };
        let id = *id;
        let Some(scene) = state.current() else {
            return;
        };
        let Some(mut object) = scene.objects.get(&id).cloned() else {
            return;
        };
        let scope = UiId {
            package: crate::PACKAGE,
            panel: package::INSPECTOR,
            document,
            object: id,
        };
        let _scope = scope.scope(ui);
        let locked = editing::editable(scene, id).is_err();
        let active_camera = scene.active_camera;
        let disabled = locked || host.state().playing || state.busy();
        let _disabled = ui.begin_disabled_with_cond(disabled);
        let mut response = EditResponse::default();
        if self
            .name
            .as_ref()
            .is_none_or(|(doc, object_id, _)| *doc != document || *object_id != id)
        {
            self.name = Some((document, id, TextDraft::new(&object.name)));
        }
        let (_, _, name) = self.name.as_mut().unwrap();
        name.sync(&object.name);
        ui.input_text("Name", &mut name.text).build();
        if ui.is_item_active() && ui.is_key_pressed(fold_ui::sdk::imgui::Key::Escape) {
            name.reset(&object.name);
        } else if ui.is_item_deactivated_after_edit() && name.text != object.name {
            if name.conflicts(&object.name) {
                state.error = "The object name changed. Your draft is retained.".into();
            } else {
                object.name = name.text.clone();
                response.changed = true;
                response.finished = true;
            }
        }
        let changed = ui.checkbox("Visible", &mut object.visible);
        response.item(ui, changed);
        if ui.collapsing_header("Transform", TreeNodeFlags::DEFAULT_OPEN) {
            if !object.transform.parent_space.is_empty() {
                ui.text_disabled("Parent motion preserved");
                if ui.is_item_hovered() {
                    ui.tooltip_text("These local channels keep their original animation. Later parent edits are applied relative to the motion saved when parenting.");
                }
            }
            for (label, parameter) in [
                ("Position", &mut object.transform.position),
                ("Rotation", &mut object.transform.rotation),
                ("Scale", &mut object.transform.scale),
            ] {
                if let Err(error) = property(
                    ui,
                    label,
                    parameter,
                    state.time,
                    host.state().auto_key,
                    &mut response,
                ) {
                    state.error = error;
                }
            }
        }
        content_controls(ui, &mut object.content, &mut response);
        let mut camera = None;
        if matches!(object.content, Content::Camera { .. })
            && active_camera != id
            && ui.button("Use camera")
        {
            camera = Some(Edit::ActiveCamera {
                expected: state.scene.as_ref().unwrap().active_camera,
                value: id,
            });
            response.changed = true;
            response.finished = true;
        }
        let mut resources = vec![];
        if let Content::Instance {
            geometry, material, ..
        } = object.content
        {
            let scene = state.current().unwrap();
            let before = state.scene.as_ref().unwrap();
            let mut value = match &scene.geometry[&geometry] {
                Geometry::Mesh { .. } => None,
                primitive => Some(primitive.clone()),
            };
            if ui.collapsing_header("Geometry", TreeNodeFlags::DEFAULT_OPEN) {
                let _disabled = ui.begin_disabled_with_cond(
                    editing::resource_editable(scene, geometry, true).is_err(),
                );
                shared(ui, scene, geometry, true);
                if let Some(value) = &mut value {
                    geometry_controls(ui, value, &mut response);
                } else if let Geometry::Mesh {
                    positions, indices, ..
                } = &scene.geometry[&geometry]
                {
                    ui.text_disabled(format!(
                        "Mesh: {} vertices, {} triangles",
                        positions.len(),
                        indices.len() / 3
                    ));
                }
            }
            if let Some(value) = value
                && before.geometry[&geometry] != value
            {
                resources.push(Edit::Geometry {
                    id: geometry,
                    expected: Some(before.geometry[&geometry].clone()),
                    value: Some(value),
                });
            }
            let mut value = scene.materials[&material].clone();
            if ui.collapsing_header("Material", TreeNodeFlags::DEFAULT_OPEN) {
                let _disabled = ui.begin_disabled_with_cond(
                    editing::resource_editable(scene, material, false).is_err(),
                );
                shared(ui, scene, material, false);
                material_controls(ui, &mut value, &mut response);
            }
            if before.materials[&material] != value {
                resources.push(Edit::Material {
                    id: material,
                    expected: Some(before.materials[&material].clone()),
                    value: Some(value),
                });
            }
        }
        response.cancel_on_escape(ui, state.editing());
        if response.cancelled {
            state.cancel(host);
        } else {
            if response.changed {
                let expected = state.scene.as_ref().unwrap().objects[&id].clone();
                let mut edits = vec![Edit::Replace {
                    id,
                    expected: Box::new(expected),
                    value: Box::new(object),
                }];
                edits.extend(resources);
                edits.extend(camera);
                if let Err(error) = state.preview(host, edits) {
                    state.error = error;
                }
            }
            if response.finished
                && let Err(error) = state.commit(host)
            {
                state.error = error;
            }
        }
        if locked {
            ui.text_disabled("Protected object");
        }
        if !state.error.is_empty() {
            ui.text_wrapped(&state.error);
        }
    }
}

fn shared(ui: &Ui, scene: &crate::model::Scene, resource: ObjectId, geometry: bool) {
    let count = scene.objects.values().filter(|object| matches!(object.content,
        Content::Instance { geometry: mesh, material, .. } if (if geometry { mesh } else { material }) == resource)).count();
    if count > 1 {
        ui.text_disabled(format!("Shared by {count} objects"));
    }
    if editing::resource_editable(scene, resource, geometry).is_err() {
        ui.text_disabled("Used by a protected object");
    }
}
fn number(
    ui: &Ui,
    label: &str,
    unit: &str,
    value: &mut f32,
    range: [f64; 2],
    response: &mut EditResponse,
) {
    let mut number = f64::from(*value);
    NumericProperty {
        id: label,
        label,
        unit,
        speed: 0.01,
        range: Some(range),
        default: None,
    }
    .draw(ui, &mut number, response);
    *value = number as f32;
}
fn vector(ui: &Ui, label: &str, unit: &str, value: &mut [f32; 3], response: &mut EditResponse) {
    let _id = ui.push_id(label);
    if ui.collapsing_header(label, TreeNodeFlags::empty()) {
        for (axis, value) in ["X", "Y", "Z"].into_iter().zip(value) {
            number(ui, axis, unit, value, [-1e6, 1e6], response);
        }
    }
}
fn content_controls(ui: &Ui, content: &mut Content, response: &mut EditResponse) {
    match content {
        Content::Camera {
            target,
            up,
            projection,
            near,
            far,
            ev100,
            ..
        } => {
            if ui.collapsing_header("Camera", TreeNodeFlags::DEFAULT_OPEN) {
                let mut kind = usize::from(matches!(projection, Projection::Orthographic { .. }));
                if ui.combo_simple_string("Projection", &mut kind, &["Perspective", "Orthographic"])
                {
                    *projection = if kind == 0 {
                        Projection::Perspective {
                            vertical_fov: std::f32::consts::FRAC_PI_4,
                        }
                    } else {
                        Projection::Orthographic { vertical_size: 2. }
                    };
                    response.changed = true;
                    response.finished = true;
                }
                match projection {
                    Projection::Perspective { vertical_fov } => {
                        let mut degrees = vertical_fov.to_degrees();
                        let original = degrees;
                        number(
                            ui,
                            "Field of view",
                            "°",
                            &mut degrees,
                            [0.1, 179.9],
                            response,
                        );
                        if degrees != original {
                            *vertical_fov = degrees.to_radians();
                        }
                    }
                    Projection::Orthographic { vertical_size } => {
                        number(ui, "View height", "m", vertical_size, [1e-6, 1e6], response)
                    }
                }
                number(ui, "Exposure", "EV", ev100, [-20., 20.], response);
                vector(ui, "Look target (local)", "m", target, response);
                vector(ui, "Up direction (local)", "", up, response);
                if ui.collapsing_header("Clipping", TreeNodeFlags::empty()) {
                    number(ui, "Near", "m", near, [1e-6, 1e6], response);
                    number(ui, "Far", "m", far, [1e-6, 1e6], response);
                }
            }
        }
        Content::DirectionalLight {
            direction,
            color,
            illuminance,
            ..
        } => {
            if ui.collapsing_header("Light", TreeNodeFlags::DEFAULT_OPEN) {
                let mut working = color.map(f64::from);
                fold_ui::sdk::color_controls::authored_rgb(ui, &mut working, false, response);
                *color = working.map(|value| value as f32);
                number(
                    ui,
                    "Illuminance",
                    "lux",
                    illuminance,
                    [0., 100_000.],
                    response,
                );
                vector(ui, "Direction (local)", "", direction, response);
            }
        }
        _ => {}
    }
}
fn geometry_controls(ui: &Ui, geometry: &mut Geometry, response: &mut EditResponse) {
    match geometry {
        Geometry::Box { size, .. } => {
            for (label, value) in ["Width", "Height", "Depth"].into_iter().zip(size) {
                number(ui, label, "m", value, [1e-6, 1e6], response);
            }
        }
        Geometry::Sphere { radius, .. } => number(ui, "Radius", "m", radius, [1e-6, 1e6], response),
        Geometry::Mesh {
            positions, indices, ..
        } => ui.text_disabled(format!(
            "Mesh: {} vertices, {} triangles",
            positions.len(),
            indices.len() / 3
        )),
    }
}
fn material_controls(ui: &Ui, material: &mut Material, response: &mut EditResponse) {
    let mut color = material.color.map(f64::from);
    fold_ui::sdk::color_controls::authored(ui, &mut color, false, response);
    material.color = color.map(|value| value as f32);
    number(
        ui,
        "Roughness",
        "",
        &mut material.roughness,
        [0., 1.],
        response,
    );
    number(
        ui,
        "Reflectance",
        "",
        &mut material.reflectance,
        [0., 1.],
        response,
    );
    let changed = ui.checkbox("Unlit", &mut material.unlit);
    response.item(ui, changed);
}

fn property(
    ui: &Ui,
    label: &str,
    parameter: &mut Parameter,
    time: Time,
    auto_key: bool,
    response: &mut EditResponse,
) -> Result<(), String> {
    let _id = ui.push_id(&parameter.id.to_string());
    let binding = parameter.binding()?;
    let mut literal = crate::evaluation::sample(parameter, time)?;
    let offset =
        matches!(&binding, Binding::Track { track } if track.combination != Combination::Replace);
    let _disabled = ui.begin_disabled_with_cond(parameter.protected || offset);
    ui.text(label);
    let mut force_key = false;
    let mut clear = false;
    let mut remove = false;
    if let Some(_menu) = ui.begin_popup_context_item_with_label(Some("animation")) {
        force_key = ui.menu_item("Add or update keyframe");
        let keyed = matches!(&binding, Binding::Track { track } if track.keys.len() > 1 && track.keys.iter().any(|key| key.time == time));
        remove = ui.menu_item_enabled_selected_no_shortcut("Remove keyframe", false, keyed);
        clear = ui.menu_item_enabled_selected_no_shortcut(
            "Remove animation",
            false,
            matches!(binding, Binding::Track { .. }),
        );
    }
    let rotation = matches!(literal, Literal::Rotation { .. });
    let mut values = match &literal {
        Literal::Vector { value, .. } => value.clone(),
        Literal::Rotation { .. } => literal.euler_xyz()?.map(f64::to_degrees).to_vec(),
        _ => return Err("Unsupported 3D transform control".into()),
    };
    let mut edit = EditResponse::default();
    for (index, value) in values.iter_mut().enumerate() {
        NumericProperty {
            id: ["x", "y", "z"][index],
            label: ["X", "Y", "Z"][index],
            unit: if rotation {
                "°"
            } else if label == "Position" {
                "m"
            } else {
                ""
            },
            speed: if rotation { 0.5 } else { 0.01 },
            range: (label == "Scale").then_some([1e-6, 1e6]),
            default: None,
        }
        .draw(ui, value, &mut edit);
    }
    if offset {
        ui.text_disabled("Offset animation: edit in Animation.");
    }
    if matches!(binding, Binding::Track { .. }) {
        ui.text_disabled("Animated");
    }
    if !edit.changed && !force_key && !clear && !remove {
        response.began |= edit.began;
        response.finished |= edit.finished;
        return Ok(());
    }
    if edit.changed {
        match &mut literal {
            Literal::Vector { value, .. } => *value = values,
            Literal::Rotation { .. } => {
                literal = Literal::euler(
                    [
                        values[0].to_radians(),
                        values[1].to_radians(),
                        values[2].to_radians(),
                    ],
                    EulerOrder::Xyz,
                    Space::World,
                )?
            }
            _ => unreachable!(),
        }
    }
    let next = if clear {
        Binding::Literal { value: literal }
    } else if remove {
        let Binding::Track { mut track } = binding else {
            unreachable!()
        };
        track.keys.retain(|key| key.time != time);
        Binding::Track { track }
    } else {
        edited_binding(binding, literal, time, auto_key || force_key)?
    };
    parameter.value = serde_json::to_value(next).map_err(|e| e.to_string())?;
    response.changed = true;
    response.began |= edit.began;
    response.finished |= edit.finished || force_key || clear || remove;
    Ok(())
}
