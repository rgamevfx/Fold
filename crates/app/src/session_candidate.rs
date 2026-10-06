//! Candidate display reuses the bounded preview renderer without changing playback.
use super::*;
use fold_platform::desktop::AgentPreview;
impl Session {
    pub(super) fn candidate_preview_snapshot(
        &self,
    ) -> Result<Option<fold_project::CommittedSnapshot>, String> {
        let Some(p) = &self.automation.preview else {
            return Ok(None);
        };
        let Some(entry) = self.automation.candidates.get(&p.candidate) else {
            return Ok(None);
        };
        if entry.epoch != self.workspace_epoch || entry.value.generation() != p.generation {
            return Ok(None);
        }
        Ok(Some(
            if p.original
                && self
                    .project
                    .snapshot()
                    .state()
                    .documents
                    .contains_key(&p.source.document)
            {
                self.project.snapshot()
            } else {
                entry.value.snapshot().clone()
            },
        ))
    }
    pub(super) fn candidate_preview_info(&self) -> Result<Option<AgentPreview>, String> {
        let Some(snapshot) = self.candidate_preview_snapshot()? else {
            return Ok(None);
        };
        let p = self.automation.preview.as_ref().unwrap();
        let root = snapshot.evaluation();
        let info = self.metadata.borrow_mut().output(&root, &p.source)?;
        let frame = p
            .time
            .to_ticks(info.rate[0], info.rate[1], fold_foundation::Rounding::Floor)
            .map_err(|e| e.to_string())?
            .clamp(0, i64::from(info.frames.saturating_sub(1))) as u32;
        let time = info.time(frame)?;
        let content = self
            .metadata
            .borrow_mut()
            .content(&root, p.source.document)?;
        let scale = (960.0 / info.width.max(info.height) as f64).min(1.0);
        let entry = &self.automation.candidates[&p.candidate];
        Ok(Some(AgentPreview {
            candidate: p.candidate,
            generation: p.generation,
            frames: info.frames,
            conflict: entry.value.base() != self.project.snapshot().revision()
                || self.overlay.is_some()
                || self.state.file_busy,
            original: p.original
                && self
                    .project
                    .snapshot()
                    .state()
                    .documents
                    .contains_key(&p.source.document),
            can_compare: self
                .project
                .snapshot()
                .state()
                .documents
                .contains_key(&p.source.document),
            key: PreviewKey {
                region: None,
                target: Some((p.source.document, time)),
                output: p.source.output.clone(),
                content,
                frame,
                dimensions: [
                    (info.width as f64 * scale).round().max(1.) as u32,
                    (info.height as f64 * scale).round().max(1.) as u32,
                ],
                view: 1,
                channels: Default::default(),
            },
        }))
    }
    pub(super) fn show_candidate(
        &mut self,
        id: u64,
        generation: u64,
        source: fold_project::DocumentRef,
        time: fold_foundation::Time,
    ) -> Result<(), String> {
        let candidate = self.automation.candidate(id, generation)?;
        let info = crate::packages::builtins().output(&candidate.snapshot(), source.document)?;
        if source.output != "video" {
            return Err("candidate preview currently requires video output".into());
        }
        if time < fold_foundation::Time::ZERO || time >= info.time(info.frames)? {
            return Err("preview time outside output range".into());
        }
        self.automation.preview = Some(crate::automation::host::CandidatePreview {
            candidate: id,
            generation,
            source,
            time,
            original: false,
        });
        Ok(())
    }
    pub(super) fn candidate_preview_action(
        &mut self,
        action: &str,
        frame: Option<u32>,
    ) -> Result<(), String> {
        let info = self.candidate_preview_info()?.ok_or("preview expired")?;
        let candidate = self.automation.candidate(info.candidate, info.generation)?;
        match action {
            "frame" => {
                let root = self
                    .candidate_preview_snapshot()?
                    .ok_or("preview expired")?;
                let p = self.automation.preview.as_mut().unwrap();
                let info = crate::packages::builtins().output(&root, p.source.document)?;
                p.time = info.time(
                    frame
                        .ok_or("frame required")?
                        .min(info.frames.saturating_sub(1)),
                )?;
            }
            "compare" => {
                if info.can_compare {
                    let p = self.automation.preview.as_mut().unwrap();
                    p.original = !info.original;
                }
            }
            "apply" => {
                if info.conflict {
                    return Err("Project changed or an artist edit is active; rebuild the candidate before applying".into());
                }
                self.project
                    .commit_named(candidate.proposal(), "Apply agent preview".into())
                    .map_err(|e| e.to_string())?;
                self.automation.candidates.remove(&info.candidate);
                self.automation.preview = None;
                self.refresh();
            }
            "discard" => {
                self.automation.candidates.remove(&info.candidate);
                self.automation.preview = None;
            }
            _ => return Err("unknown preview action".into()),
        }
        Ok(())
    }
}
