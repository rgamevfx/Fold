use crate::{
    editing::{self, Edit},
    model::Scene,
    package::{self, EditArgs},
};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, Selection},
    packages::CommandRequest,
};
use fold_project::Revision;

pub(super) struct State {
    pub document: Option<DocumentId>,
    pub selected: Vec<ObjectId>,
    pub time: Time,
    pub scene: Option<Scene>,
    pub draft: Option<Scene>,
    pub error: String,
    base: Revision,
    edits: Vec<Edit>,
    pending: Option<CommandRequest>,
    pending_selection: Option<(Selection, Selection)>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            document: None,
            selected: vec![],
            time: Time::ZERO,
            scene: None,
            draft: None,
            error: String::new(),
            base: Revision::default(),
            edits: vec![],
            pending: None,
            pending_selection: None,
        }
    }
}
impl State {
    pub fn revision(&self) -> Revision {
        self.base
    }
    pub fn sync(&mut self, host: &mut dyn DesktopClient) {
        let Some(snapshot) = host.snapshot() else {
            self.cancel(host);
            self.scene = None;
            self.document = None;
            self.pending = None;
            self.pending_selection = None;
            return;
        };
        let document = host.state().selection.document.filter(|id| {
            snapshot
                .state()
                .documents
                .get(id)
                .is_some_and(|d| d.type_id == crate::TYPE)
        });
        let time = host
            .state()
            .navigation
            .last()
            .filter(|n| Some(n.document) == document)
            .map_or(Time::ZERO, |n| n.time);
        let selected = host.state().selection.objects.clone();
        let changed =
            document != self.document || time != self.time || snapshot.revision() != self.base;
        if changed || selected != self.selected || host.state().playing {
            self.cancel(host);
        }
        self.selected = selected;
        let mut selection = None;
        if let Some(request) = &self.pending
            && !host.extension_pending(request)
        {
            self.error = host.extension_error(request).unwrap_or_else(|| {
                if snapshot.revision() == self.base {
                    "3D edit was not committed. Check project status.".into()
                } else {
                    String::new()
                }
            });
            self.pending = None;
            self.draft = None;
            if self.error.is_empty() {
                selection = self.pending_selection.take();
            } else {
                self.pending_selection = None;
            }
        }
        if changed {
            self.base = snapshot.revision();
            self.document = document;
            self.time = time;
            self.scene = document
                .and_then(|id| snapshot.state().documents.get(&id))
                .and_then(|d| match Scene::from_document(d) {
                    Ok(scene) => Some(scene),
                    Err(error) => {
                        self.error = error;
                        None
                    }
                });
        }
        if let Some((expected, target)) = selection
            && host.state().selection == expected
            && target.document == self.document
            && self.scene.as_ref().is_some_and(|scene| {
                target
                    .objects
                    .iter()
                    .all(|id| scene.objects.contains_key(id))
            })
        {
            self.selected = target.objects.clone();
            host.command(DesktopCommand::Select(target));
        }
    }
    pub fn current(&self) -> Option<&Scene> {
        self.draft.as_ref().or(self.scene.as_ref())
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn editing(&self) -> bool {
        !self.edits.is_empty()
    }
    fn request(&self, edits: &[Edit]) -> Result<CommandRequest, String> {
        Ok(CommandRequest {
            id: package::EDIT.into(),
            base: self.base,
            arguments: serde_json::to_vec(&EditArgs {
                document: self.document.ok_or("No 3D document")?,
                edits: edits.to_vec(),
            })
            .map_err(|e| e.to_string())?,
        })
    }
    pub fn preview(
        &mut self,
        host: &mut dyn DesktopClient,
        edits: Vec<Edit>,
    ) -> Result<(), String> {
        if self.busy() {
            return Err("Wait for the current 3D edit".into());
        }
        let draft = editing::apply(
            self.scene.as_ref().ok_or("No 3D document")?,
            &edits,
            &Default::default(),
        )?;
        let request = self.request(&edits)?;
        self.edits = edits;
        self.draft = Some(draft);
        self.error.clear();
        host.command(DesktopCommand::PreviewExtension(request));
        Ok(())
    }
    pub fn commit(&mut self, host: &mut dyn DesktopClient) -> Result<(), String> {
        if !self.editing() {
            return Ok(());
        }
        if self.draft == self.scene {
            self.cancel(host);
            return Ok(());
        }
        let request = self.request(&self.edits)?;
        host.command(DesktopCommand::Extension(request.clone()));
        self.pending = Some(request);
        self.edits.clear();
        Ok(())
    }
    pub fn create(
        &mut self,
        host: &mut dyn DesktopClient,
        kind: crate::tools::Kind,
    ) -> Result<(), String> {
        if self.busy() || self.editing() || host.state().playing {
            return Err("Finish the current edit and stop playback before adding objects".into());
        }
        let scene = self.current().ok_or("No 3D document")?;
        let parent = self
            .selected
            .first()
            .filter(|_| self.selected.len() == 1)
            .filter(|id| {
                matches!(
                    scene.objects.get(id).map(|o| &o.content),
                    Some(crate::model::Content::Group { .. })
                )
            })
            .copied();
        let (id, edits) = crate::tools::create(scene, kind, parent)?;
        let expected = host.state().selection.clone();
        self.preview(host, edits)?;
        self.commit(host)?;
        self.pending_selection = Some((
            expected,
            Selection {
                document: self.document,
                objects: vec![id],
            },
        ));
        Ok(())
    }
    pub fn duplicate(&mut self, host: &mut dyn DesktopClient, id: ObjectId) -> Result<(), String> {
        if self.busy() || self.editing() || host.state().playing {
            return Err(
                "Finish the current edit and stop playback before duplicating objects".into(),
            );
        }
        let (id, edits) = crate::tools::duplicate(self.current().ok_or("No 3D document")?, id)?;
        let expected = host.state().selection.clone();
        self.preview(host, edits)?;
        self.commit(host)?;
        self.pending_selection = Some((
            expected,
            Selection {
                document: self.document,
                objects: vec![id],
            },
        ));
        Ok(())
    }
    pub fn make_unique(
        &mut self,
        host: &mut dyn DesktopClient,
        id: ObjectId,
        geometry: bool,
    ) -> Result<(), String> {
        if self.busy() || self.editing() || host.state().playing {
            return Err(
                "Finish the current edit and stop playback before copying resources".into(),
            );
        }
        let edits =
            crate::tools::make_unique(self.current().ok_or("No 3D document")?, id, geometry)?;
        if edits.is_empty() {
            return Ok(());
        }
        self.preview(host, edits)?;
        self.commit(host)
    }
    pub fn move_to(
        &mut self,
        host: &mut dyn DesktopClient,
        id: ObjectId,
        parent: Option<ObjectId>,
        keep_world: bool,
    ) -> Result<(), String> {
        if self.busy() || self.editing() || host.state().playing {
            return Err("Finish the current edit and stop playback before moving objects".into());
        }
        let scene = self.current().ok_or("No 3D document")?;
        let edits = if keep_world {
            vec![Edit::MoveWorld {
                id,
                expected: editing::placement(scene, id)?,
                parent,
            }]
        } else {
            crate::tools::move_to(scene, id, parent)?
        };
        if edits.is_empty() {
            return Ok(());
        }
        self.preview(host, edits)?;
        self.commit(host)
    }
    pub fn group(&mut self, host: &mut dyn DesktopClient) -> Result<(), String> {
        if self.busy() || self.editing() || host.state().playing {
            return Err("Finish the current edit and stop playback before grouping objects".into());
        }
        let (id, edits) =
            crate::tools::group(self.current().ok_or("No 3D document")?, &self.selected)?;
        let expected = host.state().selection.clone();
        self.preview(host, edits)?;
        self.commit(host)?;
        self.pending_selection = Some((
            expected,
            Selection {
                document: self.document,
                objects: vec![id],
            },
        ));
        Ok(())
    }
    pub fn delete(&mut self, host: &mut dyn DesktopClient, id: ObjectId) -> Result<(), String> {
        if self.busy() || self.editing() || host.state().playing {
            return Err("Finish the current edit and stop playback before deleting objects".into());
        }
        let edits = crate::tools::delete(self.current().ok_or("No 3D document")?, id)?;
        let expected = host.state().selection.clone();
        self.preview(host, edits)?;
        let objects = self
            .selected
            .iter()
            .copied()
            .filter(|id| {
                self.current()
                    .is_some_and(|scene| scene.objects.contains_key(id))
            })
            .collect();
        self.commit(host)?;
        self.pending_selection = Some((
            expected,
            Selection {
                document: self.document,
                objects,
            },
        ));
        Ok(())
    }
    pub fn cancel(&mut self, host: &mut dyn DesktopClient) {
        if self.editing() {
            host.command(DesktopCommand::CancelPreviewEdit);
        }
        self.edits.clear();
        self.draft = None;
    }
}
