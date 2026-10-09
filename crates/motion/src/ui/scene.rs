//! Scene UI shares the existing graph editor and Inspector implementation.
use super::*;
use crate::scene::{Scene, package::EditArgs};
use fold_platform::{desktop::DesktopClient, packages::CommandRequest};
use fold_project::Revision;
pub(super) const PANEL: &str = "fold.motion.native.scene.network";
pub(super) const INSPECTOR: &str = "fold.motion.native.scene.inspector";
pub(super) struct Editor(pub super::Editor);
pub(super) struct Inspector {
    pub graph: super::Inspector,
    draft: Option<SceneDraft>,
    controls: literal::Controls,
}
#[derive(Clone)]
struct SceneDraft {
    document: fold_foundation::DocumentId,
    object: Option<fold_foundation::ObjectId>,
    base: Revision,
    scene: Scene,
}
impl Inspector {
    pub fn new(shared: Shared) -> Self {
        Self {
            graph: super::Inspector(shared),
            draft: None,
            controls: Default::default(),
        }
    }
}
impl Panel for Editor {
    fn selection_owner(
        &mut self,
        host: &mut dyn DesktopClient,
    ) -> Option<fold_foundation::ObjectId> {
        self.0.0.borrow_mut().sync(host);
        self.0.0.borrow().object
    }
    fn id(&self) -> &'static str {
        PANEL
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::scene::TYPE)
    }
    fn supports_network(&self) -> bool {
        true
    }
    fn is_layer_graph(&self) -> bool {
        true
    }
    fn new_instance(&self) -> Option<sdk::EditorPanels> {
        let state = Shared::default();
        Some(sdk::EditorPanels {
            editor: Box::new(Editor(super::Editor(state.clone(), Default::default()))),
            inspector: Box::new(Inspector::new(state)),
        })
    }
    fn initialize(
        &mut self,
        context: &sdk::imgui::Context,
        typography: Option<&sdk::typography::Typography>,
    ) {
        self.0.initialize(context, typography);
    }
    fn accepts_background_pan(&self, p: [f32; 2]) -> bool {
        self.0.accepts_background_pan(p)
    }
    fn cancel_interaction(&mut self, host: &mut dyn DesktopClient) {
        self.0.cancel_interaction(host);
    }
    fn draw(&mut self, c: ExtensionUi<'_>) {
        self.draw_network(c);
    }
    fn draw_network(&mut self, c: ExtensionUi<'_>) {
        self.0.0.borrow_mut().sync(c.host);
        if self.0.0.borrow().object.is_none() {
            c.ui.text_disabled("Select an object in Outliner to edit its graph.");
            return;
        }
        self.0.draw_network(c);
    }
}
impl Panel for Inspector {
    fn id(&self) -> &'static str {
        INSPECTOR
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::scene::TYPE)
    }
    fn draw(&mut self, c: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = c;
        self.controls.invalid = false;
        self.graph.0.borrow_mut().sync(host);
        let (Some(document), scope) = ({
            let state = self.graph.0.borrow();
            (state.document, state.object)
        }) else {
            return;
        };
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        let Some(original) = snapshot.state().documents.get(&document) else {
            return;
        };
        let Ok(current) = Scene::from_document(original) else {
            return;
        };
        if self.draft.as_ref().is_some_and(|draft| {
            draft.document != document || draft.object != scope || draft.base != snapshot.revision()
        }) {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.draft = None;
        }
        let (base, mut scene) = self
            .draft
            .as_ref()
            .map(|draft| (draft.base, draft.scene.clone()))
            .unwrap_or((snapshot.revision(), current));
        let local_time = self.graph.0.borrow().object_time(host);
        let mut response = EditResponse::default();
        if let Some(object) = scope {
            let locked = scene.locked(object);
            if let Some(target) = scene.objects.iter_mut().find(|o| o.id == object) {
                ui.text(&target.name);
                let _locked = ui.begin_disabled_with_cond(locked);
                let utility = target
                    .graph
                    .definitions
                    .iter()
                    .flat_map(|definition| &definition.outputs)
                    .any(|output| {
                        output.id == target.output
                            && matches!(
                                output.ty,
                                crate::types::Type::Camera | crate::types::Type::Light
                            )
                    });
                let flags = if utility {
                    sdk::imgui::TreeNodeFlags::empty()
                } else {
                    sdk::imgui::TreeNodeFlags::DEFAULT_OPEN
                };
                if ui.collapsing_header("Transform", flags) {
                    for (name, binding) in [
                        ("Position", &mut target.position),
                        ("Rotation", &mut target.rotation),
                        ("Scale", &mut target.scale),
                    ] {
                        let _id = ui.push_id(name);
                        match binding {
                            Binding::Literal { value } => {
                                let before = value.clone();
                                literal(ui, name, value, &mut response, &mut self.controls);
                                if *value != before
                                    && host.state().auto_key
                                    && !self.controls.invalid
                                    && let Ok(time) = &local_time
                                {
                                    match crate::binding::Track::constant(value.clone(), *time) {
                                        Ok(track) => *binding = Binding::Track { track },
                                        Err(error) => ui.text_wrapped(error),
                                    }
                                }
                            }
                            Binding::Track { track } => keyframes::draw(
                                ui,
                                name,
                                track,
                                local_time.clone().ok(),
                                &mut response,
                                &mut self.controls,
                            ),
                            _ => ui.text_disabled(format!("{name} · Unresolved")),
                        }
                        if let Some(_menu) =
                            ui.begin_popup_context_item_with_label(Some("animation"))
                        {
                            if ui.menu_item("Add keyframe") {
                                let result = local_time.clone().and_then(|time| match binding {
                                    Binding::Literal { value } => {
                                        crate::binding::Track::constant(value.clone(), time)
                                    }
                                    Binding::Track { track } => {
                                        let mut result = track.clone();
                                        if !result.keys.iter().any(|k| k.time == time) {
                                            let mut contribution = track.clone();
                                            contribution.combination =
                                                crate::binding::Combination::Replace;
                                            contribution.base = None;
                                            result.keys.push(crate::binding::Key {
                                                id: fold_foundation::ObjectId::new(),
                                                time,
                                                value: contribution.sample(time)?,
                                                interpolation:
                                                    crate::binding::Interpolation::Linear,
                                                handles: Default::default(),
                                                extensions: Default::default(),
                                            });
                                            result.keys.sort_by_key(|k| k.time);
                                        }
                                        Ok(result)
                                    }
                                    _ => Err("This binding cannot be keyed".into()),
                                });
                                match result {
                                    Ok(track) => {
                                        *binding = Binding::Track { track };
                                        response.changed = true;
                                        response.finished = true;
                                    }
                                    Err(error) => ui.text_wrapped(error),
                                }
                            }
                            if matches!(binding, Binding::Track { .. })
                                && ui.menu_item("Remove animation")
                            {
                                if let Binding::Track { track } = binding {
                                    match local_time.clone().and_then(|time| track.sample(time)) {
                                        Ok(value) => {
                                            *binding = Binding::Literal { value };
                                            response.changed = true;
                                            response.finished = true;
                                        }
                                        Err(error) => ui.text_wrapped(error),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        } else {
            ui.text("Scene");
            let label = scene
                .object(scene.active_camera)
                .map(|o| o.name.clone())
                .unwrap_or_default();
            if let Some(_menu) = ui.begin_combo("Camera", &label) {
                for object in &scene.objects {
                    if object
                        .graph
                        .definitions
                        .iter()
                        .flat_map(|d| &d.outputs)
                        .any(|o| o.id == object.output && o.ty == crate::types::Type::Camera)
                        && ui.selectable(&object.name)
                    {
                        scene.active_camera = object.id;
                        response.changed = true;
                        response.finished = true;
                    }
                }
            }
        }
        let graph_locked = scope.is_some_and(|object| scene.locked(object));
        if response.cancelled {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.draft = None;
        } else if response.changed || response.finished {
            let result = scene.replace(original).and_then(|_| {
                serde_json::to_vec(&EditArgs {
                    document,
                    scene: scene.clone(),
                })
                .map_err(|e| e.to_string())
            });
            match result {
                Ok(arguments) => {
                    let request = CommandRequest {
                        id: crate::scene::EDIT.into(),
                        base,
                        arguments,
                    };
                    if response.finished {
                        host.command(DesktopCommand::Extension(request));
                        self.draft = None;
                    } else {
                        host.command(DesktopCommand::PreviewExtension(request));
                        self.draft = Some(SceneDraft {
                            document,
                            object: scope,
                            base,
                            scene,
                        });
                    }
                }
                Err(error) => {
                    host.command(DesktopCommand::CancelPreviewEdit);
                    self.draft = None;
                    ui.text_wrapped(error);
                }
            }
        }
        if scope.is_some() {
            ui.separator();
            let _locked = ui.begin_disabled_with_cond(graph_locked);
            self.graph.draw(ExtensionUi { ui, host });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn switching_scene_object_cancels_the_previous_objects_transform_draft() {
        let _guard = super::super::UI_TEST_LOCK.lock().unwrap();
        let mut host = super::super::tests::scene_setup();
        let snapshot = host.snapshot().unwrap();
        let document = host.state().selection.document.unwrap();
        let first = host.state().selection.objects[0];
        let scene = Scene::from_document(&snapshot.state().documents[&document]).unwrap();
        let other = scene
            .objects
            .iter()
            .find(|object| object.id != first)
            .unwrap()
            .id;
        let mut inspector = Inspector::new(Shared::default());
        inspector.draft = Some(SceneDraft {
            document,
            object: Some(first),
            base: snapshot.revision(),
            scene,
        });
        host.command(DesktopCommand::Select(fold_platform::desktop::Selection {
            document: Some(document),
            objects: vec![other],
        }));
        let mut context = sdk::imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([800., 800.]);
        context.io_mut().set_delta_time(1. / 60.);
        let ui = context.frame();
        ui.window("Inspector")
            .size([600., 700.], sdk::imgui::Condition::Always)
            .build(|| {
                inspector.draw(ExtensionUi {
                    ui,
                    host: &mut host,
                })
            });
        let _ = context.render_legacy();
        assert!(inspector.draft.is_none());
        assert_eq!(inspector.graph.0.borrow().object, Some(other));
        assert_eq!(host.snapshot().unwrap().revision(), snapshot.revision());
    }
}
