//! Selection intent is applied once. Linked views never publish it back.
//! Providers resolve editable content; this module owns group and occurrence routing.
use super::*;
use fold_foundation::ObjectId;
use fold_platform::desktop::Selection;

use fold_platform::workspace::SelectionLink;

pub(super) struct LockedInspector {
    owner: PanelInstanceId,
    selection: Selection,
    document_type: String,
    pub(super) panel: Box<dyn crate::sdk::Panel>,
}

impl Shell {
    pub(super) fn prepare_locked_inspector(&mut self, context: &mut dear_imgui_rs::Context) {
        let Some((owner, binding)) = &self.workspace.inspector_lock else {
            self.locked_inspector = None;
            return;
        };
        if self.locked_inspector.as_ref().is_some_and(|locked| {
            locked.owner == *owner
                && locked.selection == binding.selection
                && locked.document_type == binding.document_type
        }) {
            return;
        }
        self.locked_inspector = self
            .panels
            .iter()
            .find(|p| p.panel.document_type() == Some(binding.document_type.as_str()))
            .and_then(|p| p.panel.new_instance())
            .map(|mut panels| {
                panels
                    .inspector
                    .initialize(context, self.typography.as_ref());
                LockedInspector {
                    owner: *owner,
                    selection: binding.selection.clone(),
                    document_type: binding.document_type.clone(),
                    panel: panels.inspector,
                }
            });
    }
    /// Keep linked edit clocks current without changing any selection or focus.
    pub(super) fn refresh_link_times(&mut self, client: &mut dyn DesktopClient) {
        let links: Vec<_> = self
            .workspace
            .selection_links
            .iter()
            .filter_map(|(&id, _)| self.linked_parent(id).cloned().map(|link| (id, link)))
            .collect();
        for (id, link) in links {
            let Some((_, time)) = self.workspace.editor_context(link.parent) else {
                continue;
            };
            let mut parent = self.workspace.editors[&link.parent].clone();
            parent.selection.objects = vec![link.occurrence];
            parent.navigation.last_mut().unwrap().time = time;
            let Some(editor) = self.editors.get_mut(&link.parent) else {
                continue;
            };
            let mut context = PanelContext::new(client, &mut parent).instance(link.parent);
            if let Ok(Some(target)) = editor.panels.editor.selection_target(&mut context)
                && let Some(graph) = self.workspace.editors.get_mut(&id)
                && let Some(location) = graph.navigation.last_mut()
                && location.document == target.location.document
            {
                location.time = target.location.time;
            }
        }
    }
    fn bind_layer_graph(&mut self, mut graph: EditorInstance) -> Option<PanelInstanceId> {
        let template = self.panels.iter().find(|p| {
            p.panel.is_layer_graph()
                && p.panel.document_type() == Some(graph.document_type.as_str())
        })?;
        let id = self
            .workspace
            .editors
            .iter()
            .find(|(_, e)| {
                e.contribution == template.descriptor.id
                    && e.group == graph.group
                    && graph.group != workspace::LinkGroup::Unlinked
            })
            .map(|(&id, _)| id)
            .unwrap_or_else(|| {
                self.workspace
                    .add_editor(template.descriptor.id, &graph.document_type)
            });
        graph.contribution = template.descriptor.id.into();
        self.workspace.editors.insert(id, graph);
        self.workspace.follow_network(id);
        self.sync_instances();
        Some(id)
    }

    fn remember_link(&mut self, parent: PanelInstanceId, graph: PanelInstanceId) {
        self.workspace.selection_links.remove(&graph);
        if let Some(binding) = self.workspace.editors.get(&parent)
            && let Some(document) = binding.document()
            && let [occurrence] = binding.selection.objects.as_slice()
        {
            self.workspace.selection_links.insert(
                graph,
                SelectionLink {
                    parent,
                    document,
                    occurrence: *occurrence,
                },
            );
        }
    }

    /// Explicit open may focus Network. Ordinary selection linking never moves focus.
    pub(super) fn open_layer_graph(&mut self, parent: PanelInstanceId, graph: EditorInstance) {
        if let Some(id) = self.bind_layer_graph(graph) {
            self.remember_link(parent, id);
            self.workspace.focused_editor = Some(id);
            self.focus = Some(id);
        }
    }

    fn linked_parent(&self, graph: PanelInstanceId) -> Option<&SelectionLink> {
        let link = self.workspace.selection_links.get(&graph)?;
        let parent = self.workspace.editors.get(&link.parent)?;
        let target = self.workspace.editors.get(&graph)?;
        (parent.group == target.group && parent.document() == Some(link.document)).then_some(link)
    }

    pub(super) fn linked_layer(
        &self,
        graph: PanelInstanceId,
        object: Option<ObjectId>,
    ) -> Option<(ObjectId, bool)> {
        let link = self.linked_parent(graph)?;
        self.editors
            .get(&link.parent)?
            .panels
            .editor
            .layer_for_graph(
                self.workspace.editors.get(&graph)?.document()?,
                object,
                Some(link.occurrence),
            )
    }

    pub(super) fn publish_selection(
        &mut self,
        id: PanelInstanceId,
        selection: Selection,
        client: &mut dyn DesktopClient,
    ) {
        let Some(mut binding) = self.workspace.editors.get(&id).cloned() else {
            return;
        };
        if selection.document != binding.document() {
            return;
        }
        binding.selection = selection;
        self.workspace.editors.get_mut(&id).unwrap().selection = binding.selection.clone();
        if binding.group == workspace::LinkGroup::Unlinked {
            return;
        }
        if let Some((_, time)) = self.workspace.editor_context(id)
            && let Some(location) = binding.navigation.last_mut()
        {
            location.time = time;
        }
        let Some(editor) = self.editors.get_mut(&id) else {
            return;
        };
        let mut context = PanelContext::new(client, &mut binding).instance(id);
        let target = editor.panels.editor.selection_target(&mut context);
        let object = editor.panels.editor.selection_owner(&mut context);
        let is_graph = editor.panels.editor.is_layer_graph();
        drop(context);
        self.workspace.record_selection(id);
        self.workspace.follow_network(id);
        if is_graph {
            if let Some((row, _)) = self.linked_layer(id, object)
                && let Some(parent) = self.linked_parent(id).map(|link| link.parent)
                && let Some(parent) = self.workspace.editors.get_mut(&parent)
            {
                parent.selection.objects = vec![row];
            }
            return;
        }
        let target = match target {
            Ok(Some(target)) => target,
            Ok(None) => return,
            Err(error) => {
                client.command(DesktopCommand::Notify(error));
                return;
            }
        };
        let Some(document_type) = client.snapshot().and_then(|s| {
            s.state()
                .documents
                .get(&target.location.document)
                .map(|d| d.type_id.clone())
        }) else {
            return;
        };
        let mut graph = binding;
        graph.navigate(target.location);
        graph.document_type = document_type;
        graph.selection.objects = target.objects;
        if let Some(graph) = self.bind_layer_graph(graph) {
            self.remember_link(id, graph);
            if target.inspect_content {
                self.workspace.record_selection(graph);
            }
        }
    }

    pub(super) fn flush_selections(&mut self, client: &mut dyn DesktopClient) {
        for (id, selection) in std::mem::take(&mut self.selection_events) {
            self.publish_selection(id, selection, client);
        }
    }
}
