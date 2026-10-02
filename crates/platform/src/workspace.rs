//! Workspace identities and navigation, never authored project content.
use crate::desktop::{Selection, ViewLocation};
use fold_foundation::{DocumentId, Time};
pub use fold_project::DocumentRef;

/// Names are presentation only. Bin ancestry/type and stable ordering disambiguate
/// duplicates; every choice continues to carry the original document ID.
pub fn document_labels(snapshot: &fold_project::Snapshot) -> BTreeMap<DocumentId, String> {
    let mut labels = BTreeMap::new();
    for document in snapshot.state().documents.values() {
        let item = snapshot
            .state()
            .organization
            .items
            .iter()
            .find(|i| i.id == fold_project::ItemId::Document(document.id));
        let mut kind = document
            .type_id
            .rsplit('.')
            .next()
            .unwrap_or(&document.type_id);
        if kind == "document" {
            kind = document
                .package_id
                .rsplit('.')
                .next()
                .unwrap_or(&document.package_id);
        }
        let mut parts = vec![item.map(|i| i.name.clone()).unwrap_or_else(|| kind.into())];
        let mut parent = item.map(|i| i.parent).unwrap_or_default();
        for _ in 0..64 {
            let fold_project::Parent::Bin(id) = parent else {
                break;
            };
            let Some(bin) = snapshot.state().organization.bins.get(&id) else {
                break;
            };
            parts.push(bin.name.clone());
            parent = bin.parent;
        }
        parts.reverse();
        labels.insert(document.id, format!("{} · {kind}", parts.join(" / ")));
    }
    let mut counts = BTreeMap::<String, usize>::new();
    for label in labels.values() {
        *counts.entry(label.clone()).or_default() += 1;
    }
    let mut ordinals = BTreeMap::<String, usize>::new();
    for label in labels.values_mut() {
        if counts[label] > 1 {
            let n = ordinals.entry(label.clone()).or_default();
            *n += 1;
            *label = format!("{label} ({n})");
        }
    }
    labels
}
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug)]
pub struct OutputDescriptor {
    pub reference: DocumentRef,
    pub label: String,
    pub info: Result<fold_media::VideoInfo, String>,
    pub playback_mode: crate::desktop::PlaybackMode,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct PanelInstanceId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum LinkGroup {
    #[default]
    A,
    B,
    C,
    D,
}
impl LinkGroup {
    pub const ALL: [Self; 4] = [Self::A, Self::B, Self::C, Self::D];
    pub fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditorInstance {
    #[serde(default)]
    pub group: LinkGroup,
    pub contribution: String,
    pub document_type: String,
    pub navigation: Vec<ViewLocation>,
    pub selection: Selection,
    #[serde(default)]
    pub mapped_navigation: bool,
}
impl EditorInstance {
    pub fn document(&self) -> Option<DocumentId> {
        self.navigation.last().map(|v| v.document)
    }
    pub fn navigate(&mut self, location: ViewLocation) {
        if let Some(index) = self
            .navigation
            .iter()
            .position(|v| v.document == location.document)
        {
            self.navigation.truncate(index);
        }
        self.selection = Selection {
            document: Some(location.document),
            objects: vec![],
        };
        self.navigation.push(location);
        self.mapped_navigation = true;
    }
    pub fn bind(&mut self, location: ViewLocation) {
        self.navigation.clear();
        self.navigate(location);
        self.mapped_navigation = false;
    }
    pub fn back(&mut self) {
        if self.navigation.len() > 1 {
            self.navigation.pop();
            self.mapped_navigation = true;
            self.selection = Selection {
                document: self.document(),
                objects: vec![],
            };
        }
    }
    pub fn output(&self) -> Option<DocumentRef> {
        Some(DocumentRef {
            document: self.document()?,
            output: "video".into(),
            extensions: Default::default(),
        })
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ViewerBinding {
    Pinned(DocumentRef),
    Linked,
    Unbound,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ViewerInstance {
    /// None follows the output provider's default, including on source changes.
    #[serde(default)]
    pub playback_mode: Option<crate::desktop::PlaybackMode>,
    #[serde(default)]
    pub group: LinkGroup,
    pub binding: ViewerBinding,
    pub last_output: Option<DocumentRef>,
    /// Transient routing identity: nested mapping applies only within the same
    /// source editor, not when joining a different group or source.
    #[serde(skip)]
    pub last_editor: Option<PanelInstanceId>,
    /// Optional direct-tool association for pinned outputs. Linked viewers use
    /// their group's source editor, never an association inferred from focus.
    pub editor: Option<PanelInstanceId>,
    pub time: Time,
    pub divisor: u32,
    #[serde(default)]
    pub range: crate::desktop::PlaybackRange,
    #[serde(default)]
    pub looping: bool,
    #[serde(skip)]
    pub playing: bool,
}
impl Default for ViewerInstance {
    fn default() -> Self {
        Self {
            playback_mode: None,
            group: LinkGroup::A,
            binding: ViewerBinding::Linked,
            last_output: None,
            last_editor: None,
            editor: None,
            time: Time::ZERO,
            divisor: 2,
            range: Default::default(),
            looping: false,
            playing: false,
        }
    }
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Workspace {
    pub version: u32,
    /// Canonical project path supplied by the host, not a document name.
    pub project: String,
    #[serde(default)]
    pub panels: BTreeMap<PanelInstanceId, String>,
    pub editors: BTreeMap<PanelInstanceId, EditorInstance>,
    pub viewers: BTreeMap<PanelInstanceId, ViewerInstance>,
    pub focused_editor: Option<PanelInstanceId>,
    #[serde(default)]
    pub group_sources: BTreeMap<LinkGroup, PanelInstanceId>,
    #[serde(default)]
    pub inspector_group: LinkGroup,
    #[serde(default)]
    pub inspector_viewer: Option<PanelInstanceId>,
    #[serde(default)]
    pub monitored_viewer: Option<PanelInstanceId>,
    #[serde(default)]
    monitor_initialized: bool,
    pub layout: String,
    next_id: u64,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: 3,
            project: String::new(),
            panels: Default::default(),
            editors: Default::default(),
            viewers: Default::default(),
            focused_editor: None,
            group_sources: Default::default(),
            inspector_group: LinkGroup::A,
            inspector_viewer: None,
            monitored_viewer: None,
            monitor_initialized: false,
            layout: String::new(),
            next_id: 1,
        }
    }
}
impl Workspace {
    fn allocate(&mut self) -> PanelInstanceId {
        let id = PanelInstanceId(self.next_id);
        self.next_id += 1;
        id
    }
    pub fn panel(&mut self, contribution: &str) -> PanelInstanceId {
        if let Some((&id, _)) = self
            .panels
            .iter()
            .find(|(_, value)| value.as_str() == contribution)
        {
            return id;
        }
        let id = self.allocate();
        self.panels.insert(id, contribution.into());
        id
    }
    pub fn add_editor(&mut self, contribution: &str, document_type: &str) -> PanelInstanceId {
        let id = self.allocate();
        self.editors.insert(
            id,
            EditorInstance {
                group: LinkGroup::A,
                contribution: contribution.into(),
                document_type: document_type.into(),
                navigation: vec![],
                selection: Default::default(),
                mapped_navigation: false,
            },
        );
        id
    }
    pub fn add_viewer(&mut self, viewer: ViewerInstance) -> PanelInstanceId {
        let id = self.allocate();
        if !self.monitor_initialized {
            self.monitored_viewer = Some(id);
            self.monitor_initialized = true;
        }
        self.viewers.insert(id, viewer);
        id
    }
    pub fn resolve(&self, id: PanelInstanceId) -> Option<DocumentRef> {
        let viewer = self.viewers.get(&id)?;
        match &viewer.binding {
            ViewerBinding::Pinned(output) => Some(output.clone()),
            ViewerBinding::Linked => self
                .editors
                .get(self.group_sources.get(&viewer.group)?)?
                .output(),
            ViewerBinding::Unbound => None,
        }
    }
    /// A group source is chosen explicitly, never by window focus.
    pub fn publish(&mut self, editor: PanelInstanceId) {
        if let Some(binding) = self.editors.get(&editor) {
            self.group_sources.insert(binding.group, editor);
        }
    }
    fn detach_group(&mut self, group: LinkGroup) {
        self.reconcile();
        self.group_sources.remove(&group);
        for viewer in self
            .viewers
            .values_mut()
            .filter(|v| v.group == group && v.binding == ViewerBinding::Linked)
        {
            viewer.binding = viewer
                .last_output
                .clone()
                .map(ViewerBinding::Pinned)
                .unwrap_or(ViewerBinding::Unbound);
            viewer.editor = None;
        }
    }
    pub fn set_editor_group(&mut self, editor: PanelInstanceId, group: LinkGroup) {
        let Some(previous) = self.editors.get(&editor).map(|e| e.group) else {
            return;
        };
        if previous != group && self.group_sources.get(&previous) == Some(&editor) {
            self.detach_group(previous);
        }
        self.editors.get_mut(&editor).unwrap().group = group;
        self.publish(editor);
    }
    pub fn set_viewer_group(&mut self, viewer: PanelInstanceId, group: LinkGroup) {
        if let Some(viewer) = self.viewers.get_mut(&viewer) {
            viewer.group = group;
            viewer.binding = ViewerBinding::Linked;
            viewer.editor = None;
        }
    }
    pub fn editor_for_viewer(&self, viewer: PanelInstanceId) -> Option<PanelInstanceId> {
        let v = self.viewers.get(&viewer)?;
        let editor = if v.binding == ViewerBinding::Linked {
            self.group_sources.get(&v.group).copied()
        } else {
            v.editor
        }?;
        (self.editors.get(&editor)?.output() == self.resolve(viewer)).then_some(editor)
    }
    pub fn inspector_editor(&self) -> Option<PanelInstanceId> {
        self.group_sources
            .get(&self.inspector_group)
            .copied()
            .filter(|id| self.editors.contains_key(id))
    }
    pub fn viewer_for_editor(&self, editor: PanelInstanceId) -> Result<PanelInstanceId, String> {
        let output = self
            .editors
            .get(&editor)
            .and_then(EditorInstance::output)
            .ok_or("This editor has no document output")?;
        let candidates: Vec<_> = self
            .viewers
            .iter()
            .filter(|(id, _)| {
                self.editor_for_viewer(**id) == Some(editor)
                    && self.resolve(**id).as_ref() == Some(&output)
            })
            .map(|(&id, _)| id)
            .collect();
        if let Some(id) = self.inspector_viewer
            && candidates.contains(&id)
        {
            return Ok(id);
        }
        match candidates.as_slice() {
            [id] => Ok(*id),
            [] => Err("Associate a viewer with this editor".into()),
            _ => Err("Select a viewer to choose the edit-time context".into()),
        }
    }
    /// The inspector uses its group's source and one unambiguous/explicitly
    /// selected viewer clock, never the last focused editor or completed request.
    pub fn inspector_context(&self) -> Option<(PanelInstanceId, Time)> {
        let editor = self.inspector_editor()?;
        let viewer = self.viewer_for_editor(editor).ok()?;
        Some((editor, self.viewers[&viewer].time))
    }
    pub fn reconcile(&mut self) {
        if self
            .monitored_viewer
            .is_some_and(|id| !self.viewers.contains_key(&id))
        {
            self.monitored_viewer = None;
        }
        let resolved: Vec<_> = self
            .viewers
            .keys()
            .map(|&id| (id, self.resolve(id)))
            .collect();
        for (id, output) in resolved {
            let viewer = self.viewers.get_mut(&id).unwrap();
            if let Some(output) = output {
                viewer.last_output = Some(output);
            } else if viewer.binding == ViewerBinding::Linked
                && self
                    .group_sources
                    .get(&viewer.group)
                    .is_some_and(|editor| !self.editors.contains_key(editor))
            {
                viewer.binding = viewer
                    .last_output
                    .clone()
                    .map(ViewerBinding::Pinned)
                    .unwrap_or(ViewerBinding::Unbound);
                viewer.editor = None;
            }
        }
        self.group_sources
            .retain(|_, id| self.editors.contains_key(id));
    }
    pub fn close_editor(&mut self, id: PanelInstanceId) {
        self.reconcile(); // Capture the last target before removing its owner.
        self.editors.remove(&id);
        if self.focused_editor == Some(id) {
            self.focused_editor = None;
        }
        for viewer in self.viewers.values_mut() {
            if viewer.editor == Some(id) {
                viewer.editor = None;
            }
        }
        self.reconcile();
    }
    pub fn decode(bytes: &[u8], project: &str) -> Result<Self, String> {
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Workspace exceeds size limit".into());
        }
        let json = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let mut value: Self =
            serde_json::from_value(migration::upgrade(json)?).map_err(|e| e.to_string())?;
        if value.version != 3 || value.project != project {
            return Err("incompatible workspace or project association".into());
        }
        if value.panels.len() + value.editors.len() + value.viewers.len() > 64
            || value
                .editors
                .keys()
                .any(|id| value.viewers.contains_key(id) || value.panels.contains_key(id))
            || value.viewers.keys().any(|id| value.panels.contains_key(id))
            || value.editors.values().any(|e| {
                e.navigation.len() > 64 || e.navigation.iter().any(|v| v.time < Time::ZERO)
            })
            || value
                .group_sources
                .iter()
                .any(|(group, id)| value.editors.get(id).is_some_and(|e| e.group != *group))
            || value
                .viewers
                .values()
                .any(|v| ![1, 2, 4].contains(&v.divisor) || v.time < Time::ZERO)
        {
            return Err("invalid workspace instances".into());
        }
        let max = value
            .editors
            .keys()
            .chain(value.viewers.keys())
            .chain(value.panels.keys())
            .map(|id| id.0)
            .max()
            .unwrap_or(0);
        value.next_id = max.checked_add(1).ok_or("workspace identity exhausted")?;
        value.monitor_initialized = true;
        value.reconcile();
        Ok(value)
    }
}

#[cfg(test)]
#[path = "workspace_group_tests.rs"]
mod group_tests;
#[path = "workspace_migration.rs"]
mod migration;

#[cfg(test)]
mod tests {
    use super::*;
    fn location(document: DocumentId, time: Time) -> ViewLocation {
        ViewLocation {
            document,
            time,
            label: "Same name".into(),
        }
    }
    #[test]
    fn loop_monitor_and_marks_restore_but_playback_does_not() {
        let mut w = Workspace::default();
        let a = w.add_viewer(ViewerInstance {
            playing: true,
            playback_mode: Some(crate::desktop::PlaybackMode::EveryFrame),
            looping: true,
            range: crate::desktop::PlaybackRange {
                start: Some(4),
                end: Some(9),
            },
            ..Default::default()
        });
        let b = w.add_viewer(Default::default());
        assert_eq!(w.monitored_viewer, Some(a));
        let restored = Workspace::decode(&serde_json::to_vec(&w).unwrap(), "").unwrap();
        assert!(!restored.viewers[&a].playing);
        assert!(restored.viewers[&a].looping);
        assert_eq!(
            restored.viewers[&a].playback_mode,
            Some(crate::desktop::PlaybackMode::EveryFrame)
        );
        assert_eq!(restored.viewers[&b].playback_mode, None);
        let mut legacy = serde_json::to_value(&w).unwrap();
        legacy["version"] = serde_json::json!(2);
        for viewer in legacy["viewers"].as_object_mut().unwrap().values_mut() {
            viewer.as_object_mut().unwrap().remove("playback_mode");
        }
        let migrated = Workspace::decode(&serde_json::to_vec(&legacy).unwrap(), "").unwrap();
        assert_eq!(migrated.version, 3);
        assert!(
            migrated
                .viewers
                .values()
                .all(|v| v.playback_mode.is_none() && !v.playing)
        );
        assert_eq!(restored.viewers[&a].range, w.viewers[&a].range);
        assert_eq!(restored.monitored_viewer, Some(a));
        w.viewers.remove(&a);
        w.reconcile();
        assert_eq!(w.monitored_viewer, None);
        w.viewers.remove(&b);
        w.add_viewer(Default::default());
        assert_eq!(
            w.monitored_viewer, None,
            "creating a viewer does not silently resume monitoring"
        );
    }
    #[test]
    fn contribution_instances_restore_without_reusing_editor_or_viewer_identities() {
        let mut w = Workspace::default();
        w.project = "/project/a.fold".into();
        let panel = w.panel("foreign.project-panel");
        assert_eq!(w.panel("foreign.project-panel"), panel);
        let editor = w.add_editor("editor", "kind");
        let viewer = w.add_viewer(Default::default());
        assert_ne!(panel, editor);
        assert_ne!(panel, viewer);
        let mut restored = Workspace::decode(&serde_json::to_vec(&w).unwrap(), &w.project).unwrap();
        assert_eq!(restored.panel("foreign.project-panel"), panel);
        let next = restored.panel("another-panel");
        assert!(next.0 > viewer.0);
        restored.editors.insert(panel, w.editors[&editor].clone());
        assert!(Workspace::decode(&serde_json::to_vec(&restored).unwrap(), &w.project).is_err());
    }
    #[test]
    fn targets_nested_time_closure_and_restore_are_isolated() {
        let mut w = Workspace::default();
        w.project = "/project/a.fold".into();
        let a = w.add_editor("editor", "kind");
        let b = w.add_editor("editor", "kind");
        let root = DocumentId::new();
        let nested = DocumentId::new();
        w.editors
            .get_mut(&a)
            .unwrap()
            .bind(location(root, Time::new(7, 24).unwrap()));
        w.editors
            .get_mut(&b)
            .unwrap()
            .bind(location(root, Time::ZERO));
        w.publish(a);
        let x = w.add_viewer(ViewerInstance {
            binding: ViewerBinding::Linked,
            editor: Some(a),
            ..Default::default()
        });
        let y = w.add_viewer(ViewerInstance {
            binding: ViewerBinding::Pinned(w.editors[&b].output().unwrap()),
            ..Default::default()
        });
        w.editors
            .get_mut(&a)
            .unwrap()
            .navigate(location(nested, Time::new(1, 48).unwrap()));
        w.focused_editor = Some(a);
        w.inspector_viewer = Some(x);
        w.viewers.get_mut(&x).unwrap().time = Time::new(7, 48).unwrap();
        assert_eq!(w.inspector_context(), Some((a, Time::new(7, 48).unwrap())));
        w.viewers.get_mut(&y).unwrap().time = Time::new(1, 8).unwrap();
        assert_eq!(w.inspector_context(), Some((a, Time::new(7, 48).unwrap())));
        w.focused_editor = Some(b);
        assert_eq!(
            w.inspector_context(),
            Some((a, Time::new(7, 48).unwrap())),
            "focus does not switch the inspector's group source"
        );
        assert_eq!(w.resolve(x).unwrap().document, nested);
        assert_eq!(w.resolve(y).unwrap().document, root);
        w.editors.get_mut(&a).unwrap().back();
        assert_eq!(w.editors[&a].navigation[0].time, Time::new(7, 24).unwrap());
        w.reconcile();
        w.close_editor(a);
        assert!(matches!(w.viewers[&x].binding, ViewerBinding::Pinned(_)));
        assert_eq!(w.resolve(x).unwrap().document, root);
        let bytes = serde_json::to_vec(&w).unwrap();
        let restored = Workspace::decode(&bytes, &w.project).unwrap();
        assert_eq!(restored.resolve(x), w.resolve(x));
        assert!(Workspace::decode(&bytes, "/different.fold").is_err());
    }
}
