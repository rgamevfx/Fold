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
    Unlinked,
}
impl LinkGroup {
    pub const ALL: [Self; 4] = [Self::A, Self::B, Self::C, Self::D];
    pub fn label(self) -> &'static str {
        match self {
            Self::A => "A",
            Self::B => "B",
            Self::C => "C",
            Self::D => "D",
            Self::Unlinked => "Unlinked",
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
    /// Screen sampling only; authored image filters and export stay independent.
    #[serde(default)]
    pub pixel_exact: bool,
    /// None follows the output provider's default, including on source changes.
    #[serde(default)]
    pub playback_mode: Option<crate::desktop::PlaybackMode>,
    #[serde(default)]
    pub group: LinkGroup,
    pub binding: ViewerBinding,
    /// Stable editor contribution followed within the selected group.
    #[serde(default)]
    pub source: Option<String>,
    pub last_output: Option<DocumentRef>,
    /// Transient routing identity: nested mapping applies only within the same
    /// source editor, not when joining a different group or source.
    #[serde(skip)]
    pub last_editor: Option<PanelInstanceId>,
    /// Optional direct-tool association for unlinked outputs. Linked viewers use
    /// their chosen editor type within the group, never window focus.
    pub editor: Option<PanelInstanceId>,
    pub time: Time,
    pub divisor: u32,
    #[serde(default)]
    pub channels: crate::desktop::ChannelView,
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
            pixel_exact: false,
            playback_mode: None,
            group: LinkGroup::A,
            binding: ViewerBinding::Linked,
            source: None,
            last_output: None,
            last_editor: None,
            editor: None,
            time: Time::ZERO,
            divisor: 1,
            channels: Default::default(),
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
    pub group_selections: BTreeMap<LinkGroup, PanelInstanceId>,
    #[serde(default)]
    pub inspector_group: LinkGroup,
    #[serde(default)]
    pub inspector_lock: Option<(PanelInstanceId, EditorInstance)>,
    #[serde(default)]
    pub inspector_viewer: Option<PanelInstanceId>,
    #[serde(default)]
    pub monitored_viewer: Option<PanelInstanceId>,
    #[serde(default)]
    monitor_initialized: bool,
    #[serde(default)]
    pub preset_name: Option<String>,
    pub layout: String,
    next_id: u64,
}
impl Default for Workspace {
    fn default() -> Self {
        Self {
            version: 4,
            project: String::new(),
            panels: Default::default(),
            editors: Default::default(),
            viewers: Default::default(),
            focused_editor: None,
            group_selections: Default::default(),
            inspector_group: LinkGroup::A,
            inspector_lock: None,
            inspector_viewer: None,
            monitored_viewer: None,
            monitor_initialized: false,
            preset_name: None,
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
        let group = LinkGroup::ALL
            .into_iter()
            .find(|group| {
                !self
                    .editors
                    .values()
                    .any(|e| e.group == *group && e.contribution == contribution)
            })
            .unwrap_or(LinkGroup::Unlinked);
        let id = self.allocate();
        self.editors.insert(
            id,
            EditorInstance {
                group,
                contribution: contribution.into(),
                document_type: document_type.into(),
                navigation: vec![],
                selection: Default::default(),
                mapped_navigation: false,
            },
        );
        id
    }
    pub fn add_viewer(&mut self, mut viewer: ViewerInstance) -> PanelInstanceId {
        let id = self.allocate();
        if !self.monitor_initialized {
            self.monitored_viewer = Some(id);
            self.monitor_initialized = true;
        }
        if viewer.binding == ViewerBinding::Linked && viewer.source.is_none() {
            viewer.source = self
                .group_selections
                .get(&viewer.group)
                .and_then(|id| self.editors.get(id))
                .map(|e| e.contribution.clone())
                .or_else(|| {
                    self.editors
                        .values()
                        .find(|e| e.group == viewer.group && e.document().is_some())
                        .map(|e| e.contribution.clone())
                });
        }
        self.viewers.insert(id, viewer);
        id
    }
    pub fn resolve(&self, id: PanelInstanceId) -> Option<DocumentRef> {
        let viewer = self.viewers.get(&id)?;
        match &viewer.binding {
            ViewerBinding::Pinned(output) => Some(output.clone()),
            ViewerBinding::Linked => self.editors.get(&self.linked_editor(id)?)?.output(),
            ViewerBinding::Unbound => None,
        }
    }
    /// Record explicit selection/navigation without retargeting existing viewers.
    pub fn record_selection(&mut self, editor: PanelInstanceId) {
        if let Some(binding) = self.editors.get(&editor) {
            self.group_selections.insert(binding.group, editor);
            for viewer in self.viewers.values_mut().filter(|v| {
                v.group == binding.group && v.binding == ViewerBinding::Linked && v.source.is_none()
            }) {
                viewer.source = Some(binding.contribution.clone());
            }
        }
    }
    pub fn can_join_group(&self, editor: PanelInstanceId, group: LinkGroup) -> bool {
        group == LinkGroup::Unlinked
            || self.editors.get(&editor).is_some_and(|e| {
                !self.editors.iter().any(|(id, other)| {
                    *id != editor && other.group == group && other.contribution == e.contribution
                })
            })
    }
    pub fn set_editor_group(&mut self, editor: PanelInstanceId, group: LinkGroup) {
        if !self.can_join_group(editor, group) {
            return;
        }
        let Some(previous) = self.editors.get(&editor).map(|e| e.group) else {
            return;
        };
        if previous == group {
            return;
        }
        let followers: Vec<_> = self
            .viewers
            .keys()
            .copied()
            .filter(|id| self.linked_editor(*id) == Some(editor))
            .collect();
        for viewer in followers {
            self.unlink_viewer(viewer);
        }
        self.editors.get_mut(&editor).unwrap().group = group;
        self.group_selections.retain(|_, id| *id != editor);
    }
    pub fn set_viewer_group(&mut self, viewer: PanelInstanceId, group: LinkGroup) {
        if group == LinkGroup::Unlinked {
            self.unlink_viewer(viewer);
            return;
        }
        let default_source = self
            .group_selections
            .get(&group)
            .and_then(|id| self.editors.get(id))
            .map(|e| e.contribution.clone());
        if let Some(viewer) = self.viewers.get_mut(&viewer) {
            viewer.group = group;
            viewer.binding = ViewerBinding::Linked;
            viewer.editor = None;
            if viewer.source.is_none() {
                viewer.source = default_source;
            }
        }
    }
    pub fn follow_source(&mut self, viewer: PanelInstanceId, editor: PanelInstanceId) {
        let Some(editor) = self.editors.get(&editor) else {
            return;
        };
        if editor.group == LinkGroup::Unlinked {
            return;
        }
        if let Some(viewer) = self.viewers.get_mut(&viewer) {
            viewer.group = editor.group;
            viewer.source = Some(editor.contribution.clone());
            viewer.binding = ViewerBinding::Linked;
            viewer.editor = None;
        }
    }
    pub fn pin_output(&mut self, viewer: PanelInstanceId, output: DocumentRef) {
        let mut candidates = self
            .editors
            .iter()
            .filter(|(_, e)| e.output().as_ref() == Some(&output));
        let editor = candidates
            .next()
            .filter(|_| candidates.next().is_none())
            .map(|(&id, _)| id);
        let source = editor.map(|id| self.editors[&id].contribution.clone());
        if let Some(viewer) = self.viewers.get_mut(&viewer) {
            viewer.binding = ViewerBinding::Pinned(output);
            viewer.editor = editor;
            viewer.source = source;
        }
    }
    pub fn unlink_viewer(&mut self, id: PanelInstanceId) {
        let output = self
            .resolve(id)
            .or_else(|| self.viewers.get(&id)?.last_output.clone());
        if let Some(viewer) = self.viewers.get_mut(&id) {
            viewer.binding = output
                .map(ViewerBinding::Pinned)
                .unwrap_or(ViewerBinding::Unbound);
            viewer.editor = None;
        }
    }
    fn linked_editor(&self, id: PanelInstanceId) -> Option<PanelInstanceId> {
        let viewer = self.viewers.get(&id)?;
        if viewer.binding != ViewerBinding::Linked || viewer.group == LinkGroup::Unlinked {
            return None;
        }
        let source = viewer.source.as_ref()?;
        self.editors
            .iter()
            .find(|(_, e)| e.group == viewer.group && &e.contribution == source)
            .map(|(&id, _)| id)
    }
    pub fn editor_for_viewer(&self, viewer: PanelInstanceId) -> Option<PanelInstanceId> {
        let v = self.viewers.get(&viewer)?;
        let editor = if v.binding == ViewerBinding::Linked {
            self.linked_editor(viewer)
        } else {
            v.editor
        }?;
        (self.editors.get(&editor)?.output() == self.resolve(viewer)).then_some(editor)
    }
    pub fn inspector_editor(&self) -> Option<PanelInstanceId> {
        if let Some((id, _)) = &self.inspector_lock {
            return Some(*id);
        }
        self.group_selections
            .get(&self.inspector_group)
            .copied()
            .filter(|id| self.editors.contains_key(id))
            .or_else(|| {
                self.editors
                    .iter()
                    .find(|(_, e)| e.group == self.inspector_group && e.document().is_some())
                    .map(|(&id, _)| id)
            })
    }
    pub fn toggle_inspector_lock(&mut self) {
        if self.inspector_lock.is_some() {
            self.inspector_lock = None;
        } else {
            let time = self.inspector_context().map(|(_, time)| time);
            self.inspector_lock = self.inspector_editor().and_then(|id| {
                let mut binding = self.editors.get(&id)?.clone();
                if let Some(time) = time
                    && let Some(location) = binding.navigation.last_mut()
                {
                    location.time = time;
                }
                Some((id, binding))
            });
        }
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
    /// Inspection follows group selection or a locked selection. Use its matching
    /// viewer clock, or explicit editor-local time when that output is not viewed.
    /// Multiple matching clocks require an explicit viewer choice.
    pub fn inspector_context(&self) -> Option<(PanelInstanceId, Time)> {
        let editor = self.inspector_editor()?;
        let binding = self
            .inspector_lock
            .as_ref()
            .map(|(_, e)| e)
            .or_else(|| self.editors.get(&editor))?;
        let candidates: Vec<_> = self
            .viewers
            .iter()
            .filter(|(id, _)| {
                self.editor_for_viewer(**id) == Some(editor)
                    && self.resolve(**id) == binding.output()
            })
            .collect();
        let time = if let Some((_, viewer)) = candidates
            .iter()
            .find(|(id, _)| Some(**id) == self.inspector_viewer)
        {
            viewer.time
        } else {
            match candidates.as_slice() {
                [(_, viewer)] => viewer.time,
                [] => binding.navigation.last()?.time,
                _ => return None,
            }
        };
        Some((editor, time))
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
            }
        }
        self.group_selections
            .retain(|_, id| self.editors.contains_key(id));
    }
    pub fn close_editor(&mut self, id: PanelInstanceId) {
        self.reconcile(); // Capture the last target before removing its owner.
        let followers: Vec<_> = self
            .viewers
            .keys()
            .copied()
            .filter(|viewer| self.linked_editor(*viewer) == Some(id))
            .collect();
        for viewer in followers {
            self.unlink_viewer(viewer);
        }
        self.editors.remove(&id);
        if self
            .inspector_lock
            .as_ref()
            .is_some_and(|(owner, _)| *owner == id)
        {
            self.inspector_lock = None;
        }
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
    /// Portable panel arrangement: no project targets, selections, clocks or locks.
    pub fn preset(&self) -> Self {
        let mut preset = self.clone();
        preset.project.clear();
        preset.preset_name = None;
        preset.inspector_lock = None;
        for editor in preset.editors.values_mut() {
            editor.navigation.clear();
            editor.selection = Default::default();
            editor.mapped_navigation = false;
        }
        for viewer in preset.viewers.values_mut() {
            if matches!(viewer.binding, ViewerBinding::Pinned(_)) {
                viewer.binding = ViewerBinding::Unbound;
            }
            viewer.last_output = None;
            viewer.last_editor = None;
            viewer.editor = None;
            viewer.time = Time::ZERO;
            viewer.range = Default::default();
            viewer.playing = false;
        }
        preset
    }
    pub fn decode(bytes: &[u8], project: &str) -> Result<Self, String> {
        if bytes.len() > 2 * 1024 * 1024 {
            return Err("Workspace exceeds size limit".into());
        }
        let json = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
        let mut value: Self =
            serde_json::from_value(migration::upgrade(json)?).map_err(|e| e.to_string())?;
        if value.version != 4 || value.project != project {
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
                .group_selections
                .iter()
                .any(|(group, id)| value.editors.get(id).is_some_and(|e| e.group != *group))
            || value.inspector_lock.as_ref().is_some_and(|(id, e)| {
                !value.editors.contains_key(id)
                    || e.navigation.len() > 64
                    || e.navigation.iter().any(|v| v.time < Time::ZERO)
            })
            || value
                .viewers
                .values()
                .any(|v| ![1, 2, 4].contains(&v.divisor) || v.time < Time::ZERO)
        {
            return Err("invalid workspace instances".into());
        }
        let mut occupied = std::collections::BTreeSet::new();
        if value.editors.values().any(|e| {
            e.group != LinkGroup::Unlinked && !occupied.insert((e.group, e.contribution.clone()))
        }) {
            return Err("duplicate editor type in panel group".into());
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
        legacy["group_sources"] = legacy["group_selections"].take();
        for viewer in legacy["viewers"].as_object_mut().unwrap().values_mut() {
            viewer.as_object_mut().unwrap().remove("playback_mode");
        }
        let migrated = Workspace::decode(&serde_json::to_vec(&legacy).unwrap(), "").unwrap();
        assert_eq!(migrated.version, 4);
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
        w.record_selection(a);
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
