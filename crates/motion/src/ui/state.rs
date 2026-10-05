use crate::{MOTION, Motion, package};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, ViewLocation},
    packages::CommandRequest,
};
use fold_project::Revision;
use std::{cell::RefCell, rc::Rc};
pub type Shared = Rc<RefCell<State>>;
#[derive(Default)]
pub struct State {
    pub document: Option<DocumentId>,
    pub motion: Option<Motion>,
    pub selected: Vec<ObjectId>,
    pub group: Option<ObjectId>,
    pub network_object: Option<ObjectId>,
    pub document_network: bool,
    pub parameter_sockets: bool,
    pub parents: Vec<Option<ObjectId>>,
    pub error: String,
    pub editing: bool,
    pub auto_key: bool,
    pub network_requested: bool,
    pub path_attribute_names: std::collections::BTreeMap<(ObjectId, String), String>,
    /// Only the inspector may finish/cancel a property gesture.
    pub property_editing: bool,
    pub interface_editing: bool,
    pub generation: u64,
    pub visual_revision: u64,
    pub scene_scope: Option<ObjectId>,
    pub source_history: Vec<ObjectId>,
    pub point_edit_requested: bool,
    pub reference_pick: Option<(ObjectId, String)>,
    base: Revision,
    original: Option<Motion>,
}
impl State {
    pub fn sync(&mut self, host: &mut dyn DesktopClient) {
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        let chosen = host
            .state()
            .selection
            .document
            .filter(|id| {
                snapshot
                    .state()
                    .documents
                    .get(id)
                    .is_some_and(|d| d.type_id == MOTION)
            })
            .or(self.document.filter(|id| {
                !host.explicit_target() && snapshot.state().documents.contains_key(id)
            }))
            .or_else(|| {
                if host.explicit_target() {
                    return None;
                }
                snapshot
                    .state()
                    .documents
                    .values()
                    .find(|d| d.type_id == MOTION)
                    .map(|d| d.id)
            });
        if chosen != self.document || self.base != snapshot.revision() {
            if self.editing {
                host.command(DesktopCommand::CancelPreviewEdit);
            }
            if chosen != self.document {
                self.group = None;
                self.network_object = None;
                self.scene_scope = None;
                self.source_history.clear();
                self.reference_pick = None;
                self.document_network = false;
                self.parents.clear();
                self.selected.clear();
            }
            self.document = chosen;
            self.base = snapshot.revision();
            self.motion =
                chosen.and_then(|id| Motion::from_document(&snapshot.state().documents[&id]).ok());
            self.original = self.motion.clone();
            self.editing = false;
            self.property_editing = false;
            self.interface_editing = false;
            self.path_attribute_names.clear();
            self.generation += 1;
            self.visual_revision += 1;
            if self.group.is_some_and(|id| {
                !self
                    .motion
                    .as_ref()
                    .is_some_and(|m| m.groups.contains_key(&id))
            }) {
                self.group = None;
                self.network_object = None;
                self.scene_scope = None;
                self.source_history.clear();
                self.reference_pick = None;
                self.document_network = false;
                self.parents.clear();
            }
            let view = self.view_motion();
            self.selected.retain(|id| {
                view.as_ref()
                    .is_some_and(|m| m.graph.nodes.iter().any(|n| n.id == *id))
            });
        }
    }
    pub fn view_motion(&self) -> Option<Motion> {
        let mut motion = self.motion.clone()?;
        if let Some(id) = self.group {
            motion.graph = motion.groups.get(&id)?.graph.clone();
            motion.scene = None;
        }
        Some(motion)
    }
    pub fn replace_view(&mut self, mut view: Motion) {
        if let Some(id) = self.group {
            view.scene = self.motion.as_ref().and_then(|m| m.scene.clone());
            let graph = view.graph;
            view.graph = self
                .motion
                .as_ref()
                .expect("editing document")
                .graph
                .clone();
            if let Some(group) = view.groups.get_mut(&id) {
                group.graph = graph;
            }
        }
        self.motion = Some(view);
    }
    fn request(&self) -> Result<CommandRequest, String> {
        Ok(CommandRequest {
            id: package::EDIT.into(),
            base: self.base,
            arguments: serde_json::to_vec(&package::EditArgs {
                document: self.document.ok_or("no motion document")?,
                motion: self.motion.clone().ok_or("no motion graph")?,
            })
            .map_err(|e| e.to_string())?,
        })
    }
    pub fn preview(&mut self, host: &mut dyn DesktopClient) {
        self.visual_revision += 1;
        self.editing = true;
        match self
            .motion
            .as_ref()
            .ok_or("no motion graph".into())
            .and_then(|m| m.validate())
            .and_then(|_| self.request())
        {
            Ok(request) => {
                host.command(DesktopCommand::PreviewExtension(request));
                self.error.clear();
            }
            Err(e) => {
                host.command(DesktopCommand::CancelPreviewEdit);
                self.error = e;
            }
        }
    }
    pub fn commit(&mut self, host: &mut dyn DesktopClient) {
        if self.motion == self.original {
            self.cancel(host);
            return;
        }
        if let (Some(before), Some(after)) = (&self.original, &mut self.motion) {
            super::network_scope::retain(before, after);
        }
        match self
            .motion
            .as_ref()
            .ok_or("no motion graph".into())
            .and_then(|m| m.validate())
            .and_then(|_| self.request())
        {
            Ok(request) => {
                host.command(DesktopCommand::Extension(request));
                if host.snapshot().is_some_and(|s| s.revision() != self.base) {
                    self.editing = false;
                    self.property_editing = false;
                    self.interface_editing = false;
                    self.sync(host);
                    self.error.clear();
                } else {
                    let error = host.state().status.clone();
                    self.cancel(host);
                    self.error = error;
                }
            }
            Err(error) => {
                self.cancel(host);
                self.error = error;
            }
        }
    }
    pub fn cancel(&mut self, host: &mut dyn DesktopClient) {
        host.command(DesktopCommand::CancelPreviewEdit);
        self.motion = self.original.clone();
        self.editing = false;
        self.property_editing = false;
        self.interface_editing = false;
        self.generation += 1;
        self.visual_revision += 1;
    }
    pub fn change_scene(
        &mut self,
        host: &mut dyn DesktopClient,
        f: impl FnOnce(&mut Motion) -> Result<Option<ObjectId>, String>,
    ) {
        self.group = None;
        self.parents.clear();
        self.change(host, f);
        host.command(DesktopCommand::Select(fold_platform::desktop::Selection {
            document: self.document,
            objects: self.selected.clone(),
        }));
    }
    pub fn change(
        &mut self,
        host: &mut dyn DesktopClient,
        f: impl FnOnce(&mut Motion) -> Result<Option<ObjectId>, String>,
    ) {
        self.cancel(host);
        if let Some(mut motion) = self.view_motion() {
            match f(&mut motion) {
                Ok(selected) => {
                    self.replace_view(motion);
                    if selected.is_some() {
                        self.selected = selected.into_iter().collect();
                    }
                    self.commit(host);
                }
                Err(e) => {
                    self.motion = self.original.clone();
                    self.error = e;
                }
            }
        }
    }
    pub fn create(&mut self, host: &mut dyn DesktopClient) {
        self.cancel(host);
        self.group = None;
        self.parents.clear();
        self.selected.clear();
        self.document = Some(DocumentId::new());
        self.motion = Some(Motion::new_scene());
        self.original = None;
        if let Some(snapshot) = host.snapshot() {
            self.base = snapshot.revision();
            if let Ok(request) = self.request() {
                host.command(DesktopCommand::Extension(request));
                if host.snapshot().is_some_and(|s| {
                    self.document
                        .is_some_and(|id| s.state().documents.contains_key(&id))
                }) {
                    self.view(host);
                    self.sync(host);
                } else {
                    self.error = host.state().status.clone();
                    self.motion = None;
                    self.document = None;
                }
            }
        }
    }
    pub fn view(&self, host: &mut dyn DesktopClient) {
        if let Some(document) = self.document {
            host.command(DesktopCommand::Navigate(ViewLocation {
                document,
                time: Time::ZERO,
                label: "Motion".into(),
            }));
        }
    }
}
