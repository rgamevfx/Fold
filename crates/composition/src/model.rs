use crate::layer::LayerState;
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::VideoInfo;
use fold_platform::image_source::Source;
use fold_project::{Document, Revision};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    ops::{Deref, DerefMut},
};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Layer {
    #[serde(flatten)]
    pub state: LayerState,
    /// None denotes a group. An output reference retains its editable graph.
    pub source: Option<Source>,
    #[serde(default)]
    pub effects: Vec<fold_platform::effects::Instance>,
    #[serde(default)]
    pub adjustment: bool,
    pub info: VideoInfo,
    /// Parent time corresponding to source_start, independent of the trimmed range.
    pub origin: Time,
    pub source_start: Time,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
impl Deref for Layer {
    type Target = LayerState;
    fn deref(&self) -> &LayerState {
        &self.state
    }
}
impl DerefMut for Layer {
    fn deref_mut(&mut self) -> &mut LayerState {
        &mut self.state
    }
}
impl Layer {
    pub fn new(
        name: impl Into<String>,
        source: Option<Source>,
        info: VideoInfo,
    ) -> Result<Self, String> {
        Ok(Self {
            state: LayerState::new(name, info.time(info.frames)?),
            source,
            effects: vec![],
            adjustment: false,
            info,
            origin: Time::ZERO,
            source_start: Time::ZERO,
            extensions: BTreeMap::new(),
        })
    }
    pub fn local_time(&self, time: Time) -> Result<Time, String> {
        time.checked_sub(self.origin)
            .and_then(|t| t.checked_add(self.source_start))
            .map_err(|e| e.to_string())
    }
    pub fn set_range(&mut self, start: Time, end: Time) -> Result<(), String> {
        if start >= end {
            return Err("Layer must have a positive duration".into());
        }
        let delta = start.checked_sub(self.start).map_err(|e| e.to_string())?;
        let moved = end.checked_sub(self.end).map_err(|e| e.to_string())? == delta;
        let origin = if moved {
            self.origin.checked_add(delta).map_err(|e| e.to_string())?
        } else {
            self.origin
        };
        self.start = start;
        self.end = end;
        self.origin = origin;
        Ok(())
    }
}

#[cfg(test)]
mod cutover_tests {
    use super::*;

    #[test]
    fn retired_graph_child_metadata_round_trips_as_an_unknown_extension() {
        let layer = Layer::new(
            "Group",
            None,
            VideoInfo {
                width: 16,
                height: 16,
                rate: [24, 1],
                frames: 24,
            },
        )
        .unwrap();
        let mut value = serde_json::to_value(&layer).unwrap();
        value["graph_children"] = true.into();
        let decoded: Layer = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(decoded.extensions["graph_children"], true);
        assert_eq!(serde_json::to_value(decoded).unwrap(), value);
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Composition {
    pub info: VideoInfo,
    /// Front to back. Parent links define groups; each sibling list follows this order.
    pub layers: Vec<Layer>,
    #[serde(flatten)]
    pub extensions: BTreeMap<String, serde_json::Value>,
}
impl Default for Composition {
    fn default() -> Self {
        Self {
            info: VideoInfo {
                width: 1280,
                height: 720,
                rate: [24, 1],
                frames: 120,
            },
            layers: vec![],
            extensions: BTreeMap::new(),
        }
    }
}
impl Composition {
    pub fn layer(&self, id: ObjectId) -> Result<&Layer, String> {
        self.layers
            .iter()
            .find(|l| l.id == id)
            .ok_or_else(|| "Missing layer".into())
    }
    pub fn layer_mut(&mut self, id: ObjectId) -> Result<&mut Layer, String> {
        self.layers
            .iter_mut()
            .find(|l| l.id == id)
            .ok_or_else(|| "Missing layer".into())
    }
    pub fn locked(&self, id: ObjectId) -> bool {
        let mut next = Some(id);
        for _ in 0..=self.layers.len() {
            let Some(id) = next else {
                return false;
            };
            let Ok(layer) = self.layer(id) else {
                return true;
            };
            if layer.locked {
                return true;
            }
            next = layer.parent;
        }
        true
    }
    pub fn validate(&self) -> Result<(), String> {
        self.info.validate()?;
        self.validate_reusable()?;
        if self.layers.len() > 4096 {
            return Err("Composition layer budget exceeded".into());
        }
        let mut ids = BTreeSet::new();
        for layer in &self.layers {
            layer.state.validate()?;
            fold_platform::effects::validate_stack(&layer.effects)?;
            if layer.adjustment
                && (layer.source.is_some()
                    || self.layers.iter().any(|l| l.parent == Some(layer.id)))
            {
                return Err("Adjustment layers cannot own sources or children".into());
            }
            if layer.adjustment
                && (layer.offset != [1., 0., 0., 1., 0., 0.]
                    || layer
                        .animation
                        .keys()
                        .any(|key| key.starts_with("transform.")))
            {
                return Err("Adjustment layers use containing coordinates; apply transforms inside their effect graph".into());
            }
            layer.info.validate()?;
            if !ids.insert(layer.id) || layer.source_start < Time::ZERO {
                return Err("Duplicate layer or negative source in".into());
            }
            let mut seen = BTreeSet::from([layer.id]);
            let mut parent = layer.parent;
            while let Some(id) = parent {
                if !seen.insert(id) || seen.len() > 64 {
                    return Err("Cyclic or excessively deep layer hierarchy".into());
                }
                let ancestor = self.layer(id)?;
                if ancestor.source.is_some() {
                    return Err("Only groups can contain layers".into());
                }
                parent = ancestor.parent;
            }
            for path in layer.animation.keys() {
                if path != "scene.opacity.0" && !(0..6).any(|i| *path == format!("transform.{i}")) {
                    return Err(format!("Unknown composition animation property: {path}"));
                }
            }
        }
        Ok(())
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        let mut dependencies = vec![];
        let mut assets = BTreeSet::new();
        for layer in &self.layers {
            for effect in &layer.effects {
                if !dependencies.contains(&effect.definition) {
                    dependencies.push(effect.definition.clone());
                }
            }
            match &layer.source {
                Some(Source::Output(reference)) => {
                    if !dependencies.contains(reference) {
                        dependencies.push(reference.clone());
                    }
                }
                Some(Source::Asset(id)) => {
                    assets.insert(*id);
                }
                None => {}
            }
        }
        Ok(Document {
            id,
            package_id: crate::PACKAGE.into(),
            type_id: crate::COMPOSITION.into(),
            schema_version: 3,
            revision: Revision::default(),
            dependencies,
            assets: assets.into_iter().collect(),
            payload: serde_json::to_vec(self).map_err(|e| e.to_string())?,
            extensions: BTreeMap::new(),
        })
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        if document.package_id != crate::PACKAGE
            || document.type_id != crate::COMPOSITION
            || document.schema_version != 3
        {
            return Err("Unsupported composition document".into());
        }
        let value: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        let canonical = value.document(document.id)?;
        if canonical.dependencies != document.dependencies || canonical.assets != document.assets {
            return Err("Composition dependencies do not match its layers".into());
        }
        Ok(value)
    }
}
