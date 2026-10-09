//! Desktop command execution. Staging uses immutable snapshots; publication
//! stays on Session so provider validation and project history remain authoritative.
use super::Session;
use crate::output as workflow;
use fold_media::Cancel;
use fold_platform::desktop::{DesktopCommand, DesktopState};
use fold_project::{EditBatch, Mutation};
pub(super) enum Outcome {
    Handled,
    Edit(Result<(), String>),
}
impl Session {
    pub(super) fn execute_command(&mut self, command: DesktopCommand) -> Outcome {
        let result = match command {
            DesktopCommand::AutoKey(enabled) => {
                self.state.auto_key = enabled;
                return Outcome::Handled;
            }
            DesktopCommand::NewProject => {
                self.replace_project(crate::color::new_project(32));
                self.workspace_project = None;
                self.workspace_epoch += 1;
                self.workspace_restore = false;
                let color_choices = std::mem::take(&mut self.state.color_choices);
                self.state = DesktopState {
                    color_choices,
                    ..Default::default()
                };
                self.refresh_job_state();
                self.refresh();
                return Outcome::Handled;
            }
            DesktopCommand::ViewerTransport { viewer, transport } => {
                self.configure_viewer(viewer, transport);
                return Outcome::Handled;
            }
            DesktopCommand::MonitorViewer(viewer) => {
                self.monitor_viewer(viewer);
                return Outcome::Handled;
            }
            DesktopCommand::CloseViewer(viewer) => {
                if self.viewers.monitor == Some(viewer) {
                    self.monitor_viewer(None);
                }
                self.viewers.clocks.remove(&viewer);
                return Outcome::Handled;
            }
            DesktopCommand::Notify(message) => {
                self.state.status = message;
                return Outcome::Handled;
            }
            DesktopCommand::Browser(command) => {
                use fold_platform::browser::BrowserCommand;
                if matches!(
                    command,
                    BrowserCommand::Import { .. }
                        | BrowserCommand::ImportAndPlace { .. }
                        | BrowserCommand::ChooseImport(_)
                        | BrowserCommand::Relink(_)
                ) {
                    self.background(DesktopCommand::Browser(command));
                    return Outcome::Handled;
                }
                let creating = matches!(command, BrowserCommand::Create { .. });
                crate::browser::edit(&self.project.snapshot(), command).and_then(|batch| {
                    let location = if creating {
                        batch.mutations.iter().find_map(|m| match m {
                            Mutation::PutItem(item) => match item.id {
                                fold_project::ItemId::Document(document) => {
                                    Some(fold_platform::desktop::ViewLocation {
                                        document,
                                        time: fold_foundation::Time::ZERO,
                                        label: item.name.clone(),
                                    })
                                }
                                _ => None,
                            },
                            _ => None,
                        })
                    } else {
                        None
                    };
                    self.project.commit(batch).map_err(|e| e.to_string())?;
                    if let Some(location) = location {
                        self.navigate(location);
                    }
                    Ok(())
                })
            }
            DesktopCommand::PreviewExtension(request) => {
                self.preview_edit(request);
                return Outcome::Handled;
            }
            DesktopCommand::CancelPreviewEdit => {
                self.overlay = None;
                self.refresh();
                return Outcome::Handled;
            }
            DesktopCommand::OpenLayerGraph { location, objects } => {
                self.navigate(location.clone());
                self.state.selection = fold_platform::desktop::Selection {
                    document: Some(location.document),
                    objects,
                };
                return Outcome::Handled;
            }
            DesktopCommand::Navigate(location) => {
                self.navigate(location);
                return Outcome::Handled;
            }
            DesktopCommand::NavigateBack => {
                self.back();
                return Outcome::Handled;
            }
            DesktopCommand::ActivateWorkspace(kind) => {
                self.activate_workspace(&kind);
                return Outcome::Handled;
            }
            DesktopCommand::Transport(action) => {
                self.transport(action);
                return Outcome::Handled;
            }
            DesktopCommand::Play => {
                self.start_playback();
                return Outcome::Handled;
            }
            DesktopCommand::Pause => {
                self.stop_playback();
                return Outcome::Handled;
            }
            DesktopCommand::Seek(frame) => {
                self.seek(frame);
                if self.state.playing {
                    self.start_playback();
                }
                return Outcome::Handled;
            }
            DesktopCommand::Select(selection) => {
                self.state.selection = selection;
                return Outcome::Handled;
            }
            DesktopCommand::UnlockControls {
                document,
                object,
                targets,
                base,
            } => crate::packages::builtins()
                .unlock_for_artist(&self.project.snapshot(), base, document, object, &targets)
                .and_then(|batch| {
                    self.project
                        .commit_named(batch, "Unlock controls".into())
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                }),
            DesktopCommand::Extension(request) => {
                let registry = crate::packages::builtins();
                match registry.command(&request.id) {
                    Ok(command)
                        if command.execution == fold_platform::packages::Execution::Worker =>
                    {
                        self.background(DesktopCommand::Extension(request));
                        return Outcome::Handled;
                    }
                    Ok(_) => registry
                        .stage(&self.project.snapshot(), &request, &Cancel::default())
                        .and_then(|batch| {
                            self.project
                                .commit(batch)
                                .map(|_| ())
                                .map_err(|e| e.to_string())
                        }),
                    Err(error) => Err(error),
                }
            }
            DesktopCommand::CancelExtension(request) => {
                self.jobs.cancel_extension(&request);
                return Outcome::Handled;
            }
            DesktopCommand::Cancel => {
                self.jobs.cancel();
                self.state.status = "Cancellation requested".into();
                return Outcome::Handled;
            }
            DesktopCommand::OpenSettings(target) => {
                self.settings_request = Some(target);
                return Outcome::Handled;
            }
            DesktopCommand::SetOutputColor {
                document,
                transform,
            } => {
                if !self.state.color_choices.outputs.iter().any(|choice| {
                    choice.display == transform.display
                        && choice.view == transform.view
                        && choice.look == transform.look
                }) {
                    self.state.status = "Unavailable output color transform".into();
                    return Outcome::Handled;
                }
                crate::color::set_output(&self.project.snapshot(), document, transform).and_then(
                    |batch| {
                        self.project
                            .commit(batch)
                            .map(|_| ())
                            .map_err(|e| e.to_string())
                    },
                )
            }
            DesktopCommand::SetInputColor { asset, space } => (|| {
                if !self.state.color_choices.inputs.contains(&space) {
                    return Err("Unavailable input color space".into());
                }
                let snapshot = self.project.snapshot();
                if fold_platform::color::project(&snapshot)?.is_none() {
                    return Err("Legacy project retains its original input interpretation".into());
                }
                let mut asset = snapshot
                    .state()
                    .assets
                    .get(&asset)
                    .ok_or("Missing input asset")?
                    .as_ref()
                    .clone();
                asset.extensions.insert(
                    fold_platform::color::INPUT_KEY.into(),
                    serde_json::Value::String(space),
                );
                self.project
                    .commit(EditBatch {
                        base: snapshot.revision(),
                        mutations: vec![Mutation::PutAsset(asset)],
                    })
                    .map(|_| ())
                    .map_err(|e| e.to_string())
            })(),
            DesktopCommand::SetOutput(document) => {
                workflow::select_output(&self.project.snapshot(), document).and_then(|batch| {
                    self.project
                        .commit(batch)
                        .map(|_| ())
                        .map_err(|e| e.to_string())
                })
            }
            DesktopCommand::Undo => self.project.undo().map(|_| ()).map_err(|e| e.to_string()),
            DesktopCommand::Redo => self.project.redo().map(|_| ()).map_err(|e| e.to_string()),
            other => {
                self.background(other);
                return Outcome::Handled;
            }
        };
        Outcome::Edit(result)
    }
}
