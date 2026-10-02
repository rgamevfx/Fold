use super::{current, frame_input, interaction, selected_clip, submit};
use crate::{ClipEdit, SequenceEdit, SourceMedia, TrackKind, package};
use fold_foundation::ObjectId;
use fold_platform::desktop::DesktopCommand;
use fold_project::Revision;
use fold_ui::sdk::{ExtensionUi, Panel};

#[derive(Default)]
pub(super) struct Inspector {
    selection: Option<(ObjectId, Revision)>,
    start: i32,
    end: i32,
    level: f32,
}
impl Panel for Inspector {
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::SEQUENCE)
    }
    fn id(&self) -> &'static str {
        package::INSPECTOR
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let Some(snapshot) = host.snapshot() else {
            ui.text("No project is open.");
            return;
        };
        let state = host.state().clone();
        let active = current(&snapshot, &state.selection, host.explicit_target()).ok();
        let Some((document, sequence)) = active else {
            ui.text_disabled("No sequence selected.");
            return;
        };
        if let Some(track) = state
            .selection
            .objects
            .iter()
            .find_map(|id| sequence.tracks.iter().find(|t| t.id == *id))
        {
            ui.text(format!("{} — {:?}", track.name, track.kind));
            ui.text_wrapped("Track header controls: E enables video; M mutes audio; S solos audio; L locks editing.");
            if ui.button("Remove empty track") {
                submit(
                    host,
                    snapshot.revision(),
                    document,
                    SequenceEdit::RemoveTrack { id: track.id },
                );
            }
            return;
        }
        let Some(id) = selected_clip(&sequence, &state.selection) else {
            ui.text_wrapped(
                "Select a clip on the timeline. Drag its body to move, or an edge to trim.",
            );
            return;
        };
        let clip = sequence.clips.iter().find(|c| c.id == id).unwrap();
        let track = sequence.track(clip.track).unwrap();
        if self.selection != Some((id, snapshot.revision())) {
            self.selection = Some((id, snapshot.revision()));
            self.start = interaction::frame(clip.start, sequence.rate).round() as i32;
            self.end = interaction::frame(clip.end().unwrap(), sequence.rate).round() as i32;
            self.level = clip.level;
        }
        if !matches!(clip.info, SourceMedia::Document { .. }) {
            let location = snapshot
                .state()
                .assets
                .get(&clip.asset)
                .map(|a| a.location.as_str())
                .unwrap_or("Unavailable asset");
            ui.text_wrapped(location);
        }
        ui.text(format!(
            "{}  |  {}",
            track.name,
            if clip.link.is_some() {
                "Linked A/V"
            } else {
                "Independent"
            }
        ));
        match &clip.info {
            SourceMedia::Document { source, .. } => {
                ui.text("Nested source");
                let time = state
                    .navigation
                    .last()
                    .filter(|v| v.document == document)
                    .map(|v| v.time)
                    .or_else(|| interaction::time(i64::from(state.frame), sequence.rate).ok());
                if let Some(time) = time
                    && let Ok(Some(local)) = clip.source_time(time)
                {
                    ui.text(format!(
                        "Sequence {}/{} s → source {}/{} s",
                        time.numerator(),
                        time.denominator(),
                        local.numerator(),
                        local.denominator()
                    ));
                }
                if ui.button("Open Source")
                    && let Some(time) = time
                    && let Ok(Some(local)) = clip.source_time(time)
                {
                    host.command(DesktopCommand::Navigate(
                        fold_platform::desktop::ViewLocation {
                            document: source.document,
                            time: local,
                            label: "Source".into(),
                        },
                    ));
                }
            }
            SourceMedia::Video(info) => ui.text(format!(
                "{}×{} • {}/{} fps",
                info.width, info.height, info.rate[0], info.rate[1]
            )),
            SourceMedia::Audio(info) => ui.text(format!(
                "PCM16 • {} Hz • {} channel(s)",
                info.rate, info.channels
            )),
        }
        ui.text(format!(
            "Source in: {}/{} s",
            clip.source_start.numerator(),
            clip.source_start.denominator()
        ));
        ui.separator();
        ui.text("Precise edit (sequence frames)");
        frame_input(ui, "Start", &mut self.start);
        frame_input(ui, "End", &mut self.end);
        if ui.button("Apply trim")
            && self.start >= 0
            && self.end > self.start
            && let (Ok(start), Ok(end)) = (
                interaction::time(i64::from(self.start), sequence.rate),
                interaction::time(i64::from(self.end), sequence.rate),
            )
        {
            submit(
                host,
                snapshot.revision(),
                document,
                SequenceEdit::Clip {
                    id,
                    linked: true,
                    edit: ClipEdit::Trim { start, end },
                },
            );
        }
        ui.separator();
        ui.input_float(
            if track.kind == TrackKind::Video {
                "Opacity"
            } else {
                "Gain"
            },
            &mut self.level,
        );
        if ui.button("Apply level") {
            submit(
                host,
                snapshot.revision(),
                document,
                SequenceEdit::Clip {
                    id,
                    linked: false,
                    edit: ClipEdit::Level(self.level),
                },
            );
        }
        ui.text_wrapped("Timeline edits follow linked A/V by default. Turn off Linked or hold Alt when beginning a drag to edit independently and dissolve the link.");
    }
}
