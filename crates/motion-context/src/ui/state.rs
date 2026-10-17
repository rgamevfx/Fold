use crate::package::{self, EditArgs};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion_canvas::{
    editing::{self, Edit},
    model::Canvas,
};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, Selection},
    packages::CommandRequest,
};
use fold_project::Revision;

pub(super) struct State {
    pub document: Option<DocumentId>,
    pub base: Revision,
    pub time: Time,
    pub before: Option<Canvas>,
    pub draft: Option<Canvas>,
    pub selected: Vec<ObjectId>,
    pub point: Option<(ObjectId, ObjectId)>,
    pub graph_group: Option<ObjectId>,
    pub settings: Option<(Canvas, Canvas)>,
    pub editing: bool,
    pub error: String,
    pub generation: u64,
    pub edits: Vec<Edit>,
    pending: Option<CommandRequest>,
    cancelling: bool,
}
impl Default for State {
    fn default() -> Self {
        Self {
            document: None,
            base: Revision::default(),
            time: Time::ZERO,
            before: None,
            draft: None,
            selected: vec![],
            point: None,
            graph_group: None,
            settings: None,
            editing: false,
            error: String::new(),
            generation: 0,
            edits: vec![],
            pending: None,
            cancelling: false,
        }
    }
}
impl State {
    pub fn sync(&mut self, host: &mut dyn DesktopClient) {
        let Some(snapshot) = host.snapshot() else {
            self.cancel(host);
            self.before = None;
            self.point = None;
            return;
        };
        let document = host.state().selection.document.filter(|id| {
            snapshot
                .state()
                .documents
                .get(id)
                .is_some_and(|d| d.type_id == fold_motion_canvas::TYPE)
        });
        let time = host
            .state()
            .navigation
            .last()
            .filter(|n| Some(n.document) == document)
            .map_or(Time::ZERO, |n| n.time);
        let selected = host.state().selection.objects.clone();
        self.sync_target(host, document, selected, time);
    }
    pub(super) fn sync_target(
        &mut self,
        host: &mut dyn DesktopClient,
        document: Option<DocumentId>,
        selected: Vec<ObjectId>,
        time: Time,
    ) {
        let Some(snapshot) = host.snapshot() else {
            self.cancel(host);
            self.before = None;
            return;
        };
        let selection_changed = selected != self.selected || document != self.document;
        if selection_changed {
            self.point = None;
        }
        let changed =
            document != self.document || time != self.time || self.base != snapshot.revision();
        if changed || host.state().playing || (selected != self.selected && self.editing) {
            self.cancel(host);
        }
        if !changed {
            self.selected = selected;
            if selection_changed {
                self.reveal_selection();
            }
            if let Some(request) = &self.pending {
                if host.extension_pending(request) {
                    return;
                }
                let error = host.extension_error(request);
                self.pending = None;
                self.draft = None;
                self.error = if self.cancelling {
                    String::new()
                } else {
                    error.unwrap_or_else(|| {
                        "Motion edit was not committed. Check project status.".into()
                    })
                };
                self.cancelling = false;
            }
            return;
        }
        self.pending = None;
        self.cancelling = false;
        if document != self.document {
            self.graph_group = None;
            self.settings = None;
        }
        self.document = document;
        self.time = time;
        self.base = snapshot.revision();
        self.selected = selected;
        self.before = document
            .and_then(|id| snapshot.state().documents.get(&id))
            .and_then(|d| match Canvas::from_document(d) {
                Ok(canvas) => Some(canvas),
                Err(error) => {
                    self.error = error;
                    None
                }
            });
        self.draft = None;
        if self.point.is_some_and(|(object, point)| {
            !self.selected.contains(&object) || self.before.as_ref().is_none_or(|canvas| {
                canvas.objects.get(&object).is_none_or(|object| {
                    !matches!(&object.content, fold_motion_canvas::model::Content::Path { contours, .. }
                        if contours.iter().flat_map(|c| &c.points).any(|p| p.id == point))
                })
            })
        }) { self.point = None; }
        if self.graph_group.is_some_and(|id| {
            self.before.as_ref().is_none_or(|canvas| {
                canvas.objects.get(&id).is_none_or(|object| {
                    !matches!(
                        object.content,
                        fold_motion_canvas::model::Content::Group { .. }
                    )
                })
            })
        }) {
            self.graph_group = None;
            self.settings = None;
        }
        if selection_changed {
            self.reveal_selection();
        }
    }
    pub fn canvas(&self) -> Option<&Canvas> {
        self.draft.as_ref().or(self.before.as_ref())
    }
    pub fn busy(&self) -> bool {
        self.pending.is_some()
    }
    pub fn conversion_pending(&self) -> bool {
        self.pending
            .as_ref()
            .is_some_and(|request| request.id == package::CONVERT)
    }
    pub fn cancelling(&self) -> bool {
        self.cancelling
    }
    pub fn cancel_conversion(&mut self, host: &mut dyn DesktopClient) {
        if self.conversion_pending() && !self.cancelling {
            if let Some(request) = &self.pending {
                host.command(DesktopCommand::CancelExtension(request.clone()));
                self.cancelling = true;
            }
        }
    }
    pub fn animate(
        &mut self,
        host: &mut dyn DesktopClient,
        object: ObjectId,
        spec: fold_motion_canvas::motion::Spec,
    ) -> Result<(), String> {
        if self.busy() || self.editing || host.state().playing {
            return Err("Finish the current edit and stop playback before updating motion".into());
        }
        let request = CommandRequest {
            id: package::ANIMATE.into(),
            base: self.base,
            arguments: serde_json::to_vec(&package::AnimateArgs {
                document: self.document.ok_or("No Motion document")?,
                object,
                spec,
            })
            .map_err(|e| e.to_string())?,
        };
        self.error.clear();
        host.command(DesktopCommand::Extension(request.clone()));
        self.pending = Some(request);
        Ok(())
    }
    pub fn convert(
        &mut self,
        host: &mut dyn DesktopClient,
        object: ObjectId,
    ) -> Result<(), String> {
        if self.busy() || self.editing || host.state().playing {
            return Err("Finish the current edit and stop playback before converting".into());
        }
        let request = CommandRequest {
            id: package::CONVERT.into(),
            base: self.base,
            arguments: serde_json::to_vec(&package::ConvertArgs {
                document: self.document.ok_or("No Motion document")?,
                object,
                time: self.time,
            })
            .map_err(|error| error.to_string())?,
        };
        self.error.clear();
        host.command(DesktopCommand::Extension(request.clone()));
        self.pending = Some(request);
        Ok(())
    }
    pub fn select(&mut self, host: &mut dyn DesktopClient, objects: Vec<ObjectId>) {
        self.cancel(host);
        self.point = None;
        self.selected = objects.clone();
        self.reveal_selection();
        host.command(DesktopCommand::Select(Selection {
            document: self.document,
            objects,
        }));
    }
    fn reveal_selection(&mut self) {
        let [id] = self.selected.as_slice() else {
            return;
        };
        // The opened group is also visible as the membership anchor. Selecting
        // it must not unexpectedly navigate out of the current graph.
        if Some(*id) == self.graph_group {
            return;
        }
        if let Some(canvas) = self.canvas() {
            if let Ok((parent, _)) = fold_motion_canvas::hierarchy::placement(canvas, *id) {
                self.graph_group = parent;
            }
        }
    }
    pub fn preview(
        &mut self,
        host: &mut dyn DesktopClient,
        edits: Vec<Edit>,
    ) -> Result<(), String> {
        if self.pending.is_some() {
            return Err("Wait for the current Motion edit".into());
        }
        let before = self.before.as_ref().ok_or("No Motion document")?;
        let draft = editing::apply(before, &edits)?;
        // A held pointer can produce many UI frames without changing artwork.
        // Keep the active gesture, but do not invalidate its rendered preview.
        if self.editing && self.draft.as_ref() == Some(&draft) {
            return Ok(());
        }
        let request = self.request(&edits)?;
        self.edits = edits;
        self.error.clear();
        self.draft = Some(draft);
        self.editing = true;
        host.command(DesktopCommand::PreviewExtension(request));
        Ok(())
    }
    pub fn commit(&mut self, host: &mut dyn DesktopClient) -> Result<(), String> {
        if !self.editing {
            return Ok(());
        }
        if self.draft == self.before {
            self.cancel(host);
            return Ok(());
        }
        let request = self.request(&self.edits)?;
        host.command(DesktopCommand::Extension(request.clone()));
        self.pending = Some(request);
        self.editing = false;
        self.edits.clear();
        self.sync_target(host, self.document, self.selected.clone(), self.time);
        Ok(())
    }
    pub fn cancel(&mut self, host: &mut dyn DesktopClient) {
        self.generation = self.generation.wrapping_add(1);
        if self.editing {
            host.command(DesktopCommand::CancelPreviewEdit);
        }
        self.editing = false;
        self.edits.clear();
        self.draft = None;
    }
    fn request(&self, edits: &[Edit]) -> Result<CommandRequest, String> {
        Ok(CommandRequest {
            id: package::EDIT.into(),
            base: self.base,
            arguments: serde_json::to_vec(&EditArgs {
                document: self.document.ok_or("No Motion document")?,
                edits: edits.to_vec(),
            })
            .map_err(|e| e.to_string())?,
        })
    }
}
