use super::*;
use crate::{Composition, Layer};
use fold_foundation::DocumentId;
use fold_platform::{desktop::*, image_source::Source, packages::PackageRegistry};
use fold_project::{CommittedSnapshot, DocumentRef, EditBatch, Mutation, Project};
use sdk::imgui;
pub(super) static IMGUI_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());
pub(super) struct Host {
    pub project: Project,
    pub state: DesktopState,
    registry: PackageRegistry,
    pub previews: usize,
    networks: Vec<DocumentId>,
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn has_network(&self, document: &fold_project::Document) -> bool {
        self.networks.contains(&document.id) || document.extensions.contains_key(crate::LAYER_GRAPH)
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        match command {
            DesktopCommand::Select(s) => self.state.selection = s,
            DesktopCommand::Navigate(v) => {
                self.state.selection = Selection {
                    document: Some(v.document),
                    objects: vec![],
                };
                self.state.navigation.push(v);
            }
            DesktopCommand::NavigateBack => {
                self.state.navigation.pop();
                self.state.selection.document = self.state.navigation.last().map(|v| v.document);
            }
            DesktopCommand::CancelPreviewEdit => {}
            DesktopCommand::PreviewExtension(r) => {
                self.registry
                    .stage(&self.project.snapshot(), &r, &Default::default())
                    .unwrap();
                self.previews += 1;
            }
            DesktopCommand::Extension(r) => {
                match self
                    .registry
                    .stage(&self.project.snapshot(), &r, &Default::default())
                    .and_then(|b| {
                        self.project
                            .commit(b)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    }) {
                    Ok(()) => {}
                    Err(e) => self.state.status = e,
                }
            }
            DesktopCommand::Notify(e) => self.state.status = e,
            other => panic!("Unexpected command {other:?}"),
        }
    }
    fn video_info(&self, id: DocumentId) -> Result<fold_media::VideoInfo, String> {
        self.registry.output(&self.project.snapshot(), id)
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
pub(super) fn setup() -> (Host, Shared) {
    let mut registry = PackageRegistry::default();
    crate::package::register(&mut registry).unwrap();
    let child = DocumentId::new();
    let id = DocumentId::new();
    let nested = Composition::default();
    let mut root = Composition::default();
    let mut layer = Layer::new(
        "Foreground graph",
        Some(Source::Output(DocumentRef {
            document: child,
            output: "video".into(),
            extensions: Default::default(),
        })),
        root.info.clone(),
    )
    .unwrap();
    let selected = layer.id;
    layer.animation.insert(
        "scene.opacity.0".into(),
        fold_animation::Curve {
            keys: vec![
                fold_animation::Key::new(fold_foundation::Time::ZERO, 0.),
                fold_animation::Key::new(fold_foundation::Time::new(1, 1).unwrap(), 1.),
            ],
        },
    );
    root.layers.push(layer);
    root.layers
        .push(Layer::new("Background group", None, root.info.clone()).unwrap());
    let mut project = Project::new(16);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(nested.document(child).unwrap()),
                Mutation::PutDocument(root.document(id).unwrap()),
            ],
        })
        .unwrap();
    let mut host = Host {
        project,
        registry,
        state: DesktopState::default(),
        previews: 0,
        networks: vec![],
    };
    host.state.selection = Selection {
        document: Some(id),
        objects: vec![selected],
    };
    host.state.navigation.push(ViewLocation {
        document: id,
        time: fold_foundation::Time::new(1, 2).unwrap(),
        label: "Composition".into(),
    });
    let state = Shared::default();
    state.borrow_mut().sync(&mut host);
    (host, state)
}
#[test]
fn properties_preview_cancel_commit_undo_and_source_clock() {
    let (mut host, shared) = setup();
    let mut state = shared.borrow_mut();
    let id = state.selected[0];
    let before = host.project.snapshot();
    state
        .composition
        .as_mut()
        .unwrap()
        .layer_mut(id)
        .unwrap()
        .visible = false;
    state.preview(&mut host);
    assert_eq!(host.previews, 1);
    assert_eq!(host.project.snapshot().revision(), before.revision());
    state.cancel(&mut host);
    assert!(
        state
            .composition
            .as_ref()
            .unwrap()
            .layer(id)
            .unwrap()
            .visible
    );
    state
        .composition
        .as_mut()
        .unwrap()
        .layer_mut(id)
        .unwrap()
        .locked = true;
    state.commit(&mut host);
    assert!(state.composition.as_ref().unwrap().locked(id));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert!(!state.composition.as_ref().unwrap().locked(id));
    host.project.redo().unwrap();
    state.sync(&mut host);
    assert!(state.composition.as_ref().unwrap().locked(id));
    let parent = host.state.navigation[0].clone();
    state.open(&mut host, id).unwrap();
    assert_eq!(host.state.navigation.len(), 2);
    assert_eq!(host.state.navigation[1].time, parent.time);
    host.command(DesktopCommand::NavigateBack);
    assert_eq!(host.state.navigation[0], parent);
}
#[test]
fn common_hierarchy_and_inspector_fit_normal_and_narrow() {
    let _guard = IMGUI_LOCK.lock().unwrap();
    let (mut host, state) = setup();
    let before = host.project.snapshot();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let fonts = sdk::typography::Typography::install(&mut context);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1200., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = Canvas::new(state.clone());
    canvas.initialize(&context, Some(&fonts));
    let mut inspector = inspector::Inspector(state);
    for frame in 0..40 {
        let width = if frame % 20 < 10 { 700. } else { 280. };
        let ui = context.frame();
        ui.window("Composition")
            .position([0., 0.], imgui::Condition::Always)
            .size([width, 790.], imgui::Condition::Always)
            .build(|| {
                canvas.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                });
                if frame % 10 >= 3 {
                    assert_eq!(ui.scroll_max_x(), 0.);
                }
            });
        ui.window("Inspector")
            .position([width + 10., 0.], imgui::Condition::Always)
            .size([280., 790.], imgui::Condition::Always)
            .build(|| {
                inspector.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                });
                if frame % 10 >= 3 {
                    assert_eq!(ui.scroll_max_x(), 0.);
                }
            });
        assert!(context.render_legacy().total_vtx_count() > 100);
    }
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
}

#[test]
fn layer_range_and_toggle_selection_support_precompose_without_losing_anchor() {
    let (mut host, shared) = setup();
    let mut state = shared.borrow_mut();
    let ids: Vec<_> = state
        .composition
        .as_ref()
        .unwrap()
        .layers
        .iter()
        .map(|l| l.id)
        .collect();
    state.select_layer(&mut host, ids[1], true, false);
    assert_eq!(state.selected, ids);
    state.select_layer(&mut host, ids[1], false, true);
    assert_eq!(state.selected, vec![ids[0]]);
    state.select_layer(&mut host, ids[1], false, false);
    assert_eq!(state.selected, vec![ids[1]]);
}

#[test]
fn selecting_a_layer_resolves_its_graph_without_publishing_navigation() {
    use fold_platform::{panel_context::PanelContext, workspace::EditorInstance};
    let (mut host, shared) = setup();
    let parent = host.state.selection.document.unwrap();
    let layer = host.state.selection.objects[0];
    let mut composition =
        Composition::from_document(&host.project.snapshot().state().documents[&parent]).unwrap();
    let Some(Source::Output(source)) = &mut composition.layer_mut(layer).unwrap().source else {
        unreachable!()
    };
    let graph_id = source.document;
    let mut graph = (*host.project.snapshot().state().documents[&graph_id]).clone();
    graph
        .extensions
        .insert(crate::LAYER_GRAPH.into(), true.into());
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(graph),
                Mutation::PutDocument(composition.document(parent).unwrap()),
            ],
        })
        .unwrap();
    let mut binding = EditorInstance {
        group: fold_platform::workspace::LinkGroup::A,
        contribution: package::PANEL.into(),
        document_type: crate::COMPOSITION.into(),
        navigation: host.state.navigation.clone(),
        selection: host.state.selection.clone(),
        mapped_navigation: false,
    };
    let navigation = binding.navigation.clone();
    let mut scoped = PanelContext::new(&mut host, &mut binding);
    let mut state = shared.borrow_mut();
    state.sync(&mut scoped);
    state.select(&mut scoped, vec![layer]);
    assert!(scoped.take_layer_graph().is_none());
    let graph = state.selection_target(&scoped).unwrap().unwrap();
    assert_eq!(graph.location.document, graph_id);
    assert!(graph.objects.is_empty());
    assert!(!graph.inspect_content);
    assert_eq!(graph.location.time, navigation[0].time);
    assert_eq!(scoped.state().selection.document, Some(parent));
    assert_eq!(scoped.state().selection.objects, vec![layer]);
    drop(scoped);
    assert_eq!(binding.navigation, navigation);
    assert_eq!(binding.selection.objects, vec![layer]);
}

#[test]
fn reselecting_the_only_layer_requests_its_network_again() {
    let (mut host, shared) = setup();
    let mut state = shared.borrow_mut();
    let id = state.selected[0];
    let child = match &state
        .composition
        .as_ref()
        .unwrap()
        .layer(id)
        .unwrap()
        .source
    {
        Some(Source::Output(source)) => source.document,
        _ => unreachable!(),
    };
    let mut document = (*host.project.snapshot().state().documents[&child]).clone();
    document
        .extensions
        .insert(crate::LAYER_GRAPH.into(), true.into());
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    state.sync(&mut host);
    state
        .composition
        .as_mut()
        .unwrap()
        .layers
        .retain(|l| l.id == id);
    state.commit(&mut host);
    let mut editor = fold_platform::workspace::EditorInstance {
        contribution: package::PANEL.into(),
        document_type: crate::COMPOSITION.into(),
        group: fold_platform::workspace::LinkGroup::A,
        navigation: host.state.navigation.clone(),
        selection: host.state.selection.clone(),
        mapped_navigation: false,
    };
    let mut scoped = fold_platform::panel_context::PanelContext::new(&mut host, &mut editor);
    assert!(state.selection_target(&scoped).unwrap().is_some());
    assert!(!scoped.selection_requested());
    state.select_layer(&mut scoped, id, false, false);
    assert!(
        scoped.selection_requested(),
        "reselection must publish explicit intent for the shared router"
    );
}

#[test]
fn delete_key_removes_selected_layer_and_undo_restores_it() {
    let _guard = IMGUI_LOCK.lock().unwrap();
    let (mut host, state) = setup();
    let before = host.project.snapshot();
    let selected = state.borrow().selected[0];
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([900., 700.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut canvas = Canvas::new(state.clone());
    for step in 0..6 {
        context
            .io_mut()
            .add_key_event(imgui::Key::Delete, step == 3);
        let ui = context.frame();
        ui.window("Composition")
            .position([0., 0.], imgui::Condition::Always)
            .size([700., 600.], imgui::Condition::Always)
            .focused(true)
            .build(|| {
                canvas.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                });
            });
        context.end_frame();
    }
    assert!(
        state
            .borrow()
            .composition
            .as_ref()
            .unwrap()
            .layer(selected)
            .is_err()
    );
    host.project.undo().unwrap();
    assert!(before.same_content(&host.project.snapshot()));
}

#[test]
fn deleting_a_group_protects_locked_descendants_and_undo_restores_hierarchy() {
    let (mut host, shared) = setup();
    let mut state = shared.borrow_mut();
    let child = state.selected[0];
    let c = state.composition.as_mut().unwrap();
    let group = c.layers[1].id;
    c.layer_mut(child).unwrap().parent = Some(group);
    c.layer_mut(child).unwrap().locked = true;
    state.commit(&mut host);
    state.select(&mut host, vec![group]);
    let locked = host.project.snapshot();
    actions::delete_selected(&mut host, &mut state);
    assert!(locked.same_content(&host.project.snapshot()));
    assert_eq!(state.error, "Unlock descendant layers before deleting");
    state
        .composition
        .as_mut()
        .unwrap()
        .layer_mut(child)
        .unwrap()
        .locked = false;
    state.commit(&mut host);
    let before = host.project.snapshot();
    actions::delete_selected(&mut host, &mut state);
    assert!(state.composition.as_ref().unwrap().layers.is_empty());
    host.project.undo().unwrap();
    assert!(before.same_content(&host.project.snapshot()));
}

#[test]
fn standalone_graph_selection_uses_network_capability_without_internal_marker() {
    use fold_platform::{panel_context::PanelContext, workspace::EditorInstance};
    let (mut host, shared) = setup();
    let layer = host.state.selection.objects[0];
    let source = match &shared
        .borrow()
        .composition
        .as_ref()
        .unwrap()
        .layer(layer)
        .unwrap()
        .source
    {
        Some(Source::Output(source)) => source.document,
        _ => unreachable!(),
    };
    assert!(
        !host.project.snapshot().state().documents[&source]
            .extensions
            .contains_key(crate::LAYER_GRAPH)
    );
    host.networks.push(source);
    let mut binding = EditorInstance {
        group: fold_platform::workspace::LinkGroup::A,
        contribution: package::PANEL.into(),
        document_type: crate::COMPOSITION.into(),
        navigation: host.state.navigation.clone(),
        selection: host.state.selection.clone(),
        mapped_navigation: false,
    };
    let navigation = binding.navigation.clone();
    let mut scoped = PanelContext::new(&mut host, &mut binding);
    let mut state = shared.borrow_mut();
    state.sync(&mut scoped);
    let target = state.selection_target(&scoped).unwrap().unwrap();
    assert_eq!(target.location.document, source);
    state.open(&mut scoped, layer).unwrap();
    assert!(scoped.take_layer_graph().is_some());
    drop(scoped);
    assert_eq!(binding.navigation, navigation);
}
