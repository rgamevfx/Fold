use super::*;
use fold_foundation::DocumentId;
use fold_platform::{desktop::*, packages::PackageRegistry};
use fold_project::{CommittedSnapshot, EditBatch, Mutation, Project};
pub(super) struct Host {
    project: Project,
    state: DesktopState,
    registry: PackageRegistry,
    previews: usize,
    cancels: usize,
}
impl DesktopClient for Host {
    fn hierarchy(
        &self,
        document: DocumentId,
    ) -> Result<fold_platform::hierarchy::Hierarchy, String> {
        self.registry.hierarchy(
            self.project
                .snapshot()
                .state()
                .documents
                .get(&document)
                .ok_or("Missing scene")?,
        )
    }
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        match command {
            DesktopCommand::Select(selection) => self.state.selection = selection,
            DesktopCommand::UnlockControls {
                document,
                object,
                targets,
                base,
            } => {
                match self.registry.unlock_for_artist(
                    &self.project.snapshot(),
                    base,
                    document,
                    object,
                    &targets,
                ) {
                    Ok(batch) => {
                        self.project
                            .commit_named(batch, "Unlock controls".into())
                            .unwrap();
                    }
                    Err(error) => self.state.status = error,
                }
            }
            DesktopCommand::CancelPreviewEdit => self.cancels += 1,
            DesktopCommand::PreviewExtension(request) => {
                if self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                    .is_ok()
                {
                    self.previews += 1;
                }
            }
            DesktopCommand::Extension(request) => {
                match self
                    .registry
                    .stage(&self.project.snapshot(), &request, &Default::default())
                {
                    Ok(batch) => {
                        self.project.commit(batch).unwrap();
                    }
                    Err(error) => self.state.status = error,
                }
            }
            _ => panic!("unexpected desktop command"),
        }
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
pub(super) fn setup() -> Host {
    let mut registry = PackageRegistry::default();
    package::register(&mut registry).unwrap();
    let document = DocumentId::new();
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                registry.create(crate::CONSTRUCTION, document).unwrap(),
            )],
        })
        .unwrap();
    Host {
        project,
        state: DesktopState {
            selection: Selection {
                document: Some(document),
                objects: vec![],
            },
            ..Default::default()
        },
        registry,
        previews: 0,
        cancels: 0,
    }
}

pub(super) fn scene_setup() -> Host {
    let mut host = setup();
    let id = DocumentId::new();
    let document = host.registry.create(crate::scene::TYPE, id).unwrap();
    let scene = crate::scene::Scene::from_document(&document).unwrap();
    host.project
        .commit(EditBatch {
            base: host.project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
        .unwrap();
    host.state.selection = Selection {
        document: Some(id),
        objects: vec![scene.objects[2].id],
    };
    host
}

#[test]
fn scene_object_selection_source_drafts_edits_and_unlock_are_scoped() {
    let mut host = scene_setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let document = state.document.unwrap();
    let first = state.object.unwrap();
    let original =
        crate::scene::Scene::from_document(&host.snapshot().unwrap().state().documents[&document])
            .unwrap();
    let draft = state.drafts.get_mut(&(document, Some(first))).unwrap();
    draft.text = "unfinished source".into();
    draft.dirty = true;
    let camera = original.objects[0].id;
    host.state.selection.objects = vec![camera];
    state.sync(&mut host);
    assert_eq!(state.object, Some(camera));
    assert_ne!(
        state.drafts[&(document, Some(camera))].text,
        "unfinished source"
    );
    let mut graph = state.construction.clone().unwrap();
    graph.definitions[0].nodes[0].name = "Render camera".into();
    assert!(state.commit(&mut host, graph.clone()));
    let committed =
        crate::scene::Scene::from_document(&host.snapshot().unwrap().state().documents[&document])
            .unwrap();
    assert_eq!(committed.objects[2], original.objects[2]);
    let node = graph.definitions[0].nodes[0].id;
    state.select(&mut host, vec![node]);
    assert_eq!(host.state.selection.objects, vec![camera, node]);
    state.sync(&mut host);
    assert_eq!(state.object, Some(camera));
    let parameter = graph.definitions[0].nodes[0]
        .parameters
        .values_mut()
        .next()
        .unwrap();
    parameter.protected = true;
    let target = parameter.id;
    assert!(state.commit(&mut host, graph));
    protection::unlock(&mut host, &mut state, target);
    assert!(state.error.is_empty(), "{}", state.error);
    assert!(
        !crate::protection::locked_ids(state.construction.as_ref().unwrap())
            .unwrap()
            .contains(&target)
    );
    host.state.selection.objects = vec![first];
    state.sync(&mut host);
    assert_eq!(
        state.drafts[&(document, Some(first))].text,
        "unfinished source"
    );
    host.state.selection.objects.clear();
    state.sync(&mut host);
    assert_eq!(state.object, None);
    host.project.undo().unwrap();
    host.project.undo().unwrap();
    host.project.undo().unwrap();
    assert_eq!(
        crate::scene::Scene::from_document(&host.snapshot().unwrap().state().documents[&document])
            .unwrap(),
        original
    );
}
#[test]
fn gestures_commit_once_cancel_and_keep_source_drafts_across_revision_changes() {
    let mut host = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let document = state.document.unwrap();
    let before = state.construction.clone().unwrap();
    let draft = state.drafts.get_mut(&(document, None)).unwrap();
    draft.text = "unfinished source".into();
    draft.dirty = true;
    let draft_base = draft.base;
    for value in [220., 240., 260.] {
        state.construction.as_mut().unwrap().definitions[0].inputs[0].default = Literal::Distance {
            value,
            unit: crate::types::Unit::Canvas,
        };
        state.preview(&mut host);
    }
    assert_eq!(host.previews, 3);
    assert_eq!(host.project.snapshot().revision(), draft_base);
    let changed = state.construction.clone().unwrap();
    assert!(state.commit(&mut host, changed.clone()));
    assert_eq!(state.construction.as_ref(), Some(&changed));
    assert_eq!(state.drafts[&(document, None)].text, "unfinished source");
    assert_eq!(state.drafts[&(document, None)].base, draft_base);
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    state.construction.as_mut().unwrap().definitions[0].inputs[0].default = Literal::Distance {
        value: 999.,
        unit: crate::types::Unit::Canvas,
    };
    state.preview(&mut host);
    state.cancel(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    assert!(host.cancels > 0);
    state.reload_source(&mut host);
    assert!(!state.drafts[&(document, None)].dirty);
    assert_eq!(
        crate::source::parse(&state.drafts[&(document, None)].text)
            .unwrap()
            .construction,
        before
    );
}
#[test]
fn native_panels_draw_at_normal_and_narrow_sizes_without_mutation() {
    let mut host = setup();
    let document = host.state.selection.document.unwrap();
    let current = host.project.snapshot();
    let construction =
        crate::model::Construction::from_document(&current.state().documents[&document]).unwrap();
    let height = construction.definitions[0].nodes[0].parameters["height"].id;
    let animated =
        crate::keyed::animate(&construction, height, fold_foundation::Time::ZERO).unwrap();
    host.project
        .commit(EditBatch {
            base: current.revision(),
            mutations: vec![Mutation::PutDocument(animated.document(document).unwrap())],
        })
        .unwrap();
    let before = host.project.snapshot();
    let shared = Shared::default();
    let mut editor = Editor(shared.clone(), Default::default());
    assert!(editor.supports_network());
    assert!(editor.is_layer_graph());
    let mut inspector = Inspector(shared);
    let _guard = UI_TEST_LOCK.lock().unwrap();
    let mut context = sdk::imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    let typography = sdk::typography::Typography::install(&mut context);
    editor.initialize(&context, Some(&typography));
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1100., 700.]);
    context.io_mut().set_delta_time(1. / 60.);
    for (index, width) in [700., 280., 700., 280.].into_iter().enumerate() {
        host.state.selection.objects = if index < 2 {
            vec![]
        } else {
            vec![animated.root.id]
        };
        let ui = context.frame();
        ui.window("Network")
            .position([0., 0.], sdk::imgui::Condition::Always)
            .size([width, 700.], sdk::imgui::Condition::Always)
            .build(|| {
                editor.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                })
            });
        ui.window("Inspector")
            .position([width, 0.], sdk::imgui::Condition::Always)
            .size(
                [if width > 500. { 360. } else { 240. }, 700.],
                sdk::imgui::Condition::Always,
            )
            .build(|| {
                inspector.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                })
            });
        let draw = context.render_legacy();
        assert!(draw.total_vtx_count() > 0);
    }
    assert_eq!(host.project.snapshot().revision(), before.revision());
}

#[test]
fn graph_rewires_preserve_expression_identity_and_reject_invalid_topology() {
    use crate::{
        binding::Binding,
        expression::{Binary, Expr},
        model::Reference,
    };
    use sdk::graph_canvas::{GraphChange, Socket, WireView};
    let host = setup();
    let document = host.state.selection.document.unwrap();
    let mut before = crate::model::Construction::from_document(
        &host.project.snapshot().state().documents[&document],
    )
    .unwrap();
    let definition = before.root.definition;
    let asset = &mut before.definitions[0];
    let mut alternate = asset.inputs[0].clone();
    alternate.id = fold_foundation::ObjectId::new();
    alternate.name = "Alternate width".into();
    let alternate_id = alternate.id;
    asset.inputs.push(alternate);
    let shape = asset.nodes[0].id;
    let parameter = asset.nodes[0].parameters.get_mut("width").unwrap();
    let Binding::Link { mut reference } = parameter.binding().unwrap() else {
        unreachable!()
    };
    let edge_id = reference.id;
    reference
        .extensions
        .insert("vendor".into(), serde_json::json!({"keep":true}));
    parameter.value = serde_json::to_value(Binding::Expression {
        expression: Expr::Binary {
            op: Binary::Multiply,
            left: Box::new(Expr::Reference { reference }),
            right: Box::new(Expr::Literal {
                value: Literal::Scalar { value: 2. },
            }),
        },
    })
    .unwrap();
    let view = super::graph::view(&before, document, definition, 0, vec![]).unwrap();
    let wire = view
        .wires
        .iter()
        .find(|wire| wire.target.node == shape)
        .unwrap()
        .clone();
    let alternate = view
        .nodes
        .iter()
        .find(|node| node.id == definition)
        .unwrap();
    let replacement = WireView {
        source: Socket {
            node: definition,
            key: alternate
                .outputs
                .iter()
                .find(|p| p.label == "Alternate width")
                .unwrap()
                .key
                .clone(),
        },
        target: wire.target.clone(),
    };
    let after = super::graph::apply(
        &before,
        definition,
        &[GraphChange::Rewire {
            remove: vec![wire.target.clone()],
            add: vec![replacement],
        }],
    )
    .unwrap();
    let binding = after.definitions[0].nodes[0].parameters["width"]
        .binding()
        .unwrap();
    let edges = crate::connections::edges(&binding).unwrap();
    assert_eq!(edges[0].id, edge_id);
    assert_eq!(
        edges[0].target,
        Reference::Input {
            input: alternate_id
        }
    );
    assert_eq!(
        edges[0].extensions["vendor"],
        serde_json::json!({"keep":true})
    );
    assert!(matches!(
        binding,
        Binding::Expression {
            expression: Expr::Binary {
                op: Binary::Multiply,
                ..
            }
        }
    ));
    let geometry = view.nodes.iter().find(|node| node.id == shape).unwrap();
    assert!(
        super::graph::apply(
            &before,
            definition,
            &[GraphChange::Connect(WireView {
                source: Socket {
                    node: shape,
                    key: geometry.outputs[0].key.clone()
                },
                target: wire.target
            })]
        )
        .is_err()
    );
    assert!(super::graph::apply(&before, definition, &[GraphChange::Delete(vec![shape])]).is_err());
    let mut locked = before.clone();
    locked.definitions[0].nodes[0]
        .parameters
        .get_mut("width")
        .unwrap()
        .protected = true;
    let target = view
        .wires
        .iter()
        .find(|wire| wire.target.node == shape)
        .unwrap()
        .target
        .clone();
    assert!(
        super::graph::apply(
            &locked,
            definition,
            &[GraphChange::Connect(WireView {
                source: Socket {
                    node: alternate_id,
                    key: alternate.outputs[0].key.clone()
                },
                target
            })]
        )
        .is_err()
    );
}

#[test]
fn graph_catalog_creates_valid_nodes_and_layout_gestures_commit_once() {
    use sdk::graph_canvas::{GraphChange, GraphContext, GraphEvent};
    let mut host = setup();
    let mut state = super::state::State::default();
    state.sync(&mut host);
    let original = state.construction.clone().unwrap();
    let definition = original.root.definition;
    let id = original.definitions[0].nodes[0].id;
    let revision = host.project.snapshot().revision();
    {
        let mut context = super::graph::Context {
            state: &mut state,
            host: &mut host,
            definition,
        };
        for template in context.catalog() {
            let changed = super::graph::apply(
                &original,
                definition,
                &[GraphChange::Add {
                    template: template.key,
                    position: [300., 100.],
                }],
            )
            .unwrap();
            changed.validate_semantics().unwrap();
            assert!(changed.definitions[0].nodes.len() > original.definitions[0].nodes.len());
        }
        context
            .event(GraphEvent::PreviewPositions(vec![(id, [25., 45.])]))
            .unwrap();
        context
            .event(GraphEvent::PreviewPositions(vec![(id, [50., 90.])]))
            .unwrap();
    }
    assert_eq!(host.project.snapshot().revision(), revision);
    super::graph::Context {
        state: &mut state,
        host: &mut host,
        definition,
    }
    .event(GraphEvent::Cancel)
    .unwrap();
    assert_eq!(state.construction.as_ref(), Some(&original));
    super::graph::Context {
        state: &mut state,
        host: &mut host,
        definition,
    }
    .event(GraphEvent::Commit(vec![GraphChange::Positions(vec![(
        id,
        [50., 90.],
    )])]))
    .unwrap();
    assert_eq!(
        state.construction.as_ref().unwrap().presentation.positions[&id],
        [50., 90.]
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&original));
}

#[test]
fn graph_scene_connections_keep_order_and_missing_sources_are_visible() {
    use sdk::graph_canvas::{GraphChange, Socket, WireView};
    let host = setup();
    let document = host.state.selection.document.unwrap();
    let before = crate::model::Construction::from_document(
        &host.project.snapshot().state().documents[&document],
    )
    .unwrap();
    let definition = before.root.definition;
    let view = super::graph::view(&before, document, definition, 0, vec![]).unwrap();
    let scene = before.definitions[0]
        .nodes
        .iter()
        .find(|n| n.operation == "fold.motion.scene")
        .unwrap();
    let scene_view = view.nodes.iter().find(|n| n.id == scene.id).unwrap();
    let existing = view
        .wires
        .iter()
        .find(|w| w.target.node == scene.id)
        .unwrap();
    let append = scene_view
        .inputs
        .iter()
        .find(|p| p.label.starts_with("Add "))
        .unwrap();
    let after = super::graph::apply(
        &before,
        definition,
        &[GraphChange::Connect(WireView {
            source: existing.source.clone(),
            target: Socket {
                node: scene.id,
                key: append.key.clone(),
            },
        })],
    )
    .unwrap();
    let original = scene.parameters["children"].binding().unwrap();
    let original_edges = crate::connections::edges(&original).unwrap();
    let changed = after
        .definition(definition)
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == scene.id)
        .unwrap()
        .parameters["children"]
        .binding()
        .unwrap();
    let changed_edges = crate::connections::edges(&changed).unwrap();
    assert_eq!(changed_edges.len(), original_edges.len() + 1);
    assert_eq!(changed_edges[0], original_edges[0]);
    assert_ne!(changed_edges[0].id, changed_edges[1].id);
    let removed = super::graph::apply(
        &after,
        definition,
        &[GraphChange::Disconnect(existing.target.clone())],
    )
    .unwrap();
    let remaining = removed
        .definition(definition)
        .unwrap()
        .nodes
        .iter()
        .find(|n| n.id == scene.id)
        .unwrap()
        .parameters["children"]
        .binding()
        .unwrap();
    assert_eq!(
        crate::connections::edges(&remaining).unwrap(),
        vec![changed_edges[1]]
    );
    assert!(
        super::graph::apply(
            &before,
            definition,
            &[GraphChange::Delete(vec![scene.parameters["children"].id])]
        )
        .is_err()
    );
    let mut missing = before.clone();
    missing.definitions[0].outputs[0].value.target = crate::model::Reference::Node {
        node: fold_foundation::ObjectId::new(),
    };
    let projected = super::graph::view(&missing, document, definition, 0, vec![]).unwrap();
    assert!(projected.error.contains("missing sources"));
    assert_eq!(projected.wires.len(), view.wires.len() - 1);
    missing
        .presentation
        .positions
        .insert(scene.id, [f64::MAX, 0.]);
    assert!(super::graph::view(&missing, document, definition, 0, vec![]).is_err());
}

#[test]
fn stale_source_rebase_preserves_disjoint_edits_and_requires_explicit_apply() {
    let mut host = setup();
    let mut state = super::state::State::default();
    state.sync(&mut host);
    let document = state.document.unwrap();
    let original = state.construction.clone().unwrap();
    let mut proposed = original.clone();
    proposed.seed += 1;
    let text = crate::source::canonical(&proposed).unwrap();
    state.drafts.get_mut(&(document, None)).unwrap().text = text;
    state.drafts.get_mut(&(document, None)).unwrap().dirty = true;
    let mut artist = original.clone();
    let shape = artist.definitions[0].nodes[0].id;
    artist.presentation.positions.insert(shape, [120., 240.]);
    assert!(state.commit(&mut host, artist.clone()));
    let revision = host.project.snapshot().revision();
    state.rebase_source(&mut host);
    assert!(state.error.is_empty(), "{}", state.error);
    assert_eq!(host.project.snapshot().revision(), revision);
    assert_eq!(state.construction.as_ref(), Some(&artist));
    let draft = &state.drafts[&(document, None)];
    assert!(draft.dirty);
    assert_eq!(draft.base, revision);
    let merged = crate::source::parse(&draft.text).unwrap().construction;
    assert_eq!(merged.seed, proposed.seed);
    assert_eq!(merged.presentation, artist.presentation);
    assert!(state.commit(&mut host, merged));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&artist));
    state.reload_source(&mut host);
    let mut proposed = artist.clone();
    proposed.seed += 2;
    let text = crate::source::canonical(&proposed).unwrap();
    state.drafts.get_mut(&(document, None)).unwrap().text = text.clone();
    state.drafts.get_mut(&(document, None)).unwrap().dirty = true;
    let base = state.drafts[&(document, None)].base;
    artist.seed += 3;
    assert!(state.commit(&mut host, artist));
    let revision = host.project.snapshot().revision();
    state.rebase_source(&mut host);
    assert!(state.error.contains("conflict"));
    assert_eq!(state.drafts[&(document, None)].text, text);
    assert_eq!(state.drafts[&(document, None)].base, base);
    assert_eq!(host.project.snapshot().revision(), revision);
}

#[test]
fn artist_unlock_is_selective_revision_bound_and_not_an_authoring_command() {
    let mut host = setup();
    let mut state = super::state::State::default();
    state.sync(&mut host);
    let document = state.document.unwrap();
    let mut locked = state.construction.clone().unwrap();
    locked.definitions[0].inputs[0].protected = true;
    locked.definitions[0].nodes[0]
        .parameters
        .get_mut("height")
        .unwrap()
        .protected = true;
    let input = locked.definitions[0].inputs[0].id;
    assert!(state.commit(&mut host, locked.clone()));
    let locked_revision = state.base;
    let mut unlocked = locked.clone();
    unlocked.definitions[0].inputs[0].protected = false;
    assert!(!state.commit(&mut host, unlocked.clone()));
    assert!(host.registry.commands().all(|c| !c.id.contains("unlock")));
    super::protection::unlock(&mut host, &mut state, input);
    assert!(state.error.is_empty(), "{}", state.error);
    assert_eq!(state.construction.as_ref(), Some(&unlocked));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&locked));
    let revision = state.base;
    host.command(DesktopCommand::UnlockControls {
        document,
        object: None,
        targets: vec![input],
        base: locked_revision,
    });
    assert_eq!(host.project.snapshot().revision(), revision);
    assert!(host.state.status.contains("stale"));
    host.command(DesktopCommand::UnlockControls {
        document,
        object: None,
        targets: vec![input, fold_foundation::ObjectId::new()],
        base: revision,
    });
    assert_eq!(host.project.snapshot().revision(), revision);
}

#[test]
fn presentation_groups_notes_locks_and_undo_use_native_transactions() {
    let mut host = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let original = state.construction.clone().unwrap();
    let definition = original.root.definition;
    let node = original.definition(definition).unwrap().nodes[0].id;
    let mut grouped = presentation::group(&original, definition, &[node]).unwrap();
    let group = grouped.presentation.groups[0].id;
    grouped.presentation.groups[0].name = "Layout".into();
    grouped
        .presentation
        .annotations
        .insert(node, "Keep aligned with the title".into());
    grouped.presentation.groups[0]
        .extensions
        .insert("future".into(), serde_json::json!({"color":"blue"}));
    assert!(state.commit(&mut host, grouped.clone()));
    let document = state.document.unwrap();
    let graph = graph::view(&grouped, document, definition, state.base.0, vec![node]).unwrap();
    assert!(
        graph
            .nodes
            .iter()
            .find(|n| n.id == node)
            .unwrap()
            .summary
            .contains("Layout")
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&original));
    host.project.redo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&grouped));
    let mut draft = grouped.clone();
    draft
        .presentation
        .annotations
        .insert(node, "Cancelled note".into());
    state.construction = Some(draft);
    state.preview(&mut host);
    state.cancel(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&grouped));

    let parent = fold_foundation::ObjectId::new();
    grouped.presentation.groups.push(crate::model::Group {
        id: parent,
        name: "Parent".into(),
        members: vec![group],
        locked: false,
        extensions: Default::default(),
    });
    let ungrouped = presentation::ungroup(&grouped, group).unwrap();
    assert_eq!(ungrouped.presentation.groups[0].members, vec![node]);
    assert_eq!(ungrouped.definitions, original.definitions);
    assert_eq!(
        ungrouped.presentation.annotations.get(&node),
        grouped.presentation.annotations.get(&node)
    );
    grouped.presentation.groups[0].locked = true;
    assert!(presentation::ungroup(&grouped, group).is_err());
    let mut forbidden = grouped.clone();
    forbidden
        .presentation
        .annotations
        .insert(node, "Locked edit".into());
    assert!(
        crate::editing::validate_document(
            &grouped.document(document).unwrap(),
            crate::editing::Scope::Document,
            forbidden
        )
        .is_err()
    );
}

#[test]
fn presentation_inspector_draws_expanded_at_normal_and_narrow_widths() {
    let host = setup();
    let document = host.state.selection.document.unwrap();
    let snapshot = host.project.snapshot();
    let original =
        crate::model::Construction::from_document(&snapshot.state().documents[&document]).unwrap();
    let definition = original.root.definition;
    let node = original.definition(definition).unwrap().nodes[0].id;
    let mut value = presentation::group(&original, definition, &[node]).unwrap();
    value
        .presentation
        .annotations
        .insert(node, "Artist note".into());
    let before = value.clone();
    let _guard = UI_TEST_LOCK.lock().unwrap();
    let mut context = sdk::imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([800., 700.]);
    context.io_mut().set_delta_time(1. / 60.);
    for width in [360., 240., 360., 240.] {
        let ui = context.frame();
        ui.window("Inspector")
            .position([0., 0.], sdk::imgui::Condition::Always)
            .size([width, 680.], sdk::imgui::Condition::Always)
            .build(|| {
                let mut response = EditResponse::default();
                assert!(
                    presentation::draw(ui, &mut value, definition, &[node], &mut response)
                        .is_none()
                );
                assert_eq!(response, EditResponse::default());
            });
        assert!(context.render_legacy().total_vtx_count() > 0);
    }
    assert_eq!(value, before);
}

#[test]
fn primitive_controls_preserve_exact_values_while_drawing() {
    use crate::types::{Alpha, Encoding};
    let _guard = UI_TEST_LOCK.lock().unwrap();
    let mut context = sdk::imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([600., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let original = vec![
        Literal::Rotation {
            value: [0., 0., 0., -1.],
            space: crate::types::Space::Screen,
        },
        Literal::Rotation {
            value: [0.5, 0.5, 0.5, 0.5],
            space: crate::types::Space::World,
        },
        Literal::AssetRef {
            value: crate::types::AssetReference::Definition(fold_foundation::ObjectId::new()),
        },
        Literal::AssetRef {
            value: crate::types::AssetReference::Resource(fold_foundation::AssetId::new()),
        },
        Literal::Integer { value: i64::MIN },
        Literal::Integer { value: i64::MAX },
        Literal::Text {
            value: "Title\nSecond line".into(),
        },
        Literal::Duration {
            value: fold_foundation::Time::new(-1001, 24000).unwrap(),
        },
        Literal::Color {
            value: [-0.2, 12., 0.1234567890123, 0.8],
            encoding: Encoding::AcesCg,
            alpha: Alpha::Straight,
        },
        Literal::Color {
            value: [0., 0., 0., 0.],
            encoding: Encoding::LinearRec709,
            alpha: Alpha::Premultiplied,
        },
    ];
    let mut values = original.clone();
    let mut literal_controls = literal::Controls::default();
    for width in [400., 240.] {
        let ui = context.frame();
        ui.window("Controls")
            .size([width, 780.], sdk::imgui::Condition::Always)
            .build(|| {
                for (index, value) in values.iter_mut().enumerate() {
                    let _id = ui.push_id(&index.to_string());
                    let mut response = EditResponse::default();
                    literal(ui, "Value", value, &mut response, &mut literal_controls);
                    assert_eq!(response, EditResponse::default());
                    value.validate().unwrap();
                }
            });
        assert!(context.render_legacy().total_vtx_count() > 0);
    }
    assert_eq!(values, original);
}

#[test]
fn instance_menu_input_creates_resets_and_undoes_one_override() {
    use sdk::imgui::{Condition, ConfigFlags, Context, Key, MouseButton};
    fn frame(
        context: &mut Context,
        host: &mut Host,
        state: &mut state::State,
    ) -> ([f32; 2], [f32; 2]) {
        state.sync(host);
        state.literal_controls.invalid = false;
        let mut value = state.construction.clone().unwrap();
        let inputs = value.definitions[0].inputs.clone();
        let locked = crate::protection::locked_ids(&value).unwrap();
        let mut response = EditResponse::default();
        let mut button = [0.; 2];
        let mut point = [0.; 2];
        let ui = context.frame();
        ui.window("Instance")
            .position([0.; 2], Condition::Always)
            .size([260., 350.], Condition::Always)
            .build(|| {
                let origin = ui.cursor_screen_pos();
                let label = if locked.contains(&inputs[0].id) {
                    "Locked"
                } else if value.root.overrides.contains_key(&inputs[0].id) {
                    "Instance override"
                } else {
                    "Asset default"
                };
                button = [
                    origin[0]
                        + ui.calc_text_size(label)[0]
                        + ui.clone_style().item_spacing()[0]
                        + (ui.frame_height() + 4.) / 2.,
                    origin[1] + ui.text_line_height_with_spacing() + ui.frame_height() / 2.,
                ];
                point = [origin[0] + 55., button[1] + ui.frame_height_with_spacing()];
                let _width = ui.push_item_width(110.);
                instance::draw(
                    ui,
                    &mut value.root,
                    &inputs,
                    &locked,
                    None,
                    &mut response,
                    &mut state.literal_controls,
                );
            });
        drop(context.render_legacy());
        if response.changed {
            state.construction = Some(value.clone());
            state.preview(host);
        }
        if response.finished && state.editing {
            assert!(state.commit(host, value));
        }
        (button, point)
    }
    fn choose(context: &mut Context, host: &mut Host, state: &mut state::State, animate: bool) {
        let (button, _) = frame(context, host, state);
        context.io_mut().add_mouse_pos_event(button);
        frame(context, host, state);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, true);
        frame(context, host, state);
        context
            .io_mut()
            .add_mouse_button_event(MouseButton::Left, false);
        frame(context, host, state);
        frame(context, host, state);
        context.io_mut().add_key_event(Key::DownArrow, true);
        frame(context, host, state);
        context.io_mut().add_key_event(Key::DownArrow, false);
        frame(context, host, state);
        context.io_mut().add_key_event(Key::Home, true);
        frame(context, host, state);
        context.io_mut().add_key_event(Key::Home, false);
        frame(context, host, state);
        if animate {
            context.io_mut().add_key_event(Key::DownArrow, true);
            frame(context, host, state);
            context.io_mut().add_key_event(Key::DownArrow, false);
            frame(context, host, state);
        }
        context.io_mut().add_key_event(Key::Enter, true);
        frame(context, host, state);
        context.io_mut().add_key_event(Key::Enter, false);
        frame(context, host, state);
    }
    let _guard = UI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .io_mut()
        .set_config_flags(ConfigFlags::NAV_ENABLE_KEYBOARD);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([400., 400.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut host = setup();
    let mut state = state::State::default();
    for _ in 0..3 {
        frame(&mut context, &mut host, &mut state);
    }
    let before = state.construction.clone().unwrap();
    let revision = host.project.snapshot().revision();
    let input = before.definitions[0].inputs[0].id;
    choose(&mut context, &mut host, &mut state, false);
    let mut overridden = state.construction.clone().unwrap();
    let parameter = overridden
        .root
        .overrides
        .get(&input)
        .expect("menu creates override");
    assert_eq!(
        parameter.binding().unwrap(),
        Binding::Literal {
            value: before.definitions[0].inputs[0].default.clone()
        }
    );
    assert_eq!(host.project.snapshot().revision().0, revision.0 + 1);
    assert_eq!(overridden.definitions, before.definitions);
    let parameter_id = parameter.id;
    // A value drag previews continuously, retains the parameter ID, and commits
    // exactly once on release before the contextual reset is exercised.
    let (_, point) = frame(&mut context, &mut host, &mut state);
    context.io_mut().add_mouse_pos_event(point);
    frame(&mut context, &mut host, &mut state);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    frame(&mut context, &mut host, &mut state);
    context
        .io_mut()
        .add_mouse_pos_event([point[0] + 40., point[1]]);
    frame(&mut context, &mut host, &mut state);
    assert!(state.editing);
    assert_eq!(host.project.snapshot().revision().0, revision.0 + 1);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    frame(&mut context, &mut host, &mut state);
    let edited = state.construction.clone().unwrap();
    assert_eq!(edited.root.overrides[&input].id, parameter_id);
    assert_ne!(
        edited.root.overrides[&input].value,
        overridden.root.overrides[&input].value
    );
    assert_eq!(host.project.snapshot().revision().0, revision.0 + 2);
    assert!(!state.editing);
    overridden = edited;
    choose(&mut context, &mut host, &mut state, false);
    assert_eq!(state.construction.as_ref(), Some(&before));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&overridden));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(
        state.construction.as_ref().unwrap().root.overrides[&input].id,
        parameter_id
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    // The second menu action creates a scene-time track directly from inheritance.
    choose(&mut context, &mut host, &mut state, true);
    let animated = state.construction.clone().unwrap();
    let Binding::Track { track } = animated.root.overrides[&input].binding().unwrap() else {
        panic!("animated override")
    };
    assert_eq!(track.keys[0].time, fold_foundation::Time::ZERO);
    assert_eq!(track.keys[0].value, before.definitions[0].inputs[0].default);
    assert_eq!(animated.definitions, before.definitions);
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    choose(&mut context, &mut host, &mut state, false);
    let literal = state.construction.clone().unwrap();
    choose(&mut context, &mut host, &mut state, true);
    let animated = state.construction.clone().unwrap();
    assert_eq!(
        animated.root.overrides[&input].id,
        literal.root.overrides[&input].id
    );
    assert!(matches!(
        animated.root.overrides[&input].binding().unwrap(),
        Binding::Track { .. }
    ));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&literal));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    let mut protected = before;
    protected.definitions[0].inputs[0].protected = true;
    assert!(state.commit(&mut host, protected.clone()));
    let revision = host.project.snapshot().revision();
    choose(&mut context, &mut host, &mut state, false);
    assert_eq!(host.project.snapshot().revision(), revision);
    assert_eq!(state.construction.as_ref(), Some(&protected));
}

#[test]
fn instance_clock_gestures_preserve_exact_time_and_respect_group_locks() {
    use crate::types::{Clock, Ratio};
    use fold_foundation::{ObjectId, Time};
    let mut host = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let before = state.construction.clone().unwrap();
    let mut changed = before.clone();
    changed.root.clock = Clock {
        offset: Time::new(1001, 24000).unwrap(),
        rate: Ratio::new(-1001, 1000).unwrap(),
    };
    assert_eq!(
        changed.root.clock.at(Time::new(3, 4).unwrap()).unwrap(),
        Time::new(-17015999, 24000000).unwrap()
    );
    let revision = host.project.snapshot().revision();
    state.construction = Some(changed.clone());
    state.preview(&mut host);
    assert_eq!(host.project.snapshot().revision(), revision);
    state.cancel(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    assert!(state.commit(&mut host, changed.clone()));
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    host.project.redo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&changed));
    changed.presentation.groups.push(crate::model::Group {
        id: ObjectId::new(),
        name: "Protected instance".into(),
        members: vec![changed.root.id],
        locked: true,
        extensions: Default::default(),
    });
    assert!(state.commit(&mut host, changed.clone()));
    let mut frozen = changed.clone();
    frozen.root.clock.rate = Ratio::new(0, 1).unwrap();
    assert!(!state.commit(&mut host, frozen));
    assert_eq!(state.construction.as_ref(), Some(&changed));
}

#[test]
fn override_key_edits_preview_cancel_undo_and_respect_input_locks() {
    use crate::binding::{Combination, Interpolation, Key, Track};
    use fold_foundation::{ObjectId, Time};
    let mut host = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let mut before = state.construction.clone().unwrap();
    let input = before.definitions[0].inputs[0].clone();
    let mut parameter = crate::model::Parameter::new(Binding::Track {
        track: Track {
            id: ObjectId::new(),
            ty: input.ty.clone(),
            combination: Combination::Replace,
            base: None,
            keys: vec![Key {
                id: ObjectId::new(),
                time: Time::ZERO,
                value: input.default.clone(),
                interpolation: Interpolation::Linear,
                handles: Default::default(),
                extensions: Default::default(),
            }],
            extensions: Default::default(),
        },
    })
    .unwrap();
    parameter
        .extensions
        .insert("vendor".into(), serde_json::json!({"retain":true}));
    before.root.overrides.insert(input.id, parameter);
    before.validate_semantics().unwrap();
    assert!(state.commit(&mut host, before.clone()), "{}", state.error);
    let mut changed = before.clone();
    let parameter = changed.root.overrides.get_mut(&input.id).unwrap();
    let Binding::Track { mut track } = parameter.binding().unwrap() else {
        panic!("track")
    };
    track.keys[0].time = Time::new(-1, 3).unwrap();
    track.keys[0].interpolation = Interpolation::Hold;
    parameter.value = serde_json::to_value(Binding::Track { track }).unwrap();
    let revision = host.project.snapshot().revision();
    state.construction = Some(changed.clone());
    state.preview(&mut host);
    assert_eq!(host.project.snapshot().revision(), revision);
    state.cancel(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    assert!(state.commit(&mut host, changed.clone()));
    assert_eq!(host.project.snapshot().revision().0, revision.0 + 1);
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    host.project.redo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&changed));
    changed.definitions[0].inputs[0].protected = true;
    assert!(state.commit(&mut host, changed.clone()));
    let mut reset = changed.clone();
    reset.root.overrides.remove(&input.id);
    assert!(!state.commit(&mut host, reset));
    assert_eq!(state.construction.as_ref(), Some(&changed));
}

#[test]
fn ramp_controls_draw_at_normal_and_narrow_widths_without_mutation() {
    let _guard = UI_TEST_LOCK.lock().unwrap();
    let mut context = sdk::imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([600., 800.]);
    context.io_mut().set_delta_time(1. / 60.);
    let mut value = crate::ramp::Ramp::two(
        Literal::Scalar { value: 0.2 },
        Literal::Scalar { value: 1. },
    )
    .unwrap();
    value.insert(0.375).unwrap();
    let before = value.clone();
    let mut controls = literal::Controls::default();
    for width in [400., 240.] {
        let ui = context.frame();
        ui.window("Ramp")
            .size([width, 780.], sdk::imgui::Condition::Always)
            .build(|| {
                let mut response = EditResponse::default();
                ramp::draw(ui, "Stops", &mut value, &mut response, &mut controls);
                assert_eq!(response, EditResponse::default());
            });
        assert!(context.render_legacy().total_vtx_count() > 0);
    }
    assert_eq!(value, before);
    assert!(!controls.invalid);
}

#[test]
fn auto_key_parameter_gesture_commits_once_and_cancel_discards_pending_keys() {
    let mut host = scene_setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let before = state.construction.clone().unwrap();
    let parameter = before.definitions[0].nodes[0].parameters["size"].id;
    let time = fold_foundation::Time::new(1, 2).unwrap();
    for size in [2., 3., 4.] {
        let graph = state.construction.as_mut().unwrap();
        graph.definitions[0].nodes[0]
            .parameters
            .get_mut("size")
            .unwrap()
            .value = serde_json::to_value(Binding::Literal {
            value: Literal::Vector {
                value: vec![size; 3],
                unit: crate::types::Unit::Meters,
                space: crate::types::Space::World,
            },
        })
        .unwrap();
        state.auto_keys.insert(parameter, time);
        state.preview(&mut host);
    }
    let draft = state.construction.clone().unwrap();
    assert!(state.finish_parameters(&mut host, draft));
    let channels = crate::keyed::channels(state.construction.as_ref().unwrap(), None).unwrap();
    assert_eq!(channels.len(), 3);
    assert!(
        channels
            .iter()
            .all(|c| c.curve.keys[0].time == time && c.curve.keys[0].value == 4.)
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
    state.auto_keys.insert(parameter, time);
    state.preview(&mut host);
    state.cancel(&mut host);
    assert!(state.auto_keys.is_empty());
    assert_eq!(state.construction.as_ref(), Some(&before));
}

#[test]
fn nested_occurrence_inspector_auto_keys_use_reverse_and_paused_clocks() {
    use crate::model::{Construction, Instance};
    use crate::types::{Clock, Ratio};
    use fold_foundation::{ObjectId, Time};
    let mut host = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let mut graph = state.construction.clone().unwrap();
    let root = graph.root.definition;
    let make_definition = || {
        Construction::from_document(
            &host
                .registry
                .create(crate::CONSTRUCTION, DocumentId::new())
                .unwrap(),
        )
        .unwrap()
        .definitions
        .remove(0)
    };
    let mut child = make_definition();
    child.name = "Reusable branch".into();
    let mut leaf = make_definition();
    leaf.name = "Nested shape".into();
    let parameter = leaf.nodes[0].parameters["height"].id;
    let nested = Instance {
        id: ObjectId::new(),
        name: "Nested".into(),
        definition: leaf.id,
        clock: Clock {
            offset: Time::new(-2, 1).unwrap(),
            rate: Ratio::new(1, 2).unwrap(),
        },
        overrides: Default::default(),
        extensions: Default::default(),
    };
    child.instances.push(nested.clone());
    let reverse = Instance {
        id: ObjectId::new(),
        name: "Reverse".into(),
        definition: child.id,
        clock: Clock {
            offset: Time::new(1, 4).unwrap(),
            rate: Ratio::new(-2, 1).unwrap(),
        },
        overrides: Default::default(),
        extensions: Default::default(),
    };
    let mut paused = reverse.clone();
    paused.id = ObjectId::new();
    paused.name = "Paused".into();
    paused.clock.rate = Ratio::new(0, 1).unwrap();
    graph.definitions[0]
        .instances
        .extend([reverse.clone(), paused.clone()]);
    graph.definitions.extend([child.clone(), leaf.clone()]);
    graph.root.clock = Clock {
        offset: Time::new(1, 4).unwrap(),
        rate: Ratio::new(2, 1).unwrap(),
    };
    assert!(state.commit(&mut host, graph.clone()), "{}", state.error);
    host.state.navigation.push(ViewLocation {
        document: state.document.unwrap(),
        time: Time::new(3, 4).unwrap(),
        label: "Occurrence".into(),
    });
    state.graph_definition = Some(leaf.id);
    state.graph_history = vec![root, child.id];
    for (instance, expected) in [
        (reverse.id, Time::new(1, 4).unwrap()),
        (paused.id, Time::new(1, 1).unwrap()),
    ] {
        state.graph_occurrences = vec![instance, nested.id];
        state.sync(&mut host);
        assert_eq!(state.key_time, Some((leaf.id, expected)));
        let before = state.construction.clone().unwrap();
        state.auto_keys.insert(parameter, state.key_time.unwrap().1);
        state.preview(&mut host);
        assert!(
            state.finish_parameters(&mut host, before.clone()),
            "{}",
            state.error
        );
        assert_eq!(
            state.graph_definition,
            Some(leaf.id),
            "Committing a gesture lost the selected occurrence"
        );
        let track_in = |graph: &Construction| {
            let definition = graph.definition(leaf.id).unwrap();
            let Binding::Track { track } =
                definition.nodes[0].parameters["height"].binding().unwrap()
            else {
                panic!("Auto-key must retain a track on the authored parameter")
            };
            track
        };
        let track = track_in(state.construction.as_ref().unwrap());
        assert_eq!(track.keys[0].time, expected);
        host.project.undo().unwrap();
        state.sync(&mut host);
        assert_eq!(state.construction.as_ref(), Some(&before));
        host.project.redo().unwrap();
        state.sync(&mut host);
        let restored = track_in(state.construction.as_ref().unwrap());
        assert_eq!(restored, track);
        host.project.undo().unwrap();
        state.sync(&mut host);
    }
    state.graph_history.clear();
    state.graph_occurrences.clear();
    state.sync(&mut host);
    assert_eq!(
        state.key_time, None,
        "A definition without an occurrence must not borrow root time"
    );
}

#[test]
fn channel_reference_ports_follow_identity_through_reorder_and_rename() {
    let host = setup();
    let document = host.state.selection.document.unwrap();
    let mut graph = crate::model::Construction::from_document(
        &host.project.snapshot().state().documents[&document],
    )
    .unwrap();
    let definition = graph.root.definition;
    let before = super::graph::view(&graph, document, definition, 0, vec![]).unwrap();
    let channels = before.nodes.iter().find(|n| n.id == definition).unwrap();
    assert_eq!(channels.label, "Channel References");
    assert_eq!(channels.outputs.len(), graph.definitions[0].inputs.len());
    assert!(
        !before
            .nodes
            .iter()
            .any(|n| graph.definitions[0].inputs.iter().any(|i| i.id == n.id))
    );
    let port = channels.outputs[0].key.clone();
    let mut input = graph.definitions[0].inputs[0].clone();
    input.id = fold_foundation::ObjectId::new();
    input.name = "Other width".into();
    graph.definitions[0].inputs.insert(0, input);
    graph.definitions[0].inputs[1].name = "Renamed width".into();
    let after = super::graph::view(&graph, document, definition, 1, vec![]).unwrap();
    let channels = after.nodes.iter().find(|n| n.id == definition).unwrap();
    assert_eq!(channels.outputs[1].key, port);
    assert_eq!(channels.outputs[1].label, "Renamed width");
    assert!(
        after
            .wires
            .iter()
            .any(|wire| wire.source.node == definition && wire.source.key == port)
    );
    let moved = super::graph::apply(
        &graph,
        definition,
        &[sdk::graph_canvas::GraphChange::Positions(vec![(
            definition,
            [12., 34.],
        )])],
    )
    .unwrap();
    assert_eq!(moved.presentation.positions[&definition], [12., 34.]);
    assert_eq!(moved.definitions, graph.definitions);
}

#[test]
fn adding_a_container_parameter_uses_one_native_transaction() {
    use sdk::graph_canvas::GraphContext;
    let mut host = setup();
    let mut state = state::State::default();
    state.sync(&mut host);
    let before = state.construction.clone().unwrap();
    let definition = before.root.definition;
    super::graph::Context {
        state: &mut state,
        host: &mut host,
        definition,
    }
    .node_action(definition, "add-parameter")
    .unwrap();
    let after = state.construction.as_ref().unwrap();
    assert_eq!(
        after.definition(definition).unwrap().inputs.len(),
        before.definition(definition).unwrap().inputs.len() + 1
    );
    assert_eq!(
        after.definition(definition).unwrap().nodes,
        before.definition(definition).unwrap().nodes
    );
    host.project.undo().unwrap();
    state.sync(&mut host);
    assert_eq!(state.construction.as_ref(), Some(&before));
}

pub(super) fn image_setup() -> Host {
    let mut host = setup();
    let snapshot = host.project.snapshot();
    let document = host.state.selection.document.unwrap();
    let mut graph =
        crate::model::Construction::from_document(&snapshot.state().documents[&document]).unwrap();
    let id = fold_foundation::AssetId::new();
    let asset = fold_project::Asset {
        id,
        location: "/tmp/Color study.ppm".into(),
        fingerprint: "ui-metadata-fixture".into(),
        extensions: [(
            fold_media::ingest::METADATA_KEY.into(),
            serde_json::to_value(fold_media::ingest::SourceMetadata {
                profile: fold_media::ingest::SourceProfile::Ppm {
                    dimensions: [64, 32],
                },
                streams: vec![],
                raw_format: serde_json::json!({}),
                timing_policy: "still".into(),
            })
            .unwrap(),
        )]
        .into(),
    };
    let texture = crate::registry::resolve("fold.motion.image_texture", 1, "asset")
        .unwrap()
        .node(
            "Image Texture",
            super::images::bindings(&fold_platform::image_source::Source::Asset(id)),
        )
        .unwrap();
    let sampled = crate::registry::resolve("fold.motion.sample_image", 1, "scalar")
        .unwrap()
        .node(
            "Sample Image",
            [(
                "image".into(),
                Binding::Link {
                    reference: crate::model::Reference::Node { node: texture.id }.into(),
                },
            )]
            .into(),
        )
        .unwrap();
    host.state.selection.objects = vec![texture.id, sampled.id];
    graph.definitions[0].nodes.extend([texture, sampled]);
    host.project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![
                Mutation::PutAsset(asset),
                Mutation::PutDocument(graph.document(document).unwrap()),
            ],
        })
        .unwrap();
    host
}
#[test]
fn image_graph_templates_preserve_existing_identity() {
    use sdk::graph_canvas::{GraphChange, GraphContext};
    let mut host = image_setup();
    let mut state = super::state::State::default();
    state.sync(&mut host);
    let original = state.construction.clone().unwrap();
    let definition = original.root.definition;
    let context = super::graph::Context {
        state: &mut state,
        host: &mut host,
        definition,
    };
    let templates: Vec<_> = context
        .catalog()
        .into_iter()
        .filter(|t| t.category == "Image Texture" || t.label.starts_with("Sample Image"))
        .collect();
    assert_eq!(templates.len(), 3);
    for template in templates {
        let after = super::graph::apply(
            &original,
            definition,
            &[GraphChange::Add {
                template: template.key,
                position: [0., 0.],
            }],
        )
        .unwrap();
        after.validate_semantics().unwrap();
        assert_eq!(
            &after.definitions[0].nodes[..original.definitions[0].nodes.len()],
            original.definitions[0].nodes.as_slice()
        );
    }
}
