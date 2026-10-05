//! Authored scene organization. Node identity is retained across scene and network edits.
pub mod animation;
mod relationships;
use crate::{Motion, graph::Link};
use fold_foundation::{ObjectId, Time};
pub use relationships::*;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Scene {
    /// Front to back, matching the artist-facing layer list.
    pub objects: Vec<Object>,
    #[serde(default)]
    pub output: Option<ObjectId>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Object {
    pub id: ObjectId,
    pub name: String,
    #[serde(default)]
    pub parent: Option<ObjectId>,
    pub source: Link,
    pub output: Link,
    pub transform: Option<ObjectId>,
    pub appearance: Option<ObjectId>,
    pub modifiers: Vec<ObjectId>,
    #[serde(default)]
    pub animation: animation::Channels,
    #[serde(default = "identity")]
    pub offset: [f64; 6],
    #[serde(default = "one")]
    pub opacity: f64,
    #[serde(default)]
    pub isolated: bool,
    #[serde(default)]
    pub masks: Vec<Mask>,
    #[serde(default)]
    pub constraints: Vec<Constraint>,
    pub visible: bool,
    pub locked: bool,
    pub start: Time,
    pub end: Time,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
fn identity() -> [f64; 6] {
    crate::geometry::IDENTITY
}
fn one() -> f64 {
    1.
}
impl Object {
    pub fn owns(&self, node: ObjectId) -> bool {
        self.id == node
            || self.transform == Some(node)
            || self.appearance == Some(node)
            || self.modifiers.contains(&node)
            || self.masks.iter().any(|m| m.id == node)
            || self.constraints.iter().any(|c| c.id == node)
    }
}
impl Scene {
    pub fn validate(&self, motion: &Motion) -> Result<(), String> {
        if self
            .output
            .is_some_and(|id| !self.objects.iter().any(|o| o.id == id))
        {
            return Err("missing scene output object".into());
        }
        let mut ids = BTreeSet::new();
        for object in &self.objects {
            if !ids.insert(object.id) || object.start >= object.end {
                return Err("duplicate scene object or invalid active range".into());
            }
            for link in [&object.source, &object.output] {
                let node = motion.graph.node(link.node)?;
                if motion.signature(node).is_ok_and(|(_, outputs)| {
                    !outputs.iter().any(|(name, kind)| {
                        name == &link.socket && *kind == crate::fields::Kind::Content
                    })
                }) {
                    return Err("scene objects require a content output".into());
                }
            }
            for id in object
                .modifiers
                .iter()
                .chain(object.transform.iter())
                .chain(object.appearance.iter())
            {
                motion.graph.node(*id)?;
            }
        }
        for object in &self.objects {
            let mut parent = object.parent;
            let mut visited = BTreeSet::from([object.id]);
            while let Some(id) = parent {
                if !visited.insert(id) {
                    return Err("cyclic scene parenting".into());
                }
                let ancestor = self
                    .objects
                    .iter()
                    .find(|o| o.id == id)
                    .ok_or("missing parent object")?;
                if motion.graph.node(ancestor.source.node)?.kind != "fold.motion.scene_children" {
                    return Err("parent must be a scene group".into());
                }
                parent = ancestor.parent;
            }
        }
        relationships::validate(self)?;
        validate_dependencies(motion, self)
    }
}

/// Walk reachable node dependencies, including group bodies and scene references.
/// This catches cross-object cycles before a transaction can be committed.
fn validate_dependencies(motion: &Motion, scene: &Scene) -> Result<(), String> {
    struct Walk<'a> {
        motion: &'a Motion,
        scene: &'a Scene,
        active: BTreeSet<ObjectId>,
        done: BTreeSet<ObjectId>,
    }
    impl Walk<'_> {
        fn object(&mut self, id: ObjectId) -> Result<(), String> {
            if self.done.contains(&id) {
                return Ok(());
            }
            if self.active.len() >= 64 || !self.active.insert(id) {
                return Err("cyclic scene object reference".into());
            }
            let object = self
                .scene
                .objects
                .iter()
                .find(|o| o.id == id)
                .ok_or("missing referenced scene object")?;
            for target in object.masks.iter().map(|m| m.source).chain(
                object
                    .constraints
                    .iter()
                    .filter(|c| c.kind == ConstraintKind::Path)
                    .map(|c| c.target),
            ) {
                self.object(target)?;
            }
            self.graph(
                &self.motion.graph,
                object.output.node,
                &mut BTreeSet::new(),
                0,
            )?;
            self.active.remove(&id);
            self.done.insert(id);
            Ok(())
        }
        fn graph(
            &mut self,
            graph: &crate::graph::Graph,
            id: ObjectId,
            visited: &mut BTreeSet<ObjectId>,
            depth: usize,
        ) -> Result<(), String> {
            if depth > 128 {
                return Err("scene dependency depth exceeds limit".into());
            }
            if !visited.insert(id) {
                return Ok(());
            }
            let node = graph.node(id)?;
            if node.kind == "fold.motion.object_reference"
                && let Some(value) = node.settings.get("object")
            {
                self.object(
                    serde_json::from_value(value.clone())
                        .map_err(|e| format!("invalid object reference: {e}"))?,
                )?;
            }
            if node.kind == "fold.motion.scene_children" {
                let parent: ObjectId = serde_json::from_value(node.settings["object"].clone())
                    .map_err(|e| e.to_string())?;
                for child in &self.scene.objects {
                    if child.parent == Some(parent) {
                        self.object(child.id)?;
                    }
                }
            }
            if node.kind == "fold.motion.group_instance"
                && let Some(group) = node
                    .settings::<crate::nodes::interface::GroupSettings>()?
                    .group
            {
                let group = self.motion.groups.get(&group).ok_or("missing node group")?;
                for link in group.outputs.values() {
                    self.graph(&group.graph, link.node, &mut BTreeSet::new(), depth + 1)?;
                }
            }
            for input in node.inputs.values() {
                if let crate::graph::Input::Link(link) = input {
                    self.graph(graph, link.node, visited, depth + 1)?;
                }
            }
            Ok(())
        }
    }
    let mut walk = Walk {
        motion,
        scene,
        active: BTreeSet::new(),
        done: BTreeSet::new(),
    };
    for object in &scene.objects {
        walk.object(object.id)?;
    }
    Ok(())
}
