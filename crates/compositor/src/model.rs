use crate::OperationConfig;
use fold_foundation::{AssetId, DocumentId, ObjectId};
use fold_media::VideoInfo;
use fold_project::{Document, DocumentRef, Metadata, Revision};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Source {
    Unassigned {
        info: VideoInfo,
    },
    Exr {
        asset: AssetId,
        info: VideoInfo,
        image: fold_media::exr::Info,
        #[serde(default)]
        options: fold_media::exr_sequence::Options,
        #[serde(default)]
        sequence: Option<fold_media::exr_sequence::Summary>,
        #[serde(default)]
        color_layers: Vec<String>,
    },
    Asset {
        asset: AssetId,
        info: VideoInfo,
    },
    Document {
        source: DocumentRef,
        info: VideoInfo,
    },
}
impl Source {
    pub fn info(&self) -> &VideoInfo {
        match self {
            Self::Unassigned { info }
            | Self::Exr { info, .. }
            | Self::Asset { info, .. }
            | Self::Document { info, .. } => info,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub operation: fold_graph::model::Node,
    #[serde(default, skip_serializing_if = "crate::animation::Channels::is_empty")]
    pub animation: crate::animation::Channels,
    /// Fixed, named operator ports; None is a deliberately disconnected socket.
    pub inputs: Vec<Option<fold_graph::model::Edge>>,
    pub effect: crate::Effect,
    #[serde(default)]
    pub position: Option<[f32; 2]>,
    #[serde(default)]
    pub extensions: Metadata,
}
impl Node {
    pub fn input_ids(&self) -> Vec<Option<ObjectId>> {
        self.inputs
            .iter()
            .map(|edge| {
                edge.as_ref().and_then(|edge| match edge.target {
                    fold_graph::model::Reference::Node { node } => Some(node),
                    _ => None,
                })
            })
            .collect()
    }
    pub fn set_input(&mut self, slot: usize, node: ObjectId) -> Result<(), String> {
        let input = self.inputs.get_mut(slot).ok_or("Unknown input slot")?;
        let target = fold_graph::model::Reference::Node { node };
        if let Some(edge) = input {
            edge.target = target;
        } else {
            *input = Some(target.into());
        }
        Ok(())
    }
    pub fn new(parameters: OperationConfig, inputs: Vec<ObjectId>) -> Self {
        Self {
            operation: crate::operation::record(&parameters),
            animation: Default::default(),
            inputs: inputs
                .into_iter()
                .map(|node| Some(fold_graph::model::Reference::Node { node }.into()))
                .collect(),
            effect: crate::Effect::for_config(&parameters),
            position: None,
            extensions: Default::default(),
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Composite {
    pub info: VideoInfo,
    pub nodes: Vec<Node>,
    pub output: ObjectId,
    /// Explicit extension bag retained by edits and persistence.
    #[serde(default)]
    pub extensions: Metadata,
}
impl Composite {
    pub fn validate(&self) -> Result<(), String> {
        self.prepare().map(|_| ())
    }
    pub(crate) fn check_protection(&self, next: &Self) -> Result<(), String> {
        for node in &self.nodes {
            let replacement = next.nodes.iter().find(|n| n.id == node.id);
            if let Some(instance) = node.native_instance()? {
                let next = replacement
                    .map(|n| n.native_instance())
                    .transpose()?
                    .flatten();
                instance.check_protection(next.as_ref())?;
            }
            if node.effect.protected && replacement.map(|n| &n.effect) != Some(&node.effect) {
                return Err("Effect controls are protected".into());
            }
            if node.operation.parameters.values().any(|p| p.protected)
                && replacement.is_none_or(|n| {
                    n.operation.operation != node.operation.operation
                        || n.version != node.version
                        || n.mode != node.mode
                })
            {
                return Err("Protected operation cannot be reinterpreted".into());
            }
            for (name, parameter) in &node.operation.parameters {
                if parameter.protected
                    && replacement.and_then(|n| n.operation.parameters.get(name)) != Some(parameter)
                {
                    return Err(format!("Protected compositor parameter {name}"));
                }
            }
            let prepared = node.prepare()?;
            let config = prepared.config();
            for property in prepared.animated_properties() {
                let protected = if property.id == "mix" && config.operator().supports_effect() {
                    node.effect.protected
                } else {
                    node.operation
                        .parameters
                        .get(config.parameter_for_property(&property.id))
                        .is_some_and(|p| p.protected)
                };
                if protected {
                    let prefix = format!("{}.", property.id);
                    let before: BTreeMap<_, _> = node
                        .animation
                        .bindings()
                        .iter()
                        .filter(|(path, _)| path.starts_with(&prefix))
                        .collect();
                    let after: BTreeMap<_, _> = replacement
                        .into_iter()
                        .flat_map(|n| n.animation.bindings())
                        .filter(|(path, _)| path.starts_with(&prefix))
                        .collect();
                    if before != after {
                        return Err(format!(
                            "Protected compositor control {} cannot change animation",
                            property.id
                        ));
                    }
                }
            }
            let animation = next
                .nodes
                .iter()
                .find(|next| next.id == node.id)
                .map(|node| &node.animation);
            node.animation
                .check_protection(animation.unwrap_or(&Default::default()))?;
        }
        Ok(())
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        let prepared = self.prepare()?;
        let payload = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if payload.len() > 1024 * 1024 {
            return Err("composite exceeds 1 MiB".into());
        }
        Ok(Document {
            id,
            package_id: crate::PACKAGE.into(),
            type_id: crate::COMPOSITE.into(),
            schema_version: 8,
            revision: Revision::default(),
            dependencies: prepared.dependencies(),
            assets: prepared.assets(),
            payload,
            extensions: Metadata::default(),
        })
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        let value = Self::read_document(document)?;
        value.prepare_document(document)?;
        Ok(value)
    }
    pub(crate) fn read_document(document: &Document) -> Result<Self, String> {
        if document.package_id != crate::PACKAGE
            || document.type_id != crate::COMPOSITE
            || document.schema_version != 8
            || document.payload.len() > 1024 * 1024
        {
            return Err("unsupported composite document".into());
        }
        let value: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        Ok(value)
    }
    pub(crate) fn prepare_document(
        &self,
        document: &Document,
    ) -> Result<crate::PreparedComposite<'_>, String> {
        let prepared = self.prepare()?;
        if prepared.dependencies() != document.dependencies || prepared.assets() != document.assets
        {
            return Err("composite dependency declarations disagree with payload".into());
        }
        Ok(prepared)
    }
}
impl crate::PreparedComposite<'_> {
    pub fn dependencies(&self) -> Vec<DocumentRef> {
        let mut refs = Vec::new();
        for node in &self.nodes {
            if let OperationConfig::Read {
                source: Source::Document { source, .. },
                ..
            } = &node.config()
                && !refs.contains(source)
            {
                refs.push(source.clone());
            }
        }
        for node in &self.nodes {
            if let OperationConfig::Instance { instance, .. } = &node.config()
                && !refs.contains(&instance.definition)
            {
                refs.push(instance.definition.clone());
            }
        }
        refs
    }
    fn assets(&self) -> Vec<AssetId> {
        self.nodes
            .iter()
            .filter_map(|n| match n.config() {
                OperationConfig::Read {
                    source: Source::Asset { asset, .. } | Source::Exr { asset, .. },
                    ..
                } => Some(*asset),
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
}
