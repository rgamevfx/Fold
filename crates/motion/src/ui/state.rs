use crate::{model::Construction, package};
use fold_foundation::DocumentId;
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand},
    packages::CommandRequest,
};
use fold_project::{CommittedSnapshot, Revision};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(super) type Shared = Rc<RefCell<State>>;
pub(super) struct Draft {
    pub text: String,
    pub base: Revision,
    pub dirty: bool,
    snapshot: CommittedSnapshot,
}
#[derive(Default)]
pub(super) struct State {
    pub document: Option<DocumentId>,
    pub graph_definition: Option<fold_foundation::ObjectId>,
    pub graph_history: Vec<fold_foundation::ObjectId>,
    pub base: Revision,
    pub construction: Option<Construction>,
    pub drafts: BTreeMap<DocumentId, Draft>,
    pub editing: bool,
    pub literal_controls: super::literal::Controls,
    pub key_time: Option<(fold_foundation::ObjectId, fold_foundation::Time)>,
    pub error: String,
}
impl State {
    pub fn sync(&mut self, host: &mut dyn DesktopClient) {
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        let desired =
            host.state().selection.document.filter(|id| {
                snapshot.state().documents.get(id).is_some_and(|d| {
                    d.package_id == crate::PACKAGE && d.type_id == crate::CONSTRUCTION
                })
            });
        if self.document == desired && self.base == snapshot.revision() {
            return;
        }
        self.literal_controls = Default::default();
        if self.editing {
            host.command(DesktopCommand::CancelPreviewEdit);
        }
        self.editing = false;
        if self.document.is_some() && self.document != desired {
            self.error.clear();
            self.graph_definition = None;
            self.graph_history.clear();
        }
        self.document = desired;
        self.base = snapshot.revision();
        self.construction = desired
            .and_then(|id| snapshot.state().documents.get(&id))
            .and_then(|d| {
                Construction::from_document(d)
                    .map_err(|e| self.error = e)
                    .ok()
            });
        if let Some(construction) = &self.construction {
            self.literal_controls.set_assets(construction);
        }
        if let (Some(id), Some(construction)) = (desired, &self.construction)
            && self.drafts.get(&id).is_none_or(|draft| !draft.dirty)
        {
            match crate::source::canonical(construction) {
                Ok(text) => {
                    self.drafts.insert(
                        id,
                        Draft {
                            text,
                            base: self.base,
                            snapshot: snapshot.clone(),
                            dirty: false,
                        },
                    );
                }
                Err(error) => self.error = error,
            }
        }
    }

    pub fn reload_source(&mut self, host: &mut dyn DesktopClient) {
        self.sync(host);
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        if let Some(id) = self.document {
            let result = snapshot
                .state()
                .documents
                .get(&id)
                .ok_or_else(|| "Missing Motion document".to_string())
                .and_then(|document| Construction::from_document(document))
                .and_then(|construction| crate::source::canonical(&construction));
            match result {
                Ok(text) => {
                    self.drafts.insert(
                        id,
                        Draft {
                            text,
                            base: self.base,
                            snapshot: snapshot.clone(),
                            dirty: false,
                        },
                    );
                    self.error.clear();
                }
                Err(error) => self.error = error,
            }
        }
    }
    /// Refresh the draft only; Apply remains an explicit, revision-checked action.
    pub fn rebase_source(&mut self, host: &mut dyn DesktopClient) {
        self.sync(host);
        let result = (|| -> Result<Draft, String> {
            let id = self.document.ok_or("No Motion document")?;
            let draft = self.drafts.get(&id).ok_or("No source draft")?;
            let snapshot = host.snapshot().ok_or("No project snapshot")?;
            let parsed = crate::source::parse(&draft.text).map_err(|error| {
                format!(
                    "{} (bytes {}–{})",
                    error.message, error.span.start, error.span.end
                )
            })?;
            let candidate = crate::editing::prepare(
                &draft.snapshot,
                id,
                crate::editing::Scope::Document,
                parsed.construction,
            )
            .map_err(source_error)?;
            let candidate = candidate.rebase(&snapshot).map_err(source_error)?;
            let merged = Construction::from_document(candidate.document())?;
            Ok(Draft {
                text: crate::source::canonical(&merged)?,
                base: snapshot.revision(),
                snapshot,
                dirty: true,
            })
        })();
        match result {
            Ok(draft) => {
                self.drafts
                    .insert(self.document.expect("active document"), draft);
                self.error.clear();
            }
            Err(error) => self.error = error,
        }
    }
    fn request(&self, construction: Construction) -> Result<CommandRequest, String> {
        Ok(CommandRequest {
            id: package::EDIT.into(),
            base: self.base,
            arguments: serde_json::to_vec(&package::EditArgs {
                document: self.document.ok_or("No Motion document")?,
                construction,
            })
            .map_err(|e| e.to_string())?,
        })
    }
    pub fn preview(&mut self, host: &mut dyn DesktopClient) {
        self.editing = true;
        if let Some(construction) = self.construction.clone() {
            match self.request(construction) {
                Ok(request) => host.command(DesktopCommand::PreviewExtension(request)),
                Err(error) => self.error = error,
            }
        }
    }
    pub fn cancel(&mut self, host: &mut dyn DesktopClient) {
        host.command(DesktopCommand::CancelPreviewEdit);
        self.editing = false;
        self.document = None;
        self.sync(host);
    }
    pub fn commit(&mut self, host: &mut dyn DesktopClient, construction: Construction) -> bool {
        let Some(snapshot) = host.snapshot() else {
            return false;
        };
        let Some(document) = self.document else {
            return false;
        };
        if snapshot.revision() != self.base {
            self.error = "Document changed. Reload before applying.".into();
            return false;
        }
        let Some(original) = snapshot.state().documents.get(&document) else {
            return false;
        };
        if Construction::from_document(original).as_ref() == Ok(&construction) {
            self.error.clear();
            // A zero-angle rotation can change its chosen axis without changing
            // the document. Keep that UI draft across a completed no-op gesture.
            let controls = std::mem::take(&mut self.literal_controls);
            self.cancel(host);
            self.literal_controls = controls;
            return true;
        }
        if let Err(error) = crate::editing::validate_document(
            original,
            crate::editing::Scope::Document,
            construction.clone(),
        ) {
            self.error = match error {
                crate::editing::Error::Locked(_) => {
                    "This control or its containing group is locked.".into()
                }
                crate::editing::Error::Invalid(message) => message,
                _ => "This edit could not be applied. Reload the current construction.".into(),
            };
            self.cancel(host);
            return false;
        }
        let request = match self.request(construction.clone()) {
            Ok(request) => request,
            Err(error) => {
                self.error = error;
                return false;
            }
        };
        host.command(DesktopCommand::Extension(request));
        let applied = host
            .snapshot()
            .and_then(|s| s.state().documents.get(&document).cloned())
            .and_then(|d| Construction::from_document(&d).ok())
            .as_ref()
            == Some(&construction);
        if applied {
            self.error.clear();
        } else {
            self.error = format!("Motion edit was not applied. {}", host.state().status);
        }
        self.editing = false;
        self.document = None;
        self.sync(host);
        applied
    }
}

fn source_error(error: crate::editing::Error) -> String {
    use crate::editing::Error;
    match error {
        Error::Invalid(message) => message,
        Error::Validation(diagnostic) => diagnostic.message,
        Error::Locked(_) => "This control or its containing group is locked.".into(),
        Error::RebaseConflict(conflicts) => format!(
            "Source rebase conflicts: {}",
            conflicts
                .iter()
                .map(|conflict| format!("{} at {:?}", conflict.reason, conflict.path))
                .collect::<Vec<_>>()
                .join("; ")
        ),
        Error::NoChange => "The draft has no changes to rebase. Reload the current source.".into(),
        _ => "Source could not be rebased against this document.".into(),
    }
}
