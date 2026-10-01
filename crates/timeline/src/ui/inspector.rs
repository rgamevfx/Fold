use super::{current, frame_input, interaction, selected_clip, submit};
use crate::{ClipEdit, ImportArgs, SequenceEdit, SourceMedia, TrackKind, package};
use fold_foundation::ObjectId;
use fold_platform::desktop::DesktopCommand;
use fold_project::Revision;
use fold_ui::sdk::{ExtensionUi, Panel};

#[derive(Default)]
pub(super) struct Inspector {
    path: String,
    audio_only: bool,
    selection: Option<(ObjectId, Revision)>,
    start: i32,
    end: i32,
    level: f32,
    message: String,
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
        let active = current(&snapshot, &state.selection).ok();
        ui.text("MEDIA");
        ui.text_wrapped(
            "MP4 / H.264 SDR BT.709 or PCM16 WAVE. Select a track header to target an import.",
        );
        ui.input_text("Path", &mut self.path).build();
        ui.checkbox("Audio only (from video)", &mut self.audio_only);
        let append = ui.button("Append media");
        ui.same_line();
        let at_playhead = ui.button("Place at playhead");
        if append || at_playhead {
            let selected_track = active.as_ref().and_then(|(_, seq)| {
                state
                    .selection
                    .objects
                    .iter()
                    .find_map(|id| seq.tracks.iter().find(|t| t.id == *id))
            });
            let rate = active.as_ref().map(|(_, s)| s.rate).unwrap_or([24, 1]);
            let args = ImportArgs {
                document: active.as_ref().map(|(id, _)| *id),
                paths: vec![self.path.trim().into()],
                at: if at_playhead {
                    interaction::time(i64::from(state.frame), rate).ok()
                } else {
                    None
                },
                video_track: selected_track
                    .filter(|t| t.kind == TrackKind::Video)
                    .map(|t| t.id),
                audio_track: selected_track
                    .filter(|t| t.kind == TrackKind::Audio)
                    .map(|t| t.id),
                audio_only: self.audio_only,
            };
            match package::request(&snapshot, package::IMPORT, &args) {
                Ok(request) => {
                    self.message.clear();
                    host.command(DesktopCommand::Extension(request));
                }
                Err(error) => self.message = error,
            }
        }
        ui.text(if state.busy {
            "Import/export worker busy"
        } else {
            "Worker idle"
        });
        ui.text_wrapped(&state.status);
        if !self.message.is_empty() {
            ui.text_wrapped(&self.message);
        }
        ui.separator();
        ui.text("SELECTION");
        let Some((document, sequence)) = active else {
            ui.text_wrapped("Import media to create a multi-track sequence.");
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
                ui.text(format!(
                    "Nested video: {:?} / {}",
                    source.document, source.output
                ));
                if let Ok(time) = interaction::time(i64::from(state.frame), sequence.rate)
                    && let Ok(Some(local)) = clip.source_time(time)
                {
                    ui.text(format!("Sequence {time:?} → source {local:?}"));
                }
                if ui.button("Open Source in Compositor")
                    && let Ok(time) = interaction::time(i64::from(state.frame), sequence.rate)
                    && let Ok(Some(local)) = clip.source_time(time)
                {
                    host.command(DesktopCommand::Navigate(
                        fold_platform::desktop::ViewLocation {
                            document: source.document,
                            time: local,
                            label: "Composite".into(),
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
