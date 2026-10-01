//! Host-owned transient edits and output navigation. Never used by save/export.
use super::*;
use fold_foundation::{Rounding, Time};
use fold_platform::{
    desktop::{Selection, ViewLocation},
    packages::CommandRequest,
};
impl Session {
    pub(super) fn preview_snapshot(&self) -> Snapshot {
        self.overlay
            .as_ref()
            .map(fold_project::EditSession::snapshot)
            .unwrap_or_else(|| self.project.snapshot().evaluation())
    }
    pub(super) fn preview_edit(&mut self, request: CommandRequest) {
        let result = crate::packages::builtins()
            .stage(&self.project.snapshot(), &request, &Cancel::default())
            .and_then(|batch| {
                let session = self
                    .overlay
                    .get_or_insert_with(|| self.project.begin_edit());
                self.project
                    .update_edit(session, batch.mutations)
                    .map_err(|e| e.to_string())
            });
        match result {
            Ok(()) => {
                self.refresh();
                self.state.status = "Preview — uncommitted gesture (Escape cancels)".into();
            }
            Err(error) => {
                self.overlay = None;
                self.refresh();
                self.state.status = error;
            }
        }
    }
    pub(super) fn activate_workspace(&mut self, kind: &str) {
        let snapshot = self.project.snapshot();
        let matches = |id| {
            snapshot
                .state()
                .documents
                .get(&id)
                .is_some_and(|d| d.type_id == kind)
        };
        let current = self
            .state
            .navigation
            .last()
            .map(|v| v.document)
            .or_else(|| {
                workflow::output(&snapshot)
                    .ok()
                    .map(|(source, _)| source.document)
            });
        // Focusing an already active workspace must not restart transport or
        // cancel a live edit. Selection alone is not the viewer context.
        if current.is_some_and(matches) {
            return;
        }
        let location = self
            .state
            .navigation
            .iter()
            .rev()
            .find(|v| matches(v.document))
            .cloned()
            .or_else(|| {
                self.state
                    .selection
                    .document
                    .filter(|&id| matches(id))
                    .or_else(|| {
                        snapshot
                            .state()
                            .documents
                            .values()
                            .find(|d| d.type_id == kind)
                            .map(|d| d.id)
                    })
                    .map(|document| ViewLocation {
                        document,
                        time: Time::ZERO,
                        label: kind.rsplit('.').next().unwrap_or(kind).into(),
                    })
            });
        if let Some(location) = location {
            // navigate truncates to an existing ancestor, restoring its exact
            // saved playhead, selection, preview identity and output metadata.
            let document = location.document;
            self.navigate(location);
            if self
                .state
                .navigation
                .last()
                .is_some_and(|v| v.document == document)
            {
                self.state.status = "Workspace changed".into();
            }
        }
    }
    pub(super) fn navigate(&mut self, location: ViewLocation) {
        let snapshot = self.project.snapshot();
        let result = crate::packages::builtins()
            .output(&snapshot, location.document)
            .and_then(|info| {
                if location.time < Time::ZERO || location.time >= info.time(info.frames)? {
                    return Err("source time is outside the document range".into());
                }
                Ok(info)
            });
        let info = match result {
            Ok(info) => info,
            Err(e) => {
                self.state.status = e;
                return;
            }
        };
        self.stop_playback();
        self.overlay = None;
        if self.state.navigation.is_empty()
            && let Ok((root, root_info)) = workflow::output(&snapshot)
        {
            self.state.navigation.push(ViewLocation {
                document: root.document,
                time: root_info.time(self.state.frame).unwrap_or(Time::ZERO),
                label: "Project output".into(),
            });
        }
        if let Some(index) = self
            .state
            .navigation
            .iter()
            .position(|v| v.document == location.document)
        {
            self.state.navigation.truncate(index);
        }
        self.state.selection = Selection {
            document: Some(location.document),
            objects: vec![],
        };
        self.state.frame = location
            .time
            .to_ticks(info.rate[0], info.rate[1], Rounding::Floor)
            .unwrap_or(0)
            .max(0) as u32;
        self.state.navigation.push(location);
        self.preview.cancel();
        self.refresh();
    }
    pub(super) fn back(&mut self) {
        if self.state.navigation.len() < 2 {
            return;
        }
        self.stop_playback();
        self.overlay = None;
        self.state.navigation.pop();
        let location = self.state.navigation.last().unwrap().clone();
        self.state.selection = Selection {
            document: Some(location.document),
            objects: vec![],
        };
        if let Ok(info) =
            crate::packages::builtins().output(&self.project.snapshot(), location.document)
        {
            self.state.frame = location
                .time
                .to_ticks(info.rate[0], info.rate[1], Rounding::Floor)
                .unwrap_or(0)
                .max(0) as u32;
        }
        self.preview.cancel();
        self.refresh();
    }
}
