use crate::{model::Construction, package};
use fold_foundation::DocumentId;
use fold_platform::{
    desktop::{DesktopClient, DesktopCommand},
    packages::CommandRequest,
};
use fold_project::{CommittedSnapshot, Revision};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

pub(super) type Shared = Rc<RefCell<State>>;
pub(super) struct Draft {
    pub text: String,
    pub base: Revision,
    pub dirty: bool,
    snapshot: CommittedSnapshot,
}
#[derive(Default)]
pub(super) struct State {
    pub document: Option<DocumentId>,
    pub object: Option<fold_foundation::ObjectId>,
    pub graph_definition: Option<fold_foundation::ObjectId>,
    pub graph_history: Vec<fold_foundation::ObjectId>,
    pub graph_occurrences: Vec<fold_foundation::ObjectId>,
    pub base: Revision,
    pub construction: Option<Construction>,
    pub drafts: BTreeMap<(DocumentId, Option<fold_foundation::ObjectId>), Draft>,
    pub editing: bool,
    reload_pending: bool,
    pub literal_controls: super::literal::Controls,
    pub key_time: Option<(fold_foundation::ObjectId, fold_foundation::Time)>,
    pub error: String,
    pub auto_keys: BTreeMap<fold_foundation::ObjectId, fold_foundation::Time>,
}
impl State {
    pub fn sync(&mut self, host: &mut dyn DesktopClient) {
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        let desired = host.state().selection.document.filter(|id| {
            snapshot.state().documents.get(id).is_some_and(|d| {
                d.package_id == crate::PACKAGE
                    && matches!(d.type_id.as_str(), crate::CONSTRUCTION | crate::scene::TYPE)
            })
        });
        let desired_object = desired
            .and_then(|id| snapshot.state().documents.get(&id))
            .and_then(|d| {
                if d.type_id != crate::scene::TYPE {
                    return None;
                }
                let scene = crate::scene::Scene::from_document(d).ok()?;
                let selected = &host.state().selection.objects;
                scene
                    .objects
                    .iter()
                    .find(|o| selected.contains(&o.id))
                    .or_else(|| {
                        let contains = |o: &&crate::scene::Object| {
                            o.graph.definitions.iter().any(|d| {
                                selected.contains(&d.id)
                                    || d.nodes.iter().any(|n| selected.contains(&n.id))
                                    || d.inputs.iter().any(|i| selected.contains(&i.id))
                                    || d.instances.iter().any(|i| selected.contains(&i.id))
                            }) || selected.contains(&o.graph.root.id)
                        };
                        scene
                            .objects
                            .iter()
                            .find(|o| Some(o.id) == self.object && contains(o))
                            .or_else(|| scene.objects.iter().find(contains))
                    })
                    .map(|o| o.id)
            });
        if self.document == desired
            && self.object == desired_object
            && self.base == snapshot.revision()
            && !self.reload_pending
        {
            self.update_key_time(host);
            return;
        }
        self.reload_pending = false;
        let scope_changed = self.object != desired_object || self.document != desired;
        self.object = desired_object;
        self.literal_controls = Default::default();
        if self.editing {
            host.command(DesktopCommand::CancelPreviewEdit);
        }
        self.editing = false;
        self.auto_keys.clear();
        if scope_changed {
            self.error.clear();
            self.graph_definition = None;
            self.graph_history.clear();
            self.graph_occurrences.clear();
        }
        self.document = desired;
        self.base = snapshot.revision();
        self.construction = desired
            .and_then(|id| snapshot.state().documents.get(&id))
            .and_then(|d| {
                scoped_graph(d, self.object)
                    .map_err(|e| self.error = e)
                    .ok()
            });
        if let Some(construction) = &self.construction {
            self.literal_controls.set_assets(construction);
        }
        if let (Some(id), Some(construction)) = (desired, &self.construction)
            && self
                .drafts
                .get(&(id, self.object))
                .is_none_or(|draft| !draft.dirty)
        {
            match crate::source::canonical(construction) {
                Ok(text) => {
                    self.drafts.insert(
                        (id, self.object),
                        Draft {
                            text,
                            base: self.base,
                            snapshot: snapshot.clone(),
                            dirty: false,
                        },
                    );
                }
                Err(error) => self.error = error,
            }
        }
        self.update_key_time(host);
    }

    fn update_key_time(&mut self, host: &dyn DesktopClient) {
        self.key_time = None;
        let Some(graph) = &self.construction else {
            return;
        };
        let mut time = match self
            .object_time(host)
            .and_then(|time| graph.root.clock.at(time))
        {
            Ok(time) => time,
            Err(error) => {
                self.error = error;
                return;
            }
        };
        // A definition opened directly has no occurrence clock. Entering an
        // instance retains its exact path, including repeated uses of one asset.
        if self.graph_history.len() != self.graph_occurrences.len()
            || self
                .graph_history
                .first()
                .is_some_and(|id| *id != graph.root.definition)
        {
            return;
        }
        let mut definition = graph.root.definition;
        for id in &self.graph_occurrences {
            let result = graph.definition(definition).and_then(|owner| {
                let instance = owner
                    .instances
                    .iter()
                    .find(|instance| instance.id == *id)
                    .ok_or("The selected graph occurrence no longer exists")?;
                Ok((instance.definition, instance.clock.at(time)?))
            });
            match result {
                Ok((next, local)) => {
                    definition = next;
                    time = local;
                }
                Err(error) => {
                    self.error = error;
                    return;
                }
            }
        }
        if self.graph_definition.unwrap_or(graph.root.definition) == definition {
            self.key_time = Some((definition, time));
        }
    }
    pub fn object_time(&self, host: &dyn DesktopClient) -> Result<fold_foundation::Time, String> {
        let time = host
            .state()
            .navigation
            .last()
            .map_or(fold_foundation::Time::ZERO, |v| v.time);
        if let Some(object) = self.object {
            let snapshot = host.snapshot().ok_or("Missing snapshot")?;
            let document = snapshot
                .state()
                .documents
                .get(&self.document.ok_or("Missing scene")?)
                .ok_or("Missing scene")?;
            let scene = crate::scene::Scene::from_document(document)?;
            let mut time = time
                .checked_add(scene.canvas.start)
                .map_err(|e| e.to_string())?;
            for ancestor in scene.ancestors(object)? {
                time = ancestor.clock.at(time)?;
            }
            Ok(time)
        } else {
            time.checked_add(
                self.construction
                    .as_ref()
                    .ok_or("Missing graph")?
                    .canvas
                    .start,
            )
            .map_err(|e| e.to_string())
        }
    }
    pub fn reload_source(&mut self, host: &mut dyn DesktopClient) {
        self.sync(host);
        let Some(snapshot) = host.snapshot() else {
            return;
        };
        if let Some(id) = self.document {
            let result = snapshot
                .state()
                .documents
                .get(&id)
                .ok_or_else(|| "Missing Motion document".to_string())
                .and_then(|document| scoped_graph(document, self.object))
                .and_then(|construction| crate::source::canonical(&construction));
            match result {
                Ok(text) => {
                    self.drafts.insert(
                        (id, self.object),
                        Draft {
                            text,
                            base: self.base,
                            snapshot: snapshot.clone(),
                            dirty: false,
                        },
                    );
                    self.error.clear();
                }
                Err(error) => self.error = error,
            }
        }
    }
    /// Refresh the draft only; Apply remains an explicit, revision-checked action.
    pub fn rebase_source(&mut self, host: &mut dyn DesktopClient) {
        self.sync(host);
        let result = (|| -> Result<Draft, String> {
            let id = self.document.ok_or("No Motion document")?;
            let draft = self
                .drafts
                .get(&(id, self.object))
                .ok_or("No source draft")?;
            let snapshot = host.snapshot().ok_or("No project snapshot")?;
            let parsed = crate::source::parse(&draft.text).map_err(|error| {
                format!(
                    "{} (bytes {}–{})",
                    error.message, error.span.start, error.span.end
                )
            })?;
            let old_document = draft
                .snapshot
                .state()
                .documents
                .get(&id)
                .ok_or("Missing source base")?;
            validate_scoped(old_document, self.object, parsed.construction.clone())?;
            let current = snapshot
                .state()
                .documents
                .get(&id)
                .ok_or("Missing scene/graph")?;
            let merged = crate::merge::construction(
                &scoped_graph(old_document, self.object)?,
                &parsed.construction,
                &scoped_graph(current, self.object)?,
            )
            .map_err(|conflicts| format!("Source rebase conflicts: {conflicts:?}"))?;
            validate_scoped(current, self.object, merged.clone())?;
            Ok(Draft {
                text: crate::source::canonical(&merged)?,
                base: snapshot.revision(),
                snapshot,
                dirty: true,
            })
        })();
        match result {
            Ok(draft) => {
                self.drafts.insert(
                    (self.document.expect("active document"), self.object),
                    draft,
                );
                self.error.clear();
            }
            Err(error) => self.error = error,
        }
    }
    pub fn select(
        &self,
        host: &mut dyn DesktopClient,
        mut objects: Vec<fold_foundation::ObjectId>,
    ) {
        if let Some(object) = self.object {
            objects.retain(|id| *id != object);
            objects.insert(0, object);
        }
        host.command(DesktopCommand::Select(fold_platform::desktop::Selection {
            document: self.document,
            objects,
        }));
    }
    fn request(&self, construction: Construction) -> Result<CommandRequest, String> {
        let document = self.document.ok_or("No Motion document")?;
        let (id, arguments) = if let Some(object) = self.object {
            (
                crate::scene::GRAPH_EDIT,
                serde_json::to_vec(&crate::scene::package::GraphEditArgs {
                    document,
                    object,
                    construction,
                }),
            )
        } else {
            (
                package::EDIT,
                serde_json::to_vec(&package::EditArgs {
                    document,
                    construction,
                }),
            )
        };
        Ok(CommandRequest {
            id: id.into(),
            base: self.base,
            arguments: arguments.map_err(|e| e.to_string())?,
        })
    }

    pub fn preview(&mut self, host: &mut dyn DesktopClient) {
        self.editing = true;
        if let Some(construction) = self.construction.clone() {
            match self.request(construction) {
                Ok(request) => host.command(DesktopCommand::PreviewExtension(request)),
                Err(error) => self.error = error,
            }
        }
    }
    pub fn cancel(&mut self, host: &mut dyn DesktopClient) {
        host.command(DesktopCommand::CancelPreviewEdit);
        self.editing = false;
        self.auto_keys.clear();
        self.reload_pending = true;
        self.sync(host);
    }
    pub fn finish_parameters(
        &mut self,
        host: &mut dyn DesktopClient,
        mut construction: Construction,
    ) -> bool {
        let pending = std::mem::take(&mut self.auto_keys);
        for (parameter, time) in pending {
            match crate::keyed::animate(&construction, parameter, time) {
                Ok(after) => construction = after,
                Err(error) => {
                    self.error = error;
                    self.cancel(host);
                    return false;
                }
            }
        }
        self.commit(host, construction)
    }
    pub fn commit(&mut self, host: &mut dyn DesktopClient, construction: Construction) -> bool {
        let Some(snapshot) = host.snapshot() else {
            return false;
        };
        let Some(document) = self.document else {
            return false;
        };
        if snapshot.revision() != self.base {
            self.error = "Document changed. Reload before applying.".into();
            return false;
        }
        let Some(original) = snapshot.state().documents.get(&document) else {
            return false;
        };
        if scoped_graph(original, self.object).as_ref() == Ok(&construction) {
            self.error.clear();
            // A zero-angle rotation can change its chosen axis without changing
            // the document. Keep that UI draft across a completed no-op gesture.
            let controls = std::mem::take(&mut self.literal_controls);
            self.cancel(host);
            self.literal_controls = controls;
            return true;
        }
        if let Err(error) = validate_scoped(original, self.object, construction.clone()) {
            self.error = error;
            self.cancel(host);
            return false;
        }
        let request = match self.request(construction.clone()) {
            Ok(request) => request,
            Err(error) => {
                self.error = error;
                return false;
            }
        };
        host.command(DesktopCommand::Extension(request));
        let applied = host
            .snapshot()
            .and_then(|s| s.state().documents.get(&document).cloned())
            .and_then(|d| scoped_graph(&d, self.object).ok())
            .as_ref()
            == Some(&construction);
        if applied {
            self.error.clear();
        } else {
            self.error = format!("Motion edit was not applied. {}", host.state().status);
        }
        self.editing = false;
        self.auto_keys.clear();
        self.reload_pending = true;
        self.sync(host);
        applied
    }
}

pub(super) fn scoped_graph(
    document: &fold_project::Document,
    object: Option<fold_foundation::ObjectId>,
) -> Result<Construction, String> {
    if document.type_id == crate::scene::TYPE {
        let scene = crate::scene::Scene::from_document(document)?;
        Ok(scene
            .object(object.ok_or("Select an object in Outliner")?)?
            .graph
            .clone())
    } else {
        Construction::from_document(document)
    }
}
fn validate_scoped(
    document: &fold_project::Document,
    object: Option<fold_foundation::ObjectId>,
    graph: Construction,
) -> Result<(), String> {
    if document.type_id == crate::scene::TYPE {
        let mut scene = crate::scene::Scene::from_document(document)?;
        graph.validate_semantics()?;
        scene
            .objects
            .iter_mut()
            .find(|o| Some(o.id) == object)
            .ok_or("Missing object")?
            .graph = graph;
        scene.replace(document).map(|_| ())
    } else {
        crate::editing::validate_document(document, crate::editing::Scope::Document, graph)
            .map(|_| ())
            .map_err(|e| format!("{e:?}"))
    }
}
