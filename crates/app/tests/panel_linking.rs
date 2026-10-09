#![cfg(feature = "native-probe")]
use fold_app::session::Session;
use fold_foundation::{ObjectId, Time};
use fold_platform::{
    desktop::DesktopClient,
    image_source::Source,
    workspace::{LinkGroup, Workspace},
};
use fold_project::{EditBatch, Mutation};
use fold_ui::WorkspaceProbe;

fn panels() -> fold_ui::sdk::PanelRegistry {
    let mut panels = fold_ui::sdk::PanelRegistry::new(&fold_app::packages::builtins());
    fold_app::project_panel::register(&mut panels).unwrap();
    fold_timeline::ui::register(&mut panels).unwrap();
    fold_compositor::ui::register(&mut panels).unwrap();
    fold_composition::ui::register(&mut panels).unwrap();
    fold_motion::ui::register(&mut panels).unwrap();
    panels
}

#[test]
fn native_grouped_selection_is_stable_and_routes_inspector_network_and_occurrences() {
    let path = std::env::var_os("FOLD_SELECTION_PROJECT")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(|| {
            std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../demos/m5b-background-spheres.fold")
        });
    let mut project = fold_project::load(&path, 8).unwrap();
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_composition::COMPOSITION)
        .unwrap();
    let composition_id = document.id;
    let mut composition = fold_composition::Composition::from_document(document).unwrap();
    let grade = composition
        .layers
        .iter()
        .find(|l| l.name == "Explicit background grade")
        .unwrap()
        .id;
    let background = composition
        .layers
        .iter()
        .find(|l| l.name == "Background spheres")
        .unwrap()
        .clone();
    let Some(Source::Output(source)) = &background.source else {
        panic!("scene source")
    };
    let scene_id = source.document;
    let scene =
        fold_motion::scene::Scene::from_document(&snapshot.state().documents[&scene_id]).unwrap();
    let camera = scene.objects.iter().find(|o| o.name == "Camera").unwrap();
    let light = scene.objects.iter().find(|o| o.name == "Light").unwrap();
    let camera_node = camera.graph.definitions[0].nodes[0].id;
    let objects = [camera.id, light.id];
    let mut session = Session::new(project);
    let mut probe = WorkspaceProbe::new(panels());
    let parent = probe.bind(fold_composition::COMPOSITION, composition_id, LinkGroup::A);
    for _ in 0..4 {
        probe.frame(&mut session, 1600.);
    }
    let revision = session.snapshot().unwrap().revision();
    let delivery = session.output_info().unwrap().0;

    // Original camera -> light -> grade failure, with the real panels visible.
    for width in [1600., 580.] {
        for object in objects.into_iter().map(Some).chain(std::iter::once(None)) {
            let row = object
                .map(|o| probe.row(parent, scene_id, o, Some(background.id)))
                .unwrap_or(grade);
            probe.select(parent, vec![row]);
            for _ in 0..16 {
                probe.frame(&mut session, width);
                assert_eq!(
                    probe.workspace().editors[&parent].selection.objects,
                    vec![row],
                    "selection changed without input"
                );
                let network = probe.workspace().network_target(LinkGroup::A).unwrap();
                let inspector = probe.workspace().inspector_editor().unwrap();
                if let Some(object) = object {
                    assert_eq!(
                        probe.workspace().editors[&network].document(),
                        Some(scene_id)
                    );
                    assert_eq!(
                        probe.workspace().editors[&network].selection.objects,
                        vec![object]
                    );
                    assert_eq!(
                        inspector, network,
                        "scene parameters must follow the object"
                    );
                } else {
                    let Some(Source::Output(grade_source)) =
                        &composition.layer(grade).unwrap().source
                    else {
                        panic!("grade source")
                    };
                    assert_eq!(
                        probe.workspace().editors[&network].document(),
                        Some(grade_source.document)
                    );
                    assert_eq!(
                        inspector, parent,
                        "image layer parameters retain their occurrence"
                    );
                }
            }
        }
    }

    let camera_row = probe.row(parent, scene_id, objects[0], Some(background.id));
    let light_row = probe.row(parent, scene_id, objects[1], Some(background.id));
    probe.select(parent, vec![camera_row]);
    probe.frame(&mut session, 1600.);
    let graph = probe.workspace().network_target(LinkGroup::A).unwrap();
    probe.workspace_mut().toggle_inspector_lock();
    probe.select(parent, vec![light_row]);
    for _ in 0..8 {
        probe.frame(&mut session, 1600.);
        assert_eq!(
            probe
                .workspace()
                .inspector_lock
                .as_ref()
                .unwrap()
                .1
                .selection
                .objects,
            vec![objects[0]]
        );
        assert_eq!(
            probe.workspace().editors[&graph].selection.objects,
            vec![objects[1]],
            "Inspector lock must not redirect Network"
        );
    }
    probe.workspace_mut().toggle_inspector_lock();

    // A different visible composition/group must not become the reverse-link parent.
    let other = probe.bind(fold_composition::COMPOSITION, composition_id, LinkGroup::B);
    probe.frame(&mut session, 1600.);
    let other_camera = probe.row(other, scene_id, objects[0], Some(background.id));
    probe.select(other, vec![other_camera]);
    probe.frame(&mut session, 1600.);
    let graph_b = probe.workspace().network_target(LinkGroup::B).unwrap();
    assert_ne!(graph_b, graph);
    assert_eq!(probe.workspace().network_target(LinkGroup::A), Some(graph));
    probe.select(graph, vec![objects[0], camera_node]);
    for _ in 0..8 {
        probe.frame(&mut session, 1600.);
    }
    assert_eq!(
        probe.workspace().editors[&parent].selection.objects,
        vec![camera_row]
    );
    assert_eq!(
        probe.workspace().editors[&other].selection.objects,
        vec![other_camera]
    );
    assert_eq!(
        probe.workspace().editors[&graph].selection.objects,
        vec![objects[0], camera_node]
    );
    assert_eq!(session.snapshot().unwrap().revision(), revision);
    assert_eq!(session.output_info().unwrap().0, delivery);

    // Add a second occurrence only in this in-memory fixture, then verify its clock.
    let mut alias = background;
    alias.id = ObjectId::new();
    alias.name = "Second occurrence".into();
    alias.origin = Time::new(1, 1).unwrap();
    let alias_id = alias.id;
    composition.layers.push(alias);
    project = fold_project::load(&path, 8).unwrap();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                composition.document(composition_id).unwrap(),
            )],
        })
        .unwrap();
    session = Session::new(project);
    let viewer = *probe.workspace().viewers.keys().next().unwrap();
    probe.workspace_mut().viewers.get_mut(&viewer).unwrap().time = Time::new(2, 1).unwrap();
    probe.workspace_mut().composition_editor = Some(parent);
    probe.frame(&mut session, 1600.);
    let alias_row = probe.row(parent, scene_id, objects[0], Some(alias_id));
    assert_ne!(alias_row, camera_row);
    probe.select(parent, vec![alias_row]);
    probe.frame(&mut session, 1600.);
    let graph = probe.workspace().network_target(LinkGroup::A).unwrap();
    assert_eq!(
        probe.workspace().editors[&graph]
            .navigation
            .last()
            .unwrap()
            .time,
        Time::new(1, 1).unwrap()
    );
    probe.seek(parent, Time::new(3, 1).unwrap(), &mut session);
    probe.frame(&mut session, 1600.);
    assert_eq!(
        probe.workspace().editors[&graph]
            .navigation
            .last()
            .unwrap()
            .time,
        Time::new(2, 1).unwrap(),
        "linked parameter time must follow the composition viewer"
    );
    probe.select(graph, vec![objects[0], camera_node]);
    probe.frame(&mut session, 1600.);
    assert_eq!(
        probe.workspace().editors[&parent].selection.objects,
        vec![alias_row]
    );
    let saved = serde_json::to_vec(probe.workspace()).unwrap();
    probe.restore(Workspace::decode(&saved, "").unwrap());
    for _ in 0..8 {
        probe.frame(&mut session, 1600.);
    }
    assert_eq!(
        probe.row(parent, scene_id, objects[0], Some(alias_id)),
        alias_row
    );
    assert_eq!(
        probe.workspace().editors[&parent].selection.objects,
        vec![alias_row]
    );
    assert_eq!(
        probe.workspace().editors[&graph].selection.objects,
        vec![objects[0], camera_node]
    );
    assert_eq!(probe.workspace().editors[&graph].document(), Some(scene_id));
    // Selection and graph inspection stay available before the occurrence starts.
    probe.seek(parent, Time::ZERO, &mut session);
    probe.frame(&mut session, 1600.);
    assert_eq!(
        probe.workspace().editors[&graph]
            .navigation
            .last()
            .unwrap()
            .time,
        Time::new(-1, 1).unwrap()
    );
    Workspace::decode(&serde_json::to_vec(probe.workspace()).unwrap(), "").unwrap();
}

#[test]
#[ignore = "native GPU capture; writes /tmp/fold-selection-diagnosis/*.ppm"]
fn native_linking_normal_and_narrow() {
    let project = fold_project::load(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../demos/m5b-background-spheres.fold"),
        8,
    )
    .unwrap();
    let snapshot = project.snapshot();
    let document = snapshot
        .state()
        .documents
        .values()
        .find(|d| d.type_id == fold_composition::COMPOSITION)
        .unwrap();
    let composition = fold_composition::Composition::from_document(document).unwrap();
    let background = composition
        .layers
        .iter()
        .find(|l| l.name == "Background spheres")
        .unwrap();
    let grade = composition
        .layers
        .iter()
        .find(|l| l.name == "Explicit background grade")
        .unwrap()
        .id;
    let Some(Source::Output(source)) = &background.source else {
        panic!("scene")
    };
    let scene =
        fold_motion::scene::Scene::from_document(&snapshot.state().documents[&source.document])
            .unwrap();
    let camera = scene
        .objects
        .iter()
        .find(|o| o.name == "Camera")
        .unwrap()
        .id;
    let light = scene.objects.iter().find(|o| o.name == "Light").unwrap().id;
    let mut session = Session::new(project);
    let mut probe = WorkspaceProbe::new_gpu(panels());
    let parent = probe.bind(fold_composition::COMPOSITION, document.id, LinkGroup::A);
    std::fs::create_dir_all("/tmp/fold-selection-diagnosis").unwrap();
    for _ in 0..4 {
        probe.frame(&mut session, 1600.);
    }
    for (size, width) in [("normal", 1600.), ("narrow", 580.)] {
        for (name, object) in [
            ("camera", Some(camera)),
            ("light", Some(light)),
            ("grade", None),
        ] {
            let row = object
                .map(|o| probe.row(parent, source.document, o, Some(background.id)))
                .unwrap_or(grade);
            probe.select(parent, vec![row]);
            for _ in 0..20 {
                probe.frame(&mut session, width);
            }
            probe.capture(&std::path::PathBuf::from(format!(
                "/tmp/fold-selection-diagnosis/{name}-{size}.ppm"
            )));
        }
    }
}
