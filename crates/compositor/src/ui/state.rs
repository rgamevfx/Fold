use crate::{Composite, package};
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
    pub lens_grid: bool,
    pub qualifier: Rc<RefCell<super::qualifier::Editor>>,
    pub roto: Rc<RefCell<super::roto::Editor>>,
    roto_sample: Option<(u64, ObjectId, Time, crate::roto::Roto)>,
    pub document: Option<DocumentId>,
    pub base: Revision,
    pub original: Option<Composite>,
    pub graph: Option<Composite>,
    pub selected: Vec<ObjectId>,
    pub error: String,
    pub generation: u64,
    pub editing: bool,
    /// Only the inspector may finish/cancel a property gesture.
    pub property_editing: bool,
    pub shader_pending: Option<CommandRequest>,
    root: Option<std::sync::Arc<fold_project::Document>>,
    view: Option<DocumentId>,
}
impl State {
    pub fn roto_at(&mut self, id: ObjectId, time: Time) -> Result<crate::roto::Roto, String> {
        if let Some((generation, node, at, settings)) = &self.roto_sample {
            if *generation == self.generation && *node == id && *at == time {
                return Ok(settings.clone());
            }
        }
        let crate::OperationConfig::Roto { settings } = self
            .graph
            .as_ref()
            .ok_or("Missing composite")?
            .prepare()?
            .evaluated_parameters(id, time)?
        else {
            return Err("Not a Roto node".into());
        };
        self.roto_sample = Some((self.generation, id, time, settings.clone()));
        Ok(settings)
    }

    pub fn sync(&mut self, host: &mut dyn DesktopClient) {
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        if self
            .shader_pending
            .as_ref()
            .is_some_and(|r| !host.extension_pending(r))
            && snapshot.revision() == self.base
        {
            self.shader_pending = None;
            self.property_editing = false;
            host.command(DesktopCommand::CancelPreviewEdit);
            self.error = format!("Shader draft was not published. {}", host.state().status);
        }
        let view = host.state().navigation.last().map(|v| v.document);
        if self.view != view {
            if self.editing {
                self.cancel(host);
            }
            self.view = view;
        }
        let desired = host
            .state()
            .selection
            .document
            .filter(|id| {
                snapshot
                    .state()
                    .documents
                    .get(id)
                    .is_some_and(|d| d.type_id == crate::COMPOSITE)
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
                    .find(|d| d.type_id == crate::COMPOSITE)
                    .map(|d| d.id)
            });
        let root = desired
            .and_then(|id| snapshot.state().documents.get(&id))
            .cloned();
        let replaced = match (&self.root, &root) {
            (Some(a), Some(b)) => !std::sync::Arc::ptr_eq(a, b),
            (None, None) => false,
            _ => true,
        };
        if self.document != desired || self.base != snapshot.revision() || replaced {
            if self.editing {
                host.command(DesktopCommand::CancelPreviewEdit);
            }
            if self.shader_pending.is_some() {
                self.shader_pending = None;
                self.error.clear();
            }
            self.document = desired;
            self.root = root;
            self.base = snapshot.revision();
            self.editing = false;
            self.property_editing = false;
            self.graph = desired
                .and_then(|id| Composite::from_document(&snapshot.state().documents[&id]).ok());
            self.original = self.graph.clone();
            self.selected.retain(|id| {
                self.graph
                    .as_ref()
                    .is_some_and(|g| g.nodes.iter().any(|n| n.id == *id))
            });
            self.generation += 1;
            self.roto_sample = None;
        }
    }
    pub fn request(&self) -> Result<CommandRequest, String> {
        Ok(CommandRequest {
            id: package::EDIT.into(),
            base: self.base,
            arguments: serde_json::to_vec(&package::EditArgs {
                document: self.document.ok_or("no composite")?,
                composite: self.graph.clone().ok_or("no graph")?,
            })
            .map_err(|e| e.to_string())?,
        })
    }
    pub fn preview(&mut self, host: &mut dyn DesktopClient) {
        self.roto_sample = None;
        self.editing = true;
        match self
            .graph
            .as_ref()
            .ok_or("no graph".into())
            .and_then(|g| g.validate())
            .and_then(|()| self.request())
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
        if self.shader_pending.is_some() {
            return;
        }
        if self.graph == self.original {
            self.cancel(host);
            return;
        }
        let result = self
            .graph
            .as_ref()
            .ok_or("no graph".into())
            .and_then(|g| g.validate())
            .and_then(|()| self.request());
        match result {
            Ok(mut request) => {
                let shader_changed = self.graph.as_ref().is_some_and(|graph| {
                    crate::shader::requires_validation(self.original.as_ref(), graph)
                });
                if shader_changed {
                    request.id = package::SHADER_EDIT.into();
                }
                host.command(DesktopCommand::Extension(request.clone()));
                if shader_changed && host.extension_pending(&request) {
                    self.shader_pending = Some(request);
                    self.property_editing = false;
                    self.error.clear();
                    return;
                }
                if host.snapshot().is_some_and(|s| s.revision() == self.base) {
                    self.error = host.state().status.clone();
                    self.cancel(host);
                } else {
                    self.editing = false;
                    self.property_editing = false;
                    self.sync(host);
                    self.error.clear();
                }
            }
            Err(e) => {
                self.cancel(host);
                self.error = e;
            }
        }
    }
    pub fn cancel(&mut self, host: &mut dyn DesktopClient) {
        if let Some(request) = self.shader_pending.take() {
            host.command(DesktopCommand::CancelExtension(request));
        }
        host.command(DesktopCommand::CancelPreviewEdit);
        self.graph = self.original.clone();
        self.selected.retain(|id| {
            self.graph
                .as_ref()
                .is_some_and(|g| g.nodes.iter().any(|n| n.id == *id))
        });
        self.editing = false;
        self.property_editing = false;
        self.generation += 1;
        self.roto_sample = None;
    }
    pub fn view(&self, host: &mut dyn DesktopClient) {
        if let Some(document) = self.document {
            host.command(DesktopCommand::Navigate(ViewLocation {
                document,
                time: Time::ZERO,
                label: "Composite".into(),
            }));
        }
    }
}
