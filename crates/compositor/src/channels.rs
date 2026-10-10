//! Authoring choices for channel routing and common effect coverage.
use fold_foundation::ObjectId;
use fold_render::channels::{ChannelName, RGBA};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub id: ObjectId,
    pub protected: bool,
    pub extensions: fold_project::Metadata,
    pub mask: Option<fold_graph::model::Edge>,
    #[serde(default)]
    pub mask_disabled: bool,
    pub mask_channel: ChannelName,
    pub invert: bool,
    pub mix: f32,
    pub channels: Vec<ChannelName>,
}
impl Default for Effect {
    fn default() -> Self {
        Self {
            id: ObjectId::new(),
            protected: false,
            extensions: Default::default(),
            mask: None,
            mask_disabled: false,
            mask_channel: ChannelName::try_from("rgba.alpha".to_owned()).unwrap(),
            invert: false,
            mix: 1.,
            channels: RGBA
                .map(|name| ChannelName::try_from(name.to_owned()).unwrap())
                .into(),
        }
    }
}
impl Effect {
    pub(crate) fn for_config(config: &crate::OperationConfig) -> Self {
        let mut effect = Self::default();
        if matches!(config, crate::OperationConfig::ColorGrade { .. }) {
            effect.channels.truncate(3);
        }
        effect
    }
    pub fn mask_node(&self) -> Option<ObjectId> {
        self.mask.as_ref().and_then(|edge| match edge.target {
            fold_graph::model::Reference::Node { node } => Some(node),
            _ => None,
        })
    }
    pub fn set_mask(&mut self, node: ObjectId) {
        let target = fold_graph::model::Reference::Node { node };
        if let Some(edge) = &mut self.mask {
            edge.target = target;
        } else {
            self.mask = Some(target.into());
        }
    }
    pub fn is_default(&self) -> bool {
        self.mask.is_none()
            && !self.mask_disabled
            && self.mask_channel.as_str() == "rgba.alpha"
            && !self.invert
            && self.mix == 1.
            && self.channels.iter().map(|c| c.as_str()).eq(RGBA)
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
        fold_graph::model::extension_keys(
            &self.extensions,
            &[
                "id",
                "protected",
                "extensions",
                "mask",
                "mask_disabled",
                "mask_channel",
                "invert",
                "mix",
                "channels",
            ],
        )?;
        if let Some(edge) = &self.mask {
            edge.validate()?;
            if self.mask_node().is_none() {
                return Err("Effect mask requires a native node connection".into());
            }
        }
        if !self.mix.is_finite() || !(0.0..=1.).contains(&self.mix) || self.channels.len() > 256 {
            return Err("Effect mix must be in 0..1; select at most 256 channels".into());
        }
        Ok(())
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ChannelSource {
    A(ChannelName),
    B(ChannelName),
    Zero,
    One,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChannelMapping {
    pub destination: ChannelName,
    pub source: ChannelSource,
}

impl crate::PreparedComposite<'_> {
    /// Structural channel discovery uses no decoding and is safe for inspectors.
    pub fn channel_names(&self, output: ObjectId) -> Result<Vec<ChannelName>, String> {
        self.channel_names_with(output, &|_| {
            Ok(RGBA.map(|name| name.to_owned().try_into().unwrap()).into())
        })
    }
    pub fn channel_names_with(
        &self,
        output: ObjectId,
        resolve: &fold_platform::ChannelResolver<'_>,
    ) -> Result<Vec<ChannelName>, String> {
        let mut outputs = std::collections::BTreeMap::<ObjectId, Vec<ChannelName>>::new();
        for &id in self.evaluation_order() {
            let node = self.node(id)?;
            let primary = usize::from(matches!(
                node.config(),
                crate::OperationConfig::Composite { .. }
            ));
            let mut names = node
                .input_ids()
                .get(primary)
                .and_then(|id| *id)
                .and_then(|id| outputs.get(&id))
                .cloned()
                .unwrap_or_else(|| {
                    RGBA.map(|name| ChannelName::try_from(name.to_owned()).unwrap())
                        .into()
                });
            match &node.config() {
                crate::OperationConfig::Read {
                    source: crate::Source::Document { source, .. },
                    ..
                } => {
                    names = resolve(source)?;
                }
                crate::OperationConfig::Read {
                    source: crate::Source::Exr { image, .. },
                    ..
                } => {
                    names = image
                        .channels
                        .iter()
                        .map(|channel| channel.name.clone().try_into())
                        .collect::<Result<_, _>>()?;
                    for channel in &image.channels {
                        if channel.color {
                            let layer = channel.name.rsplit_once('.').unwrap().0;
                            let alpha = ChannelName::try_from(format!("{layer}.alpha"))?;
                            if !names.contains(&alpha)
                                && ["red", "green", "blue"].iter().all(|component| {
                                    image
                                        .channels
                                        .iter()
                                        .any(|c| c.name == format!("{layer}.{component}"))
                                })
                            {
                                names.push(alpha);
                            }
                        }
                    }
                }

                crate::OperationConfig::Shuffle { mappings } => {
                    names.extend(mappings.iter().map(|mapping| mapping.destination.clone()));
                }
                crate::OperationConfig::RemoveChannels { channels, keep } => {
                    names.retain(|name| channels.contains(name) == *keep);
                }
                _ => {}
            }
            names.sort();
            names.dedup();
            outputs.insert(id, names);
        }
        outputs
            .remove(&output)
            .ok_or_else(|| "Missing channel source".into())
    }
}

impl crate::Composite {
    pub fn channel_names(&self, output: ObjectId) -> Result<Vec<ChannelName>, String> {
        self.prepare()?.channel_names(output)
    }
    pub fn channel_names_with(
        &self,
        output: ObjectId,
        resolve: &fold_platform::ChannelResolver<'_>,
    ) -> Result<Vec<ChannelName>, String> {
        self.prepare()?.channel_names_with(output, resolve)
    }
}
