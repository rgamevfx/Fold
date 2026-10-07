//! Transition inspector and default creation; all edits use the timeline transaction path.
use crate::{Sequence, SequenceEdit, Transition};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::desktop::{DesktopClient, DesktopCommand, ViewLocation};
use fold_project::Revision;
use fold_ui::sdk::{
    effects,
    imgui::{self, Ui},
};
#[derive(Default)]
pub(super) struct Editor {
    key: Option<(ObjectId, Revision)>,
    draft: Option<Transition>,
    editing: bool,
}
impl Editor {
    pub fn cancel(&mut self, host: &mut dyn DesktopClient) {
        if self.editing {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.editing = false;
            self.key = None;
        }
    }

    pub fn draw(
        &mut self,
        ui: &Ui,
        host: &mut dyn DesktopClient,
        document: DocumentId,
        sequence: &Sequence,
        selected: ObjectId,
        revision: Revision,
    ) {
        let Some(original) = sequence.transitions.iter().find(|t| t.id == selected) else {
            return;
        };
        if self.key != Some((selected, revision)) {
            if self.editing {
                host.command(DesktopCommand::CancelPreviewEdit);
            }
            self.key = Some((selected, revision));
            self.draft = Some(original.clone());
            self.editing = false;
        }
        let transition = self.draft.as_mut().unwrap();
        let outgoing = sequence
            .clips
            .iter()
            .find(|c| c.id == transition.outgoing)
            .unwrap();
        let _disabled = ui.begin_disabled_with_cond(sequence.track(outgoing.track).unwrap().locked);
        ui.text_wrapped(effects::name(host, &transition.effect.definition));
        if ui.small_button("Edit Graph") {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.editing = false;
            host.command(DesktopCommand::Navigate(ViewLocation {
                document: transition.effect.definition.document,
                time: Time::ZERO,
                label: "Transition".into(),
            }));
            return;
        }
        let cut = outgoing.end().unwrap();
        let mut response = fold_ui::sdk::EditResponse::default();
        let mut frames =
            super::interaction::frame(transition.duration, sequence.rate).round() as i32;
        if imgui::Drag::new("Duration")
            .display_format(fold_ui::sdk::imgui::NumericFormat::new("%d fr").unwrap())
            .speed(1.)
            .range(1, 18000)
            .build(ui, &mut frames)
        {
            let duration = super::interaction::time(i64::from(frames), sequence.rate).unwrap();
            let lead = cut.checked_sub(transition.start).unwrap();
            let ratio = super::interaction::frame(lead, sequence.rate)
                / super::interaction::frame(transition.duration, sequence.rate);
            let lead =
                super::interaction::time((f64::from(frames) * ratio).round() as i64, sequence.rate)
                    .unwrap();
            transition.start = cut.checked_sub(lead).unwrap();
            transition.duration = duration;
            response.changed = true;
        }
        response.finished |= ui.is_item_deactivated_after_edit();
        let alignment = if transition.start == cut {
            "After cut"
        } else if transition.end().ok() == Some(cut) {
            "Before cut"
        } else {
            "Centered / custom"
        };
        if let Some(_combo) = ui.begin_combo("Alignment", alignment) {
            for (label, divisor) in [("Before cut", 1), ("Centered", 2), ("After cut", 0)] {
                if ui.selectable(label) {
                    let lead = if divisor == 0 {
                        Time::ZERO
                    } else {
                        transition.duration.checked_scale(1, divisor).unwrap()
                    };
                    transition.start = cut.checked_sub(lead).unwrap();
                    response.changed = true;
                    response.finished = true;
                }
            }
        }
        let time = host
            .state()
            .navigation
            .last()
            .filter(|v| v.document == document)
            .map(|v| v.time)
            .unwrap_or(transition.start);
        effects::parameters(ui, host, &mut transition.effect, time, &mut response);
        if let Some((definition, interface)) =
            effects::picker(ui, host, "Replace", &["outgoing", "incoming"])
        {
            transition.effect = effects::new_instance(definition, &interface);
            response.changed = true;
            response.finished = true;
        }
        if ui.small_button("Remove Transition") {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.editing = false;
            super::submit(
                host,
                revision,
                document,
                SequenceEdit::RemoveTransition { id: selected },
            );
            return;
        }
        if ui.is_key_pressed(imgui::Key::Escape) || response.cancelled {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.draft = Some(original.clone());
            self.editing = false;
        } else {
            let edit = SequenceEdit::Transition {
                transition: transition.clone(),
            };
            if response.changed {
                match sequence.edited(&edit) {
                    Ok(_) => {
                        if let Ok(request) = super::request(revision, document, edit.clone()) {
                            host.command(DesktopCommand::PreviewExtension(request));
                            self.editing = true;
                        }
                    }
                    Err(error) => {
                        ui.text_wrapped(error);
                    }
                }
            }
            if response.finished && self.editing {
                if sequence.edited(&edit).is_ok() {
                    super::submit(host, revision, document, edit);
                } else {
                    host.command(DesktopCommand::CancelPreviewEdit);
                    self.draft = Some(original.clone());
                }
                self.editing = false;
            }
        }
    }
}
pub(super) fn add(
    host: &mut dyn DesktopClient,
    document: DocumentId,
    sequence: &Sequence,
    outgoing: ObjectId,
    revision: Revision,
) {
    let existing = sequence.transitions.iter().find(|t| t.outgoing == outgoing);
    let id = existing.map(|t| t.id).unwrap_or_default();
    if existing.is_none() {
        host.command(DesktopCommand::Extension(
            fold_platform::packages::CommandRequest {
                id: "fold.app.add-transition".into(),
                base: revision,
                arguments: serde_json::to_vec(
                    &serde_json::json!({"document":document,"outgoing":outgoing,"id":id}),
                )
                .unwrap(),
            },
        ));
    }
    if host
        .snapshot()
        .and_then(|s| {
            s.state()
                .documents
                .get(&document)
                .and_then(|d| Sequence::from_document(d).ok())
        })
        .is_some_and(|s| s.transitions.iter().any(|t| t.id == id))
    {
        host.command(DesktopCommand::Select(fold_platform::desktop::Selection {
            document: Some(document),
            objects: vec![id],
        }));
    }
}
