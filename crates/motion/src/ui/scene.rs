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
    draft: Option<(Revision, Scene)>,
    controls: literal::Controls,
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
    fn selected_layer_object(&self) -> Option<fold_foundation::ObjectId> {
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
        if self
            .draft
            .as_ref()
            .is_some_and(|(base, _)| *base != snapshot.revision())
        {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.draft = None;
        }
        let (base, mut scene) = self.draft.clone().unwrap_or((snapshot.revision(), current));
        let local_time = self.graph.0.borrow().object_time(host);
        let mut response = EditResponse::default();
        if let Some(object) = scope {
            let locked = scene.locked(object);
            if let Some(target) = scene.objects.iter_mut().find(|o| o.id == object) {
                ui.text(&target.name);
                let _locked = ui.begin_disabled_with_cond(locked);
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
                    if let Some(_menu) = ui.begin_popup_context_item_with_label(Some("animation")) {
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
                                            interpolation: crate::binding::Interpolation::Linear,
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
                        self.draft = Some((base, scene));
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
