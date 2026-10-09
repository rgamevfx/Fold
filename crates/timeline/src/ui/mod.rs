//! Timeline-owned trusted panel contributions. The shared shell only docks and
//! invokes them. Selection is host UI state; drag/zoom state stays panel-local.
mod canvas;
mod controls;
mod inspector;
mod interaction;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod tests;
use crate::{Sequence, SequenceEdit, TrackKind, package};
use canvas::{Action, Canvas, CanvasModel};
use fold_foundation::{DocumentId, ObjectId};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, Selection},
    packages::CommandRequest,
};
use fold_project::{Revision, Snapshot};
use fold_ui::sdk::{ExtensionUi, Panel, PanelRegistry, toolbar::ToolbarIcon};
use std::collections::BTreeMap;

pub fn register(registry: &mut PanelRegistry) -> Result<(), String> {
    registry.register(crate::PACKAGE, TimelinePanel::default())?;
    registry.register_inspector(crate::PACKAGE, inspector::Inspector::default())
}
fn current(
    snapshot: &Snapshot,
    selection: &Selection,
    explicit: bool,
) -> Result<(DocumentId, Sequence), String> {
    if let Some(id) = selection.document
        && let Some(document) = snapshot.state().documents.get(&id)
        && document.type_id == crate::SEQUENCE
    {
        return Ok((id, Sequence::from_document(document)?));
    }
    if explicit {
        return Err("Select a sequence for this editor".into());
    }
    let (reference, sequence) = crate::active(snapshot)?;
    Ok((reference.document, sequence))
}
fn request(
    base: Revision,
    document: DocumentId,
    edit: SequenceEdit,
) -> Result<CommandRequest, String> {
    Ok(CommandRequest {
        id: package::EDIT.into(),
        base,
        arguments: serde_json::to_vec(&package::EditArgs { document, edit })
            .map_err(|e| e.to_string())?,
    })
}
fn submit(host: &mut dyn DesktopClient, base: Revision, document: DocumentId, edit: SequenceEdit) {
    // These owned scalar/enum arguments have no fallible custom serializers.
    if let Some(snapshot) = host.snapshot()
        && let Err(error) = crate::edit_sequence(&snapshot, document, &edit)
    {
        host.command(DesktopCommand::Notify(error));
        return;
    }
    if let Ok(request) = request(base, document, edit) {
        host.command(DesktopCommand::Extension(request));
    }
}
#[derive(Default)]
struct TimelinePanel {
    canvas: Canvas,
    thumbnails: fold_ui::sdk::media_thumbnails::Thumbnails,
    document: Option<DocumentId>,
    cached: Option<(
        Revision,
        DocumentId,
        Sequence,
        BTreeMap<fold_foundation::AssetId, String>,
    )>,
}
impl Panel for TimelinePanel {
    fn cancel_interaction(&mut self, host: &mut dyn DesktopClient) {
        if self.canvas.transition_drag.take().is_some() {
            host.command(DesktopCommand::CancelPreviewEdit);
        }
    }
    fn external_drag(&mut self, position: Option<[f32; 2]>) {
        self.canvas.external_pointer = position;
    }
    fn files_dropped(
        &mut self,
        position: [f32; 2],
        paths: &[std::path::PathBuf],
        host: &mut dyn DesktopClient,
    ) -> bool {
        let Some((_, document, sequence, _)) = &self.cached else {
            return false;
        };
        let Some(request) =
            self.canvas
                .native_placement(*document, sequence, position, host.state().frame)
        else {
            return false;
        };
        fold_ui::sdk::project_drop::import_and_place(host, paths, request);
        true
    }

    fn prepare_frame(&mut self, context: &mut fold_ui::sdk::imgui::Context) {
        self.thumbnails.prepare(context);
    }
    fn new_instance(&self) -> Option<fold_ui::sdk::EditorPanels> {
        Some(fold_ui::sdk::EditorPanels {
            editor: Box::new(Self::default()),
            inspector: Box::new(inspector::Inspector::default()),
        })
    }
    fn document_type(&self) -> Option<&'static str> {
        Some(crate::SEQUENCE)
    }
    fn id(&self) -> &'static str {
        package::PANEL
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        let ExtensionUi { ui, host } = context;
        let Some(snapshot) = host.snapshot() else {
            ui.text("No project is open.");
            return;
        };
        let state = host.state().clone();
        if self.cached.as_ref().is_none_or(|(revision, id, _, _)| {
            *revision != snapshot.revision() || state.selection.document != Some(*id)
        }) {
            self.cached = current(&snapshot, &state.selection, host.explicit_target())
                .ok()
                .map(|(id, sequence)| {
                    let labels = snapshot
                        .state()
                        .assets
                        .iter()
                        .map(|(id, asset)| {
                            (
                                *id,
                                std::path::Path::new(&asset.location)
                                    .file_name()
                                    .unwrap_or_default()
                                    .to_string_lossy()
                                    .into_owned(),
                            )
                        })
                        .collect();
                    (snapshot.revision(), id, sequence, labels)
                });
        }
        let Some((revision, document, sequence, labels)) = &self.cached else {
            ui.text_disabled("Create or open a sequence in Project.");
            return;
        };
        if self.document != Some(*document) {
            self.canvas = Canvas::default();
            self.document = Some(*document);
        }
        if let Some(edit) = controls::toolbar(
            ui,
            &mut self.canvas,
            sequence,
            &state.selection.objects,
            state.frame,
        ) {
            submit(host, *revision, *document, edit);
        }
        self.canvas.visuals.clear();
        for clip in &sequence.clips {
            let track = sequence.track(clip.track).unwrap();
            let (name, icon, kind) = match &clip.info {
                crate::SourceMedia::Adjustment => {
                    ("Adjustment".into(), ToolbarIcon::Composition, "Adjustment")
                }
                crate::SourceMedia::Document { source, .. } => {
                    let kind = snapshot
                        .state()
                        .documents
                        .get(&source.document)
                        .map(|d| d.type_id.as_str())
                        .unwrap_or("");
                    let graph = kind == "fold.compositor.composite";
                    (
                        fold_platform::browser::item_name(
                            &snapshot,
                            fold_project::ItemId::Document(source.document),
                            if graph { "Node graph" } else { "Composition" },
                        ),
                        if graph {
                            ToolbarIcon::Arrange
                        } else {
                            ToolbarIcon::Composition
                        },
                        if kind.is_empty() {
                            "Missing source"
                        } else if graph {
                            "Node graph"
                        } else {
                            "Composition"
                        },
                    )
                }
                _ => (
                    fold_platform::browser::item_name(
                        &snapshot,
                        fold_project::ItemId::Asset(clip.asset),
                        labels
                            .get(&clip.asset)
                            .map(String::as_str)
                            .unwrap_or("Missing media"),
                    ),
                    if track.kind == TrackKind::Audio {
                        ToolbarIcon::Audio
                    } else {
                        ToolbarIcon::View
                    },
                    if track.kind == TrackKind::Audio {
                        "Audio"
                    } else {
                        "Video"
                    },
                ),
            };
            let visible = interaction::frame(clip.end().unwrap(), sequence.rate)
                >= self.canvas.view.first
                && interaction::frame(clip.start, sequence.rate)
                    <= self.canvas.view.first
                        + f64::from(ui.content_region_avail()[0])
                            / self.canvas.view.pixels_per_frame;
            let thumbnail = if self.canvas.media_previews && visible {
                snapshot.state().assets.get(&clip.asset).and_then(|asset| {
                    self.thumbnails.get_range(
                        asset,
                        Some((
                            clip.source_start,
                            clip.duration,
                            track.kind == TrackKind::Audio,
                        )),
                    )
                })
            } else {
                None
            };
            let (texture, error) = match thumbnail {
                Some(Ok(texture)) => (Some(texture), None),
                Some(Err(error)) => (None, Some(error)),
                None => (
                    None,
                    if kind == "Missing source" {
                        Some("Source document is unavailable".into())
                    } else {
                        None
                    },
                ),
            };
            self.canvas.visuals.insert(
                clip.id,
                canvas::ClipVisual {
                    name,
                    icon,
                    kind,
                    texture,
                    error,
                },
            );
        }
        self.canvas.cache = host
            .cache_reference()
            .filter(|key| key.target.is_some_and(|(id, _)| id == *document))
            .map(|key| fold_ui::sdk::cache_indicator::frames(host.cached_previews(), &key))
            .unwrap_or_default();
        ui.separator();
        let placement_mode = match self.canvas.mode {
            crate::EditMode::Move => fold_platform::browser::PlacementMode::Place,
            crate::EditMode::Insert => fold_platform::browser::PlacementMode::Insert,
            crate::EditMode::Overwrite => fold_platform::browser::PlacementMode::Overwrite,
        };
        let placement = |entry: fold_platform::browser::Entry, track, at, new_track, audio_only| {
            let source = entry.source(|id| host.default_output(id))?;
            let output = match &source {
                fold_platform::browser::PlacementSource::Output(source) => Some(
                    host.outputs(source.document)
                        .into_iter()
                        .find(|output| output.reference == *source)
                        .ok_or("Missing document output")?
                        .info?,
                ),
                _ => None,
            };
            let request = fold_platform::browser::Placement {
                graph: None,
                source,
                target: *document,
                track,
                at,
                source_start: fold_foundation::Time::ZERO,
                duration: None,
                audio_only,
                new_track,
                mode: placement_mode,
            };
            crate::placement::place(&snapshot, &request, output)
                .and_then(|document| Sequence::from_document(&document))
        };
        for action in self.canvas.draw(
            ui,
            CanvasModel {
                sequence,
                revision: *revision,
                playhead: state.frame,
                selected: &state.selection.objects,
                labels,
                placement: Some(&placement),
            },
        ) {
            match action {
                Action::Place {
                    entry,
                    track,
                    at,
                    new_track,
                    audio_only,
                } => match entry.source(|id| host.default_output(id)) {
                    Ok(source) => host.command(DesktopCommand::Browser(
                        fold_platform::browser::BrowserCommand::Place(
                            fold_platform::browser::Placement {
                                graph: None,
                                source,
                                target: *document,
                                track,
                                at,
                                source_start: fold_foundation::Time::ZERO,
                                duration: None,
                                audio_only,
                                new_track,
                                mode: placement_mode,
                            },
                        ),
                    )),
                    Err(error) => host.command(DesktopCommand::Notify(error)),
                },
                Action::AddTransition(outgoing) => {
                    transitions::add(host, *document, sequence, outgoing, *revision)
                }
                Action::Open(id) => {
                    if let Some(clip) = sequence.clips.iter().find(|c| c.id == id)
                        && let crate::SourceMedia::Document { source, .. } = &clip.info
                    {
                        let at = interaction::time(i64::from(state.frame), sequence.rate)
                            .unwrap_or(clip.start);
                        let time = clip
                            .source_time(at)
                            .ok()
                            .flatten()
                            .unwrap_or(clip.source_start);
                        host.command(DesktopCommand::Navigate(
                            fold_platform::desktop::ViewLocation {
                                document: source.document,
                                time,
                                label: self
                                    .canvas
                                    .visuals
                                    .get(&id)
                                    .map(|v| v.name.clone())
                                    .unwrap_or_default(),
                            },
                        ));
                    }
                }
                Action::Select(objects) => host.command(DesktopCommand::Select(Selection {
                    document: Some(*document),
                    objects,
                })),
                Action::Seek(frame) => host.command(DesktopCommand::Seek(frame)),
                Action::Pause => host.command(DesktopCommand::Pause),
                Action::Preview { base, edit } => {
                    if let Ok(request) = request(base, *document, edit) {
                        host.command(DesktopCommand::PreviewExtension(request));
                    }
                }
                Action::CancelPreview => host.command(DesktopCommand::CancelPreviewEdit),
                Action::Commit { base, edit } => submit(host, base, *document, edit),
            }
        }
    }
}
fn selected_clip(sequence: &Sequence, selection: &Selection) -> Option<ObjectId> {
    selection
        .objects
        .iter()
        .find(|id| sequence.clips.iter().any(|c| c.id == **id))
        .copied()
}

mod transitions;
