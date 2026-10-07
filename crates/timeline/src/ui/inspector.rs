use super::{current, selected_clip, submit};
use crate::{ClipEdit, SequenceEdit, SourceMedia, TrackKind, package};
use fold_foundation::ObjectId;
use fold_platform::desktop::DesktopCommand;
use fold_project::Revision;
use fold_ui::sdk::{ExtensionUi, Panel};

#[derive(Default)]
pub(super) struct Inspector {
    selection: Option<(ObjectId, Revision)>,
    level: f32,
    transform: crate::Transform,
    effects: Vec<fold_platform::effects::Instance>,
    editing: bool,
    transitions: super::transitions::Editor,
}
impl Panel for Inspector {
    fn cancel_interaction(&mut self, host: &mut dyn fold_platform::desktop::DesktopClient) {
        if self.editing {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.editing = false;
            self.selection = None;
        }
        self.transitions.cancel(host);
    }
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
        let desired = active
            .as_ref()
            .and_then(|(_, sequence)| selected_clip(sequence, &state.selection))
            .map(|id| (id, snapshot.revision()));
        if self.editing && self.selection != desired {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.editing = false;
            self.selection = None;
        }
        let Some((document, sequence)) = active else {
            ui.text_disabled("No sequence selected.");
            return;
        };
        if let Some(id) = state
            .selection
            .objects
            .iter()
            .find(|id| sequence.transitions.iter().any(|t| t.id == **id))
        {
            self.transitions
                .draw(ui, host, document, &sequence, *id, snapshot.revision());
            return;
        }
        self.transitions.cancel(host);
        if let Some(track) = state
            .selection
            .objects
            .iter()
            .find_map(|id| sequence.tracks.iter().find(|t| t.id == *id))
        {
            ui.text(format!("{} — {:?}", track.name, track.kind));

            if track.kind == TrackKind::Video {
                let _disabled = ui.begin_disabled_with_cond(track.locked);
                if ui.button("Add adjustment") {
                    let start = state
                        .navigation
                        .last()
                        .filter(|v| v.document == document)
                        .map(|v| v.time)
                        .unwrap_or(fold_foundation::Time::ZERO);
                    submit(
                        host,
                        snapshot.revision(),
                        document,
                        SequenceEdit::AddAdjustment {
                            id: ObjectId::new(),
                            track: track.id,
                            start,
                            duration: fold_foundation::Time::new(1, 1).unwrap(),
                        },
                    );
                }
            }
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
            ui.text_disabled("Select a clip or transition.");
            return;
        };
        let clip = sequence.clips.iter().find(|c| c.id == id).unwrap();
        let track = sequence.track(clip.track).unwrap();
        if self.selection != Some((id, snapshot.revision())) {
            if self.editing {
                host.command(DesktopCommand::CancelPreviewEdit);
                self.editing = false;
            }
            self.effects = clip.effects.clone();
            self.selection = Some((id, snapshot.revision()));
            self.level = clip.level;
            self.transform = clip.transform().unwrap_or_default();
        }
        let name = match &clip.info {
            SourceMedia::Document { source, .. } => fold_platform::browser::item_name(
                &snapshot,
                fold_project::ItemId::Document(source.document),
                "Composition",
            ),
            SourceMedia::Adjustment => "Adjustment".into(),
            _ => snapshot
                .state()
                .assets
                .get(&clip.asset)
                .map(|a| {
                    std::path::Path::new(&a.location)
                        .file_name()
                        .unwrap_or_default()
                        .to_string_lossy()
                        .into_owned()
                })
                .unwrap_or_else(|| "Missing source".into()),
        };
        ui.text_wrapped(&name);
        if let SourceMedia::Document { source, .. } = &clip.info {
            if ui.small_button("Open Network") {
                host.command(DesktopCommand::CancelPreviewEdit);
                self.editing = false;
                host.command(DesktopCommand::Navigate(
                    fold_platform::desktop::ViewLocation {
                        document: source.document,
                        time: state
                            .navigation
                            .last()
                            .filter(|v| v.document == document)
                            .and_then(|v| clip.source_time(v.time).ok().flatten())
                            .unwrap_or(clip.source_start),
                        label: name,
                    },
                ));
            }
        }
        let _disabled = ui.begin_disabled_with_cond(track.locked);
        if track.kind == TrackKind::Video && !matches!(clip.info, SourceMedia::Adjustment) {
            let mut changed = false;
            let mut finished = false;
            changed |= fold_ui::sdk::imgui::Drag::new("Position")
                .display_format(fold_ui::sdk::imgui::NumericFormat::new("%.1f").unwrap())
                .speed(1.)
                .build_array(ui, &mut self.transform.position);
            finished |= ui.is_item_deactivated_after_edit();
            changed |= fold_ui::sdk::imgui::Drag::new("Scale")
                .display_format(fold_ui::sdk::imgui::NumericFormat::new("%.2f").unwrap())
                .speed(0.01)
                .build_array(ui, &mut self.transform.scale);
            finished |= ui.is_item_deactivated_after_edit();
            changed |= fold_ui::sdk::imgui::Drag::new("Rotation")
                .display_format(fold_ui::sdk::imgui::NumericFormat::new("%.1f").unwrap())
                .speed(0.5)
                .build(ui, &mut self.transform.rotation);
            finished |= ui.is_item_deactivated_after_edit();
            let edit = SequenceEdit::Transform {
                id,
                transform: self.transform.clone(),
            };
            if changed {
                if let Ok(request) = super::request(snapshot.revision(), document, edit.clone()) {
                    host.command(DesktopCommand::PreviewExtension(request));
                    self.editing = true;
                }
            }
            if finished {
                submit(host, snapshot.revision(), document, edit);
                self.editing = false;
            }
        }
        if fold_ui::sdk::imgui::Drag::new(if track.kind == TrackKind::Video {
            "Opacity"
        } else {
            "Gain"
        })
        .speed(0.01)
        .range(0., 1.)
        .build(ui, &mut self.level)
        {
            if let Ok(request) = super::request(
                snapshot.revision(),
                document,
                SequenceEdit::Clip {
                    id,
                    linked: false,
                    edit: ClipEdit::Level(self.level),
                },
            ) {
                host.command(DesktopCommand::PreviewExtension(request));
                self.editing = true;
            }
        }
        if ui.is_item_deactivated_after_edit() {
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
            self.editing = false;
        }
        if track.kind == TrackKind::Video {
            let _disabled = ui.begin_disabled_with_cond(track.locked);
            let time = state
                .navigation
                .last()
                .filter(|v| v.document == document)
                .map(|v| v.time)
                .unwrap_or(clip.start);
            let mut edit = fold_ui::sdk::EditResponse::default();
            let navigate =
                fold_ui::sdk::effects::stack(ui, host, &mut self.effects, time, &mut edit);
            if self.editing
                && (edit.cancelled || ui.is_key_pressed(fold_ui::sdk::imgui::Key::Escape))
            {
                host.command(DesktopCommand::CancelPreviewEdit);
                self.effects = clip.effects.clone();
                self.selection = None;
                self.editing = false;
            } else {
                if edit.changed {
                    if let Ok(request) = super::request(
                        snapshot.revision(),
                        document,
                        SequenceEdit::Effects {
                            id,
                            effects: self.effects.clone(),
                        },
                    ) {
                        host.command(DesktopCommand::PreviewExtension(request));
                        self.editing = true;
                    }
                    self.editing = true;
                }
                if self.editing && edit.finished {
                    submit(
                        host,
                        snapshot.revision(),
                        document,
                        SequenceEdit::Effects {
                            id,
                            effects: self.effects.clone(),
                        },
                    );
                    self.editing = false;
                }
            }
            if let Some(source) = navigate {
                host.command(DesktopCommand::CancelPreviewEdit);
                self.editing = false;
                host.command(DesktopCommand::Navigate(
                    fold_platform::desktop::ViewLocation {
                        document: source.document,
                        time,
                        label: "Effect".into(),
                    },
                ));
            }
        }
        if self.editing && ui.is_key_pressed(fold_ui::sdk::imgui::Key::Escape) {
            host.command(DesktopCommand::CancelPreviewEdit);
            self.selection = None;
            self.editing = false;
        }
    }
}
