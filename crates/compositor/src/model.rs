use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_media::VideoInfo;
use fold_project::{Document, DocumentRef, Metadata, Revision};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortType {
    Image,
    Mask,
    Scalar,
    Color,
    Scene,
}
#[derive(Clone, Copy, Debug)]
pub struct Operator {
    pub id: &'static str,
    pub inputs: &'static [PortType],
    pub output: PortType,
    pub version: u32,
}
use PortType::{Image, Mask};
pub const OPERATORS: &[Operator] = &[
    Operator {
        id: "Read",
        inputs: &[],
        output: Image,
        version: 1,
    },
    Operator {
        id: "Solid",
        inputs: &[],
        output: Image,
        version: 1,
    },
    Operator {
        id: "Transform",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        id: "Crop",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        id: "Blur",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        id: "Grade",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
    Operator {
        id: "Mask",
        inputs: &[],
        output: Mask,
        version: 1,
    },
    Operator {
        id: "ApplyMask",
        inputs: &[Image, Mask],
        output: Image,
        version: 1,
    },
    Operator {
        id: "Merge",
        inputs: &[Image, Image],
        output: Image,
        version: 1,
    },
    Operator {
        id: "Output",
        inputs: &[Image],
        output: Image,
        version: 1,
    },
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Source {
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
            Self::Asset { info, .. } | Self::Document { info, .. } => info,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub enum Parameters {
    Read {
        source: Source,
        start: Time,
        source_start: Time,
        duration: Time,
    },
    Solid {
        rgba: [f32; 4],
    },
    /// Translation and scale in document pixels, plus opacity.
    Transform {
        translate: [f64; 2],
        scale: [f64; 2],
        opacity: f32,
    },
    Crop {
        rect: [u32; 4],
    },
    Blur {
        radius: u32,
    },
    Grade {
        gain: [f32; 3],
    },
    Mask {
        rect: [u32; 4],
    },
    ApplyMask,
    /// Input 0 is foreground, input 1 background.
    Merge,
    Output,
}
impl Parameters {
    pub fn operator(&self) -> &'static Operator {
        &OPERATORS[match self {
            Self::Read { .. } => 0,
            Self::Solid { .. } => 1,
            Self::Transform { .. } => 2,
            Self::Crop { .. } => 3,
            Self::Blur { .. } => 4,
            Self::Grade { .. } => 5,
            Self::Mask { .. } => 6,
            Self::ApplyMask => 7,
            Self::Merge => 8,
            Self::Output => 9,
        }]
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        match self {
            Self::Read {
                source,
                start,
                source_start,
                duration,
            } => {
                let info = source.info();
                info.validate()?;
                if *start < Time::ZERO
                    || *source_start < Time::ZERO
                    || *duration <= Time::ZERO
                    || source_start
                        .checked_add(*duration)
                        .map_err(|e| e.to_string())?
                        > info.time(info.frames)?
                {
                    return Err("Read requires a valid half-open source range".into());
                }
                start.checked_add(*duration).map_err(|e| e.to_string())?;
                if let Source::Document { source, .. } = source
                    && source.output != "video"
                {
                    return Err("Read requires a video output port".into());
                }
            }
            Self::Solid { rgba }
                if rgba.iter().any(|v| !v.is_finite()) || !(0.0..=1.0).contains(&rgba[3]) =>
            {
                return Err("Solid requires finite RGB and alpha in 0..1".into());
            }
            Self::Transform {
                translate,
                scale,
                opacity,
            } if translate
                .iter()
                .chain(scale)
                .any(|v| !v.is_finite() || v.abs() > 1e6)
                || scale.iter().any(|v| v.abs() < 1e-6)
                || !opacity.is_finite()
                || !(0.0..=1.0).contains(opacity) =>
            {
                return Err("Transform requires finite invertible scale and opacity 0..1".into());
            }
            Self::Crop { rect } | Self::Mask { rect } if rect[0] > rect[2] || rect[1] > rect[3] => {
                return Err("invalid rectangle".into());
            }
            Self::Blur { radius } if *radius > 64 => return Err("blur radius exceeds 64".into()),
            Self::Grade { gain }
                if gain
                    .iter()
                    .any(|v| !v.is_finite() || !(0.0..=16.0).contains(v)) =>
            {
                return Err("gain must be finite in 0..16".into());
            }
            _ => {}
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Node {
    pub id: ObjectId,
    pub parameters: Parameters,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub animation: crate::animation::Channels,
    /// Fixed, named operator ports; None is a deliberately disconnected socket.
    pub inputs: Vec<Option<ObjectId>>,
    #[serde(default)]
    pub position: Option<[f32; 2]>,
    #[serde(default)]
    pub extensions: Metadata,
}
impl Node {
    pub fn new(parameters: Parameters, inputs: Vec<ObjectId>) -> Self {
        Self {
            id: ObjectId::new(),
            parameters,
            animation: Default::default(),
            inputs: inputs.into_iter().map(Some).collect(),
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
        self.info.validate()?;
        if self.nodes.is_empty()
            || self.nodes.len() > 512
            || self.info.time(self.info.frames)? > Time::new(600, 1).unwrap()
        {
            return Err("composite requires 1..512 nodes and at most ten minutes".into());
        }
        let mut ports = BTreeMap::new();
        for node in &self.nodes {
            if !matches!(node.parameters, Parameters::Output) {
                ports.insert(node.id, node.parameters.operator().output);
            }
        }
        let mut ids = BTreeSet::new();
        for node in &self.nodes {
            node.parameters.validate()?;
            node.validate_animation()?;
            let op = node.parameters.operator();
            if !ids.insert(node.id)
                || node.inputs.len() != op.inputs.len()
                || node
                    .position
                    .is_some_and(|p| p.iter().any(|v| !v.is_finite() || v.abs() > 1e7))
            {
                return Err(format!(
                    "{}: duplicate identity or wrong input count",
                    op.id
                ));
            }
            for (input, expected) in node.inputs.iter().zip(op.inputs) {
                if let Some(input) = input
                    && ports.get(input) != Some(expected)
                {
                    return Err(format!(
                        "{}: input requires {expected:?} (missing or incompatible connection)",
                        op.id
                    ));
                }
            }
        }
        if !self
            .nodes
            .iter()
            .any(|n| n.id == self.output && matches!(n.parameters, Parameters::Output))
        {
            return Err("composite output must identify an Output node".into());
        }
        self.evaluation_order()?;
        Ok(())
    }
    pub fn dependencies(&self) -> Vec<DocumentRef> {
        let mut refs = Vec::new();
        for node in &self.nodes {
            if let Parameters::Read {
                source: Source::Document { source, .. },
                ..
            } = &node.parameters
                && !refs.contains(source)
            {
                refs.push(source.clone());
            }
        }
        refs
    }
    fn assets(&self) -> Vec<AssetId> {
        self.nodes
            .iter()
            .filter_map(|n| match n.parameters {
                Parameters::Read {
                    source: Source::Asset { asset, .. },
                    ..
                } => Some(asset),
                _ => None,
            })
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect()
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate()?;
        let payload = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if payload.len() > 1024 * 1024 {
            return Err("composite exceeds 1 MiB".into());
        }
        Ok(Document {
            id,
            package_id: crate::PACKAGE.into(),
            type_id: crate::COMPOSITE.into(),
            schema_version: 3,
            revision: Revision::default(),
            dependencies: self.dependencies(),
            assets: self.assets(),
            payload,
            extensions: Metadata::default(),
        })
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        if document.package_id != crate::PACKAGE
            || document.type_id != crate::COMPOSITE
            || ![1, 2, 3].contains(&document.schema_version)
            || document.payload.len() > 1024 * 1024
        {
            return Err("unsupported composite document".into());
        }
        let value: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        value.validate()?;
        if value.dependencies() != document.dependencies || value.assets() != document.assets {
            return Err("composite dependency declarations disagree with payload".into());
        }
        Ok(value)
    }
}
