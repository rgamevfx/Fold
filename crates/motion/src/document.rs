//! Versioned motion payload. Unknown nodes/data survive persistence without evaluation.
use crate::{
    fields::{Datum, Kind},
    graph::{Graph, Input, Link, Node, definition::Socket, registry},
    nodes::interface::{GroupDefinition, GroupSettings},
};
use fold_foundation::{DocumentId, ObjectId};
use fold_media::VideoInfo;
use fold_project::{Document, Revision};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
pub const MOTION: &str = "fold.motion.document";
pub const FONT_LICENSE: &str = include_str!("../resources/fonts/OFL.txt");
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Font {
    pub name: String,
    pub face_index: u32,
    pub source: FontSource,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum FontSource {
    NotoSansRegularV1,
    Embedded(Vec<u8>),
}
impl Font {
    pub fn bytes(&self) -> &[u8] {
        match &self.source {
            FontSource::NotoSansRegularV1 => {
                include_bytes!("../resources/fonts/NotoSans-Regular.ttf")
            }
            FontSource::Embedded(bytes) => bytes,
        }
    }
    pub fn builtin() -> Self {
        Self {
            name: "Noto Sans Regular".into(),
            face_index: 0,
            source: FontSource::NotoSansRegularV1,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    pub info: VideoInfo,
    pub graph: Graph,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scene: Option<crate::scene::Scene>,
    #[serde(default)]
    pub fonts: BTreeMap<ObjectId, Font>,
    #[serde(default)]
    pub controls: BTreeMap<ObjectId, Datum>,
    #[serde(default)]
    pub groups: BTreeMap<ObjectId, GroupDefinition>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
impl Motion {
    pub fn new_scene() -> Self {
        Self {
            scene: Some(crate::scene::Scene::default()),
            ..Self::empty()
        }
    }

    pub fn empty() -> Self {
        let output = Node::new("fold.motion.output").expect("built-in output");
        let link = Link {
            node: output.id,
            socket: "content".into(),
        };
        Self {
            info: VideoInfo {
                width: 1280,
                height: 720,
                rate: [24, 1],
                frames: 120,
            },
            graph: Graph {
                nodes: vec![output],
                output: link,
            },
            scene: None,
            fonts: BTreeMap::new(),
            controls: BTreeMap::new(),
            groups: BTreeMap::new(),
            extensions: BTreeMap::new(),
        }
    }
    /// Incomplete wiring is editable. Unavailable node payloads remain opaque;
    /// compilation separately requires registered implementations and inputs.
    pub fn validate(&self) -> Result<(), String> {
        self.info.validate()?;
        if let Some(scene) = &self.scene {
            scene.validate(self)?;
        }
        let nodes = self.graph.nodes.len()
            + self
                .groups
                .values()
                .map(|g| g.graph.nodes.len())
                .sum::<usize>();
        if nodes > 4096 || self.groups.len() > 128 {
            return Err("motion construction exceeds graph budget".into());
        }
        if self.fonts.values().map(|f| f.bytes().len()).sum::<usize>() > 16 * 1024 * 1024 {
            return Err("embedded fonts exceed 16 MiB".into());
        }
        self.validate_graph(&self.graph)?;
        let mut controls = BTreeSet::new();
        for n in &self.graph.nodes {
            if n.kind == "fold.motion.published_control" && n.version == 1 {
                let s = n.settings::<crate::nodes::interface::ControlSettings>()?;
                if !controls.insert(s.id) {
                    return Err("duplicate published control identity".into());
                }
            }
        }
        for (id, group) in &self.groups {
            self.validate_graph(&group.graph)?;
            let mut ports = BTreeSet::new();
            for p in &group.inputs {
                if p.id.is_empty()
                    || !ports.insert(&p.id)
                    || p.kind == Kind::Generic
                    || p.default.as_ref().is_some_and(|v| v.kind() != p.kind)
                {
                    return Err("invalid group input interface".into());
                }
            }
            let mut path = BTreeSet::new();
            self.validate_group_references(*id, &mut path)?;
        }
        for value in self.controls.values() {
            value.validate()?;
        }
        Ok(())
    }
    fn validate_group_references(
        &self,
        id: ObjectId,
        path: &mut BTreeSet<ObjectId>,
    ) -> Result<(), String> {
        if path.len() >= 32 || !path.insert(id) {
            return Err("recursive or excessively nested node groups".into());
        }
        let group = self.groups.get(&id).ok_or("missing group")?;
        for n in &group.graph.nodes {
            if n.kind == "fold.motion.group_instance" {
                if let Some(id) = n.settings::<GroupSettings>()?.group {
                    self.validate_group_references(id, path)?;
                }
            }
        }
        path.remove(&id);
        Ok(())
    }
    fn validate_graph(&self, graph: &Graph) -> Result<(), String> {
        let mut ids = BTreeSet::new();
        for n in &graph.nodes {
            if !ids.insert(n.id) || !n.position.iter().all(|v| v.is_finite()) {
                return Err("duplicate node ID or invalid graph position".into());
            }
            if n.inputs.len() > 256 {
                return Err("node input budget exceeded".into());
            }
            for v in n.inputs.values() {
                if let Input::Value(v) = v {
                    v.validate()?;
                }
            }
            if let Ok(d) = registry::find(&n.kind)
                && n.version == 1
            {
                n.validate_animation()?;
                (d.validate)(n).map_err(|e| format!("{} {:?}: {e}", d.name, n.id))?;
            }
        }
        graph.node(graph.output.node)?;
        for n in &graph.nodes {
            if registry::find(&n.kind).is_err() || n.version != 1 {
                continue;
            }
            // A newly created group instance may be unassigned, like an unconnected socket.
            if n.kind == "fold.motion.group_instance"
                && n.settings::<GroupSettings>()?.group.is_none()
            {
                continue;
            }
            let (sockets, _) = self.signature(n)?;
            for (name, input) in &n.inputs {
                let slot = sockets
                    .iter()
                    .find(|s| s.id == *name)
                    .ok_or_else(|| format!("{} {:?}: unknown input {name}", n.kind, n.id))?;
                let kind = match input {
                    Input::Value(v) => Some(v.kind()),
                    Input::Link(l) => {
                        let source = graph.node(l.node)?;
                        if registry::find(&source.kind).is_ok() && source.version == 1 {
                            Some(
                                self.signature(source)?
                                    .1
                                    .iter()
                                    .find(|(name, _)| *name == l.socket)
                                    .ok_or_else(|| {
                                        format!("missing output {} at {:?}", l.socket, l.node)
                                    })?
                                    .1,
                            )
                        } else {
                            None
                        }
                    }
                };
                if kind.is_some_and(|k| k != slot.kind) {
                    return Err(format!(
                        "{} {:?}.{name}: expected {:?}, got {kind:?}",
                        n.kind, n.id, slot.kind
                    ));
                }
            }
        }
        let mut done = BTreeSet::new();
        let mut active = BTreeSet::new();
        fn visit(
            g: &Graph,
            id: ObjectId,
            done: &mut BTreeSet<ObjectId>,
            active: &mut BTreeSet<ObjectId>,
        ) -> Result<(), String> {
            if done.contains(&id) {
                return Ok(());
            }
            if active.len() >= 128 || !active.insert(id) {
                return Err(format!("cyclic or excessively deep graph at {id:?}"));
            }
            for input in g.node(id)?.inputs.values() {
                if let Input::Link(l) = input {
                    visit(g, l.node, done, active)?;
                }
            }
            active.remove(&id);
            done.insert(id);
            Ok(())
        }
        for n in &graph.nodes {
            visit(graph, n.id, &mut done, &mut active)?;
        }
        Ok(())
    }
    pub fn signature(&self, node: &Node) -> Result<(Vec<Socket>, Vec<(String, Kind)>), String> {
        if node.kind == "fold.motion.group_instance" {
            crate::nodes::interface::signature(self, node)
        } else {
            registry::find(&node.kind)?.signature(node)
        }
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        Ok(Document {
            id,
            package_id: crate::PACKAGE.into(),
            type_id: MOTION.into(),
            schema_version: if self.scene.is_some() { 4 } else { 2 },
            revision: Revision::default(),
            dependencies: vec![],
            assets: vec![],
            payload: serde_json::to_vec(self).map_err(|e| e.to_string())?,
            extensions: Default::default(),
        })
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        if document.package_id != crate::PACKAGE
            || document.type_id != MOTION
            || ![1, 2, 3, 4].contains(&document.schema_version)
            || !document.dependencies.is_empty()
            || !document.assets.is_empty()
        {
            return Err("unsupported motion document envelope".into());
        }
        let result: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        result.validate()?;
        Ok(result)
    }
}
