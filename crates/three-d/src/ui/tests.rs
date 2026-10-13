use super::*;
use crate::{editing::Edit, model::Scene};
use fold_foundation::{DocumentId, Time};
use fold_graph::{
    binding::{Binding, Combination, Track},
    types::{EulerOrder, Literal, Space, Unit},
};
use fold_platform::{
    desktop::{DesktopState, PreviewKey, PreviewResult},
    packages::PackageRegistry,
};
use fold_project::{EditBatch, EditSession, Mutation, Project};

struct Host {
    project: Project,
    registry: PackageRegistry,
    desktop: DesktopState,
    preview: Option<EditSession>,
}
impl DesktopClient for Host {
    fn poll(&mut self) {}
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
    fn state(&self) -> &DesktopState {
        &self.desktop
    }
    fn snapshot(&self) -> Option<fold_project::CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn command(&mut self, command: DesktopCommand) {
        match command {
            DesktopCommand::PreviewExtension(request) => {
                let batch = self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .unwrap();
                let preview = self
                    .preview
                    .get_or_insert_with(|| self.project.begin_edit());
                self.project.update_edit(preview, batch.mutations).unwrap();
            }
            DesktopCommand::Extension(request) => {
                let batch = self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .unwrap();
                self.project.commit(batch).unwrap();
                self.preview = None;
            }
            DesktopCommand::CancelPreviewEdit => self.preview = None,
            DesktopCommand::Select(selection) => self.desktop.selection = selection,
            _ => panic!("unexpected 3D editor command"),
        }
    }
}

#[test]
fn scene_controls_preview_cancel_commit_once_and_observe_undo() {
    let id = DocumentId::new();
    let mut scene = Scene::new(fold_media::VideoInfo {
        width: 640,
        height: 360,
        rate: [30, 1],
        frames: 150,
    });
    let camera = ObjectId::new();
    let mut second_camera = scene.objects[&scene.active_camera].clone();
    second_camera.name = "Second camera".into();
    for parameter in [
        &mut second_camera.transform.position,
        &mut second_camera.transform.rotation,
        &mut second_camera.transform.scale,
    ] {
        parameter.id = ObjectId::new();
    }
    scene.roots.push(camera);
    scene.objects.insert(camera, second_camera);
    let object = scene.roots[0];
    scene
        .objects
        .get_mut(&object)
        .unwrap()
        .extensions
        .insert("vendor.note".into(), serde_json::json!("retained"));
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(scene.document(id).unwrap())],
        })
        .unwrap();
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let mut host = Host {
        project,
        registry,
        preview: None,
        desktop: DesktopState {
            selection: Selection {
                document: Some(id),
                objects: vec![object],
            },
            ..Default::default()
        },
    };
    let before = host.project.snapshot();
    let mut state = state::State::default();
    state.sync(&mut host);
    let edit = |x| {
        let mut value = scene.objects[&object].clone();
        value.transform.position.value = serde_json::to_value(Binding::Literal {
            value: Literal::Vector {
                value: vec![x, 0., 0.],
                unit: Unit::Meters,
                space: Space::World,
            },
        })
        .unwrap();
        Edit::Replace {
            id: object,
            expected: Box::new(scene.objects[&object].clone()),
            value: Box::new(value),
        }
    };
    state.preview(&mut host, vec![edit(1.)]).unwrap();
    state.preview(&mut host, vec![edit(2.)]).unwrap();
    assert_eq!(host.project.snapshot().state(), before.state());
    assert!(host.preview.is_some());
    state.cancel(&mut host);
    assert!(host.preview.is_none());
    assert_eq!(state.current(), Some(&scene));
    state.preview(&mut host, vec![edit(3.)]).unwrap();
    state.commit(&mut host).unwrap();
    state.sync(&mut host);
    assert_eq!(
        state.current().unwrap().objects[&object].extensions,
        scene.objects[&object].extensions
    );
    assert_ne!(host.project.snapshot().state(), before.state());
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    assert!(host.project.snapshot().revision() > before.revision());
    assert_eq!(state.current(), Some(&scene));
    state.preview(&mut host, vec![edit(4.)]).unwrap();
    host.desktop.playing = true;
    state.sync(&mut host);
    assert!(host.preview.is_none());
    assert_eq!(state.current(), Some(&scene));
    host.desktop.playing = false;
    let Content::Instance {
        geometry, material, ..
    } = scene.objects[&object].content
    else {
        unreachable!()
    };
    let mut shape = scene.geometry[&geometry].clone();
    let crate::model::Geometry::Box { size, .. } = &mut shape else {
        unreachable!()
    };
    *size = [2., 3., 4.];
    let mut surface = scene.materials[&material].clone();
    surface.color = [4., 2., 0.5, 0.5];
    surface.roughness = 0.25;
    let resource_edits = vec![
        Edit::Geometry {
            id: geometry,
            expected: Some(scene.geometry[&geometry].clone()),
            value: Some(shape.clone()),
        },
        Edit::Material {
            id: material,
            expected: Some(scene.materials[&material].clone()),
            value: Some(surface.clone()),
        },
    ];
    state.preview(&mut host, resource_edits.clone()).unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    state.commit(&mut host).unwrap();
    state.sync(&mut host);
    assert_eq!(state.current().unwrap().geometry[&geometry], shape);
    assert_eq!(state.current().unwrap().materials[&material], surface);
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.current(), Some(&scene));
    let mut protected_scene = scene.clone();
    let shared = ObjectId::new();
    let mut protected = scene.objects[&object].clone();
    protected.transform = Default::default();
    protected.protected = true;
    protected_scene.roots.push(shared);
    protected_scene.objects.insert(shared, protected);
    assert!(crate::editing::resource_editable(&protected_scene, geometry, true).is_err());
    assert!(crate::editing::resource_editable(&protected_scene, material, false).is_err());
    assert!(crate::editing::apply(&protected_scene, &resource_edits, &Default::default()).is_err());
    let content_edits = scene
        .objects
        .iter()
        .filter_map(|(&id, expected)| {
            let mut value = expected.clone();
            match &mut value.content {
                Content::Camera {
                    projection, ev100, ..
                } => {
                    *projection = crate::model::Projection::Orthographic { vertical_size: 2.5 };
                    *ev100 = 9.;
                }
                Content::DirectionalLight {
                    illuminance, color, ..
                } => {
                    *illuminance = 3000.;
                    *color = [0.8, 0.7, 0.6];
                }
                _ => return None,
            }
            Some(Edit::Replace {
                id,
                expected: Box::new(expected.clone()),
                value: Box::new(value),
            })
        })
        .collect::<Vec<_>>();
    state.preview(&mut host, content_edits).unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    state.commit(&mut host).unwrap();
    state.sync(&mut host);
    let Content::Camera {
        projection, ev100, ..
    } = state.current().unwrap().objects[&scene.active_camera].content
    else {
        unreachable!()
    };
    assert_eq!(
        projection,
        crate::model::Projection::Orthographic { vertical_size: 2.5 }
    );
    assert_eq!(ev100, 9.);
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.current(), Some(&scene));
    let mut invalid = scene.objects[&scene.active_camera].clone();
    let Content::Camera { target, .. } = &mut invalid.content else {
        unreachable!()
    };
    *target = [0.; 3];
    assert!(
        state
            .preview(
                &mut host,
                vec![Edit::Replace {
                    id: scene.active_camera,
                    expected: Box::new(scene.objects[&scene.active_camera].clone()),
                    value: Box::new(invalid)
                }]
            )
            .is_err()
    );
    assert!(host.preview.is_none());
    let mut panels = panels();
    let mut context = sdk::imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_delta_time(1. / 60.);
    for width in [640., 260.] {
        for selected in &scene.roots {
            host.desktop.selection.objects = vec![*selected];
            context.io_mut().set_display_size([width * 2. + 30., 800.]);
            let ui = context.frame();
            ui.window("3D scene")
                .position([0., 0.], sdk::imgui::Condition::Always)
                .size([width, 600.], sdk::imgui::Condition::Always)
                .build(|| {
                    panels.editor.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
                });
            ui.window("3D properties")
                .position([width + 10., 0.], sdk::imgui::Condition::Always)
                .size([width, 600.], sdk::imgui::Condition::Always)
                .build(|| {
                    panels.inspector.draw(ExtensionUi {
                        ui,
                        host: &mut host,
                    });
                });
            drop(context.render_legacy());
        }
    }
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    // Exercise the actual button with an existing transform draft. Camera
    // selection must join that draft, not replace its publication request.
    host.desktop.selection.objects = vec![camera];
    let shared = Shared::default();
    shared.borrow_mut().sync(&mut host);
    let original = scene.objects[&camera].clone();
    let mut moved = original.clone();
    moved.transform.position.value = serde_json::to_value(Binding::Literal {
        value: Literal::Vector {
            value: vec![3., 0., 5.],
            unit: Unit::Meters,
            space: Space::World,
        },
    })
    .unwrap();
    shared
        .borrow_mut()
        .preview(
            &mut host,
            vec![Edit::Replace {
                id: camera,
                expected: Box::new(original),
                value: Box::new(moved.clone()),
            }],
        )
        .unwrap();
    let mut inspector = super::inspector::Inspector::new(shared);
    let mut center = [0.; 2];
    let mut frame = |context: &mut sdk::imgui::Context, host: &mut Host| {
        let ui = context.frame();
        ui.window("Camera interaction")
            .position([0.; 2], sdk::imgui::Condition::Always)
            .size([640., 780.], sdk::imgui::Condition::Always)
            .build(|| {
                inspector.draw(ExtensionUi { ui, host });
                let a = ui.item_rect_min();
                let b = ui.item_rect_max();
                center = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
            });
        drop(context.render_legacy());
        center
    };
    context.io_mut().set_display_size([1000., 900.]);
    for _ in 0..3 {
        frame(&mut context, &mut host);
    }
    let center = frame(&mut context, &mut host);
    context.io_mut().add_mouse_pos_event(center);
    context
        .io_mut()
        .add_mouse_button_event(sdk::imgui::MouseButton::Left, true);
    frame(&mut context, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(sdk::imgui::MouseButton::Left, false);
    frame(&mut context, &mut host);
    let committed = Scene::from_document(&host.project.snapshot().state().documents[&id]).unwrap();
    assert_eq!(committed.active_camera, camera);
    assert_eq!(committed.objects[&camera], moved);
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    let mut renamed = scene.objects[&object].clone();
    renamed.name = "Hero box".into();
    let rename = Edit::Replace {
        id: object,
        expected: Box::new(scene.objects[&object].clone()),
        value: Box::new(renamed.clone()),
    };
    let mut naming = state::State::default();
    naming.sync(&mut host);
    naming.preview(&mut host, vec![rename]).unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    naming.commit(&mut host).unwrap();
    naming.sync(&mut host);
    assert_eq!(naming.current().unwrap().objects[&object].name, "Hero box");
    host.project.undo().unwrap();
    naming.sync(&mut host);
    assert_eq!(naming.current(), Some(&scene));
    renamed.name.clear();
    assert!(
        naming
            .preview(
                &mut host,
                vec![Edit::Replace {
                    id: object,
                    expected: Box::new(scene.objects[&object].clone()),
                    value: Box::new(renamed),
                }]
            )
            .is_err()
    );
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );

    host.desktop.selection.objects = vec![scene.roots[2], scene.roots[0]];
    let mut grouping = state::State::default();
    grouping.sync(&mut host);
    grouping.group(&mut host).unwrap();
    grouping.sync(&mut host);
    let grouped = grouping.current().unwrap();
    let group_id = host.desktop.selection.objects[0];
    let Content::Group { children, .. } = &grouped.objects[&group_id].content else {
        unreachable!()
    };
    assert_eq!(children, &[scene.roots[0], scene.roots[2]]);
    assert_eq!(grouped.roots[0], group_id);
    for id in children {
        assert_eq!(grouped.objects[id], scene.objects[id]);
    }
    assert_eq!(grouped.geometry, scene.geometry);
    assert_eq!(grouped.materials, scene.materials);
    assert!(crate::tools::group(grouped, &[group_id, children[0]]).is_err());
    let child = children[0];
    let grouped = grouped.clone();
    let selection = host.desktop.selection.clone();
    assert!(crate::tools::move_to(&grouped, group_id, Some(group_id)).is_err());
    grouping.move_to(&mut host, child, None, true).unwrap();
    grouping.sync(&mut host);
    let moved = grouping.current().unwrap();
    assert_eq!(moved.roots.last(), Some(&child));
    assert_eq!(moved.objects[&child], grouped.objects[&child]);
    assert_eq!(moved.geometry, grouped.geometry);
    assert_eq!(moved.materials, grouped.materials);
    assert_eq!(host.desktop.selection, selection);
    grouping
        .move_to(&mut host, child, Some(group_id), false)
        .unwrap();
    grouping.sync(&mut host);
    assert_eq!(
        crate::editing::placement(grouping.current().unwrap(), child)
            .unwrap()
            .parent,
        Some(group_id)
    );
    host.project.undo().unwrap();
    host.project.undo().unwrap();
    grouping.sync(&mut host);
    assert_eq!(grouping.current(), Some(&grouped));
    host.project.undo().unwrap();
    grouping.sync(&mut host);
    assert_eq!(grouping.current(), Some(&scene));
    host.desktop.selection.objects = vec![object];

    // Drive the real hierarchy controls at both normal and narrow widths.
    for width in [640., 260.] {
        host.desktop.selection.objects = vec![object];
        let mut editor = Editor::new(Shared::default());
        let mut draw = |context: &mut sdk::imgui::Context, host: &mut Host| {
            let ui = context.frame();
            let mut center = [0.; 2];
            ui.window("Hierarchy interaction")
                .focused(true)
                .position([0.; 2], sdk::imgui::Condition::Always)
                .size([width, 600.], sdk::imgui::Condition::Always)
                .build(|| {
                    editor.draw(ExtensionUi { ui, host });
                    let a = ui.item_rect_min();
                    let b = ui.item_rect_max();
                    center = [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5];
                });
            drop(context.render_legacy());
            center
        };
        for _ in 0..3 {
            draw(&mut context, &mut host);
        }
        let center = draw(&mut context, &mut host);
        context
            .io_mut()
            .add_key_event(sdk::imgui::Key::ModCtrl, true);
        context.io_mut().add_mouse_pos_event(center);
        draw(&mut context, &mut host);
        context
            .io_mut()
            .add_mouse_button_event(sdk::imgui::MouseButton::Left, true);
        draw(&mut context, &mut host);
        context
            .io_mut()
            .add_mouse_button_event(sdk::imgui::MouseButton::Left, false);
        draw(&mut context, &mut host);
        let last = *scene.roots.last().unwrap();
        assert_eq!(host.desktop.selection.objects, vec![object, last]);
        context.io_mut().add_key_event(sdk::imgui::Key::G, true);
        draw(&mut context, &mut host);
        context.io_mut().add_key_event(sdk::imgui::Key::G, false);
        context
            .io_mut()
            .add_key_event(sdk::imgui::Key::ModCtrl, false);
        draw(&mut context, &mut host);
        let committed =
            Scene::from_document(&host.project.snapshot().state().documents[&id]).unwrap();
        let group = host.desktop.selection.objects[0];
        let Content::Group { children, .. } = &committed.objects[&group].content else {
            panic!("Ctrl-G must select the published group")
        };
        let expected = scene
            .roots
            .iter()
            .copied()
            .filter(|id| *id == object || *id == last)
            .collect::<Vec<_>>();
        assert_eq!(children, &expected);
        host.project.undo().unwrap();
        assert_eq!(
            host.project.snapshot().state().documents,
            before.state().documents
        );
    }
    host.desktop.selection.objects = vec![object];
    let mut creation = state::State::default();
    creation.sync(&mut host);
    for kind in [
        crate::tools::Kind::Box,
        crate::tools::Kind::Sphere,
        crate::tools::Kind::Camera,
        crate::tools::Kind::Light,
        crate::tools::Kind::Group,
    ] {
        let selected_before = host.desktop.selection.clone();
        creation.create(&mut host, kind).unwrap();
        assert_eq!(
            host.desktop.selection, selected_before,
            "focus waits for publication observation"
        );
        creation.sync(&mut host);
        let created = host.desktop.selection.objects[0];
        let current = creation.current().unwrap();
        assert_eq!(current.objects.len(), scene.objects.len() + 1);
        assert!(current.objects.contains_key(&created));
        let names = current
            .objects
            .values()
            .map(|o| o.name.as_str())
            .collect::<std::collections::BTreeSet<_>>();
        assert_eq!(names.len(), current.objects.len());
        if let Content::Instance {
            geometry, material, ..
        } = current.objects[&created].content
        {
            assert!(!scene.geometry.contains_key(&geometry));
            assert!(!scene.materials.contains_key(&material));
            assert_eq!(
                current.materials[&material],
                crate::model::Material::default()
            );
        }
        host.project.undo().unwrap();
        creation.sync(&mut host);
        assert_eq!(creation.current(), Some(&scene));
    }
    creation
        .create(&mut host, crate::tools::Kind::Group)
        .unwrap();
    creation.sync(&mut host);
    let group = host.desktop.selection.objects[0];
    creation
        .create(&mut host, crate::tools::Kind::Sphere)
        .unwrap();
    creation.sync(&mut host);
    let child = host.desktop.selection.objects[0];
    let Content::Group { children, .. } = &creation.current().unwrap().objects[&group].content
    else {
        unreachable!()
    };
    assert_eq!(children, &[child]);
    assert!(!creation.current().unwrap().roots.contains(&child));
    let mut protected = creation.current().unwrap().clone();
    protected.objects.get_mut(&group).unwrap().protected = true;
    assert!(crate::tools::create(&protected, crate::tools::Kind::Box, Some(group)).is_err());
    assert!(crate::tools::delete(&protected, group).is_err());
    assert!(crate::tools::delete(creation.current().unwrap(), scene.active_camera).is_err());
    let with_group = creation.current().unwrap().clone();
    creation.delete(&mut host, group).unwrap();
    creation.sync(&mut host);
    assert_eq!(
        creation.current(),
        Some(&scene),
        "delete removes the subtree and its unused resources"
    );
    assert!(host.desktop.selection.objects.is_empty());
    host.project.undo().unwrap();
    creation.sync(&mut host);
    assert_eq!(creation.current(), Some(&with_group));
    host.project.undo().unwrap();
    host.project.undo().unwrap();
    creation.sync(&mut host);
    assert_eq!(creation.current(), Some(&scene));
    host.desktop.selection.objects = vec![object];
    creation.sync(&mut host);
    creation.duplicate(&mut host, object).unwrap();
    assert_eq!(host.desktop.selection.objects, vec![object]);
    creation.sync(&mut host);
    let duplicated = host.desktop.selection.objects[0];
    assert_ne!(duplicated, object);
    let shared = creation.current().unwrap().clone();
    assert_eq!(
        shared.objects[&duplicated].content,
        scene.objects[&object].content
    );
    creation.make_unique(&mut host, duplicated, false).unwrap();
    creation.sync(&mut host);
    assert_eq!(
        creation.current().unwrap().materials.len(),
        scene.materials.len() + 1
    );
    host.project.undo().unwrap();
    creation.sync(&mut host);
    assert_eq!(creation.current(), Some(&shared));
    host.project.undo().unwrap();
    creation.sync(&mut host);
    assert_eq!(creation.current(), Some(&scene));
    creation.create(&mut host, crate::tools::Kind::Box).unwrap();
    host.desktop.selection.objects = vec![object];
    creation.sync(&mut host);
    assert_eq!(
        host.desktop.selection.objects,
        vec![object],
        "publication must not steal a newer artist selection"
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    // Real ImGui pointer input exercises selection, preview, cancellation and
    // one Undo entry at normal and narrow sizes. This is not visual acceptance.
    for width in [640., 260.] {
        host.desktop.selection.objects.clear();
        let shared = Shared::default();
        let mut editor = Editor::new(shared.clone());
        let rect = sdk::ViewerRect {
            editable: true,
            image_current: true,
            origin: [10., 30.],
            size: [width - 20., (width - 20.) * 360. / 640.],
            canvas_origin: [10., 30.],
            canvas_size: [width - 20., (width - 20.) * 360. / 640.],
            dimensions: [640, 360],
        };
        let center = [
            rect.origin[0] + rect.size[0] * 0.5,
            rect.origin[1] + rect.size[1] * 0.5,
        ];
        let mode = super::viewport::Mode::Move;
        let mut frame = |context: &mut sdk::imgui::Context, host: &mut Host, mode| {
            editor.viewport.mode = mode;
            let ui = context.frame();
            ui.window("3D viewer input")
                .position([0.; 2], sdk::imgui::Condition::Always)
                .size([width, 500.], sdk::imgui::Condition::Always)
                .focused(true)
                .build(|| editor.draw_viewer_overlay(ExtensionUi { ui, host }, rect));
            drop(context.render_legacy());
        };
        context.io_mut().set_display_size([1000., 900.]);
        for _ in 0..3 {
            frame(&mut context, &mut host, mode);
        }
        context.io_mut().add_mouse_pos_event(center);
        frame(&mut context, &mut host, mode);
        context
            .io_mut()
            .add_mouse_button_event(sdk::imgui::MouseButton::Left, true);
        frame(&mut context, &mut host, mode);
        context
            .io_mut()
            .add_mouse_button_event(sdk::imgui::MouseButton::Left, false);
        frame(&mut context, &mut host, mode);
        assert_eq!(host.desktop.selection.objects, vec![object]);
        for mode in [
            super::viewport::Mode::Move,
            super::viewport::Mode::Rotate,
            super::viewport::Mode::Scale,
        ] {
            for cancel in [true, false] {
                let (start, end) = match mode {
                    super::viewport::Mode::Move => (center, [center[0] + 30., center[1]]),
                    super::viewport::Mode::Rotate => {
                        ([center[0] + 60., center[1]], [center[0], center[1] - 60.])
                    }
                    super::viewport::Mode::Scale => (center, [center[0], center[1] - 30.]),
                };
                context.io_mut().add_mouse_pos_event(start);
                frame(&mut context, &mut host, mode);
                context
                    .io_mut()
                    .add_mouse_button_event(sdk::imgui::MouseButton::Left, true);
                frame(&mut context, &mut host, mode);
                context.io_mut().add_mouse_pos_event(end);
                frame(&mut context, &mut host, mode);
                assert!(host.preview.is_some());
                assert!(
                    shared.borrow().current() != Some(&scene),
                    "{mode:?}, width {width}, cancel {cancel}, error {}",
                    shared.borrow().error
                );
                assert_eq!(
                    host.project.snapshot().state().documents,
                    before.state().documents
                );
                if cancel {
                    context
                        .io_mut()
                        .add_key_event(sdk::imgui::Key::Escape, true);
                    frame(&mut context, &mut host, mode);
                    context
                        .io_mut()
                        .add_key_event(sdk::imgui::Key::Escape, false);
                }
                context
                    .io_mut()
                    .add_mouse_button_event(sdk::imgui::MouseButton::Left, false);
                frame(&mut context, &mut host, mode);
                assert!(host.preview.is_none());
                if !cancel {
                    assert_ne!(
                        host.project.snapshot().state().documents,
                        before.state().documents
                    );
                    host.project.undo().unwrap();
                    frame(&mut context, &mut host, mode);
                }
                assert_eq!(
                    host.project.snapshot().state().documents,
                    before.state().documents
                );
            }
        }
    }
}

#[test]
fn rotation_display_and_key_updates_preserve_native_intent() {
    for angles in [
        [0.2, -0.7, 1.1],
        [2., std::f64::consts::FRAC_PI_2, -0.4],
        [-0.8, -std::f64::consts::FRAC_PI_2, 1.4],
        [0.; 3],
    ] {
        let value = Literal::euler(angles, EulerOrder::Xyz, Space::World).unwrap();
        let restored =
            Literal::euler(value.euler_xyz().unwrap(), EulerOrder::Xyz, Space::World).unwrap();
        let (Literal::Rotation { value: a, .. }, Literal::Rotation { value: b, .. }) =
            (value, restored)
        else {
            unreachable!()
        };
        assert!((a.iter().zip(b).map(|(a, b)| a * b).sum::<f64>().abs() - 1.).abs() < 1e-12);
    }
    let literal = |x| Literal::Vector {
        value: vec![x, 0., 0.],
        unit: Unit::Meters,
        space: Space::World,
    };
    let mut track = Track::constant(literal(1.), Time::ZERO).unwrap();
    let original = track.keys[0].id;
    track
        .extensions
        .insert("vendor.note".into(), serde_json::json!("retained"));
    track.keys[0]
        .extensions
        .insert("vendor.key".into(), serde_json::json!(7));
    let Binding::Track { track: updated } = crate::controls::edited_binding(
        Binding::Track {
            track: track.clone(),
        },
        literal(2.),
        Time::ZERO,
        false,
    )
    .unwrap() else {
        unreachable!()
    };
    assert_eq!(updated.id, track.id);
    assert_eq!(updated.keys[0].id, original);
    assert_eq!(updated.keys[0].extensions, track.keys[0].extensions);
    assert_eq!(updated.extensions, track.extensions);
    let Binding::Track { track: inserted } = crate::controls::edited_binding(
        Binding::Track { track: updated },
        literal(3.),
        Time::new(1, 2).unwrap(),
        false,
    )
    .unwrap() else {
        unreachable!()
    };
    assert_eq!(inserted.keys.len(), 2);
    assert_eq!(inserted.keys[0].id, original);
    assert_eq!(inserted.keys[1].time, Time::new(1, 2).unwrap());
    track.combination = Combination::Add;
    track.base = Some(literal(0.));
    assert!(
        crate::controls::edited_binding(Binding::Track { track }, literal(3.), Time::ZERO, false)
            .is_err()
    );
}
