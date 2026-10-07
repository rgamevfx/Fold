use crate::{Composition, package};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand, Selection, ViewLocation},
    packages::CommandRequest,
};
use fold_project::Revision;
use std::{cell::RefCell, rc::Rc};
pub(super) type Shared = Rc<RefCell<State>>;
#[derive(Default)]
pub(super) struct State {
    pub document: Option<DocumentId>,
    pub composition: Option<Composition>,
    original: Option<Composition>,
    pub base: Revision,
    pub generation: u64,
    pub selected: Vec<ObjectId>,
    pub editing: bool,
    pub key_ids: super::channels::KeyIds,
    pub graph_channels:
        std::collections::BTreeMap<ObjectId, Vec<fold_platform::animation::Channel>>,
    original_channels: std::collections::BTreeMap<ObjectId, Vec<fold_platform::animation::Channel>>,
    edit_time: Option<Time>,
    pub error: String,
    network_layer: Option<(DocumentId, ObjectId, DocumentId)>,
}
impl State {
    pub fn sync(&mut self, host: &mut dyn DesktopClient) {
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        let desired = host.state().selection.document.filter(|id| {
            snapshot
                .state()
                .documents
                .get(id)
                .is_some_and(|d| d.type_id == crate::COMPOSITION)
        });
        if desired != self.document || snapshot.revision() != self.base {
            if self.editing {
                host.command(DesktopCommand::CancelPreviewEdit);
            }
            self.document = desired;
            self.base = snapshot.revision();
            self.editing = false;
            self.composition = desired
                .and_then(|id| Composition::from_document(&snapshot.state().documents[&id]).ok());
            self.original = self.composition.clone();
            self.graph_channels.clear();
            if let Some(composition) = &self.composition {
                for layer in &composition.layers {
                    let Some(fold_platform::image_source::Source::Output(source)) = &layer.source
                    else {
                        continue;
                    };
                    // Nested compositions own their own editor and clock.
                    if snapshot
                        .state()
                        .documents
                        .get(&source.document)
                        .is_none_or(|d| d.type_id == crate::COMPOSITION)
                    {
                        continue;
                    }
                    match host.animation_channels(source.document, layer.graph_object()) {
                        Ok(channels) => {
                            self.graph_channels.insert(layer.id, channels);
                        }
                        Err(error) => self.error = error,
                    }
                }
            }
            self.original_channels = self.graph_channels.clone();
            self.generation += 1;
        }
        if let Some(composition) = &self.composition {
            self.selected = host
                .state()
                .selection
                .objects
                .iter()
                .copied()
                .filter(|id| composition.layer(*id).is_ok())
                .collect();
        }
    }
    fn request(&self) -> Result<CommandRequest, String> {
        Ok(CommandRequest {
            id: package::EDIT.into(),
            base: self.base,
            arguments: serde_json::to_vec(&package::EditArgs {
                document: self.document.ok_or("No composition")?,
                composition: self.composition.clone().ok_or("No composition")?,
                graph_animation: self
                    .graph_channels
                    .iter()
                    .filter_map(|(layer, channels)| {
                        if self.composition.as_ref()?.locked(*layer) {
                            return None;
                        }
                        let original = self.original_channels.get(layer)?;
                        let changed: Vec<_> = channels
                            .iter()
                            .filter(|c| {
                                original
                                    .iter()
                                    .find(|o| o.target == c.target)
                                    .is_none_or(|o| o.curve != c.curve)
                            })
                            .cloned()
                            .collect();
                        (!changed.is_empty()).then_some(package::GraphAnimationEdit {
                            layer: *layer,
                            channels: changed,
                            time: self.edit_time.unwrap_or(Time::ZERO),
                        })
                    })
                    .collect(),
            })
            .map_err(|e| e.to_string())?,
        })
    }
    pub fn cancel(&mut self, host: &mut dyn DesktopClient) {
        host.command(DesktopCommand::CancelPreviewEdit);
        self.composition = self.original.clone();
        self.graph_channels = self.original_channels.clone();
        self.editing = false;
        self.generation += 1;
    }
    pub fn preview(&mut self, host: &mut dyn DesktopClient) {
        self.edit_time = Some(time(host));
        match self
            .composition
            .as_ref()
            .ok_or("No composition".into())
            .and_then(|c| c.validate())
            .and_then(|_| self.request())
        {
            Ok(request) => {
                self.editing = true;
                host.command(DesktopCommand::PreviewExtension(request));
            }
            Err(error) => {
                host.command(DesktopCommand::CancelPreviewEdit);
                self.error = error;
            }
        }
    }
    pub fn commit(&mut self, host: &mut dyn DesktopClient) {
        if self.original == self.composition && self.graph_channels == self.original_channels {
            self.cancel(host);
            return;
        }
        match self.request() {
            Ok(request) => {
                host.command(DesktopCommand::Extension(request));
                if host.snapshot().is_some_and(|s| s.revision() == self.base) {
                    let error = host.state().status.clone();
                    self.cancel(host);
                    self.error = error;
                } else {
                    self.editing = false;
                    self.sync(host);
                    self.error.clear();
                }
            }
            Err(error) => {
                self.cancel(host);
                self.error = error;
            }
        }
    }
    pub fn select(&mut self, host: &mut dyn DesktopClient, ids: Vec<ObjectId>) {
        self.selected = ids.clone();
        host.command(DesktopCommand::Select(Selection {
            document: self.document,
            objects: ids,
        }));
        // An explicit selection also restores a network changed in another panel.
        self.network_layer = None;
        self.follow_layer(host);
    }
    pub fn select_layer(
        &mut self,
        host: &mut dyn DesktopClient,
        id: ObjectId,
        range: bool,
        toggle: bool,
    ) {
        let mut selected = self.selected.clone();
        if range
            && let Some(anchor) = selected.first()
            && let Some(composition) = &self.composition
        {
            let a = composition.layers.iter().position(|l| l.id == *anchor);
            let b = composition.layers.iter().position(|l| l.id == id);
            if let (Some(a), Some(b)) = (a, b) {
                selected = composition.layers[a.min(b)..=a.max(b)]
                    .iter()
                    .map(|l| l.id)
                    .collect();
            }
        } else if toggle {
            if selected.contains(&id) {
                selected.retain(|candidate| *candidate != id);
            } else {
                selected.push(id);
            }
        } else {
            selected = vec![id];
        }
        self.select(host, selected);
    }
    pub fn follow_layer(&mut self, host: &mut dyn DesktopClient) {
        let target = self.document.and_then(|document| {
            let [id] = self.selected.as_slice() else {
                return None;
            };
            let layer = self.composition.as_ref()?.layer(*id).ok()?;
            let Some(fold_platform::image_source::Source::Output(source)) = &layer.source else {
                return None;
            };
            let snapshot = host.snapshot()?;
            let graph = snapshot.state().documents.get(&source.document)?;
            (graph
                .extensions
                .get(crate::LAYER_GRAPH)
                .and_then(|v| v.as_bool())
                == Some(true))
            .then_some((document, *id, source.document))
        });
        if target != self.network_layer {
            self.network_layer = target;
            if let Some((_, id, _)) = target
                && let Err(error) = self.open(host, id)
            {
                self.error = error;
            }
        }
    }
    pub fn open(&mut self, host: &mut dyn DesktopClient, id: ObjectId) -> Result<(), String> {
        let layer = self
            .composition
            .as_ref()
            .ok_or("No composition")?
            .layer(id)?;
        let Some(fold_platform::image_source::Source::Output(source)) = &layer.source else {
            return Err("This layer has no editable source graph".into());
        };
        let time = layer.local_time(time(host))?;
        let info = host.video_info(source.document)?;
        if time < Time::ZERO || time >= info.time(info.frames)? {
            return Err("Playhead is outside the source range".into());
        }
        let location = ViewLocation {
            document: source.document,
            time,
            label: layer.name.clone(),
        };
        let graph = host.snapshot().is_some_and(|s| {
            s.state().documents.get(&source.document).is_some_and(|d| {
                d.extensions
                    .get(crate::LAYER_GRAPH)
                    .and_then(|v| v.as_bool())
                    == Some(true)
            })
        });
        if graph {
            host.command(DesktopCommand::OpenLayerGraph {
                location,
                objects: layer.graph_object().into_iter().collect(),
            });
        } else {
            host.command(DesktopCommand::Navigate(location));
        }
        Ok(())
    }
    pub fn command(&mut self, host: &mut dyn DesktopClient, id: &str, args: serde_json::Value) {
        let previous: Vec<_> = self
            .composition
            .iter()
            .flat_map(|c| c.layers.iter().map(|l| l.id))
            .collect();
        self.cancel(host);
        host.command(DesktopCommand::Extension(CommandRequest {
            id: id.into(),
            base: self.base,
            arguments: serde_json::to_vec(&args).unwrap(),
        }));
        if host.snapshot().is_some_and(|s| s.revision() == self.base) {
            self.error = host.state().status.clone();
        } else {
            self.sync(host);
            self.error.clear();
            if let Some(id) = self
                .composition
                .as_ref()
                .and_then(|c| c.layers.iter().find(|l| !previous.contains(&l.id)))
                .map(|l| l.id)
            {
                self.select(host, vec![id]);
            }
        }
    }
}
pub(super) fn time(host: &dyn DesktopClient) -> Time {
    host.state()
        .navigation
        .last()
        .map(|v| v.time)
        .unwrap_or(Time::ZERO)
}
