//! Authoring choices for channel routing and common effect coverage.
use fold_foundation::ObjectId;
use fold_render::channels::{ChannelName, RGBA};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Effect {
    pub mask: Option<ObjectId>,
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
    pub fn is_default(&self) -> bool {
        self == &Self::default()
    }
    pub(crate) fn validate(&self) -> Result<(), String> {
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

impl crate::Composite {
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
        for id in self.evaluation_order()? {
            let node = self.node(id)?;
            let primary = usize::from(matches!(
                node.parameters,
                crate::Parameters::Merge | crate::Parameters::Composite { .. }
            ));
            let mut names = node
                .inputs
                .get(primary)
                .and_then(|id| *id)
                .and_then(|id| outputs.get(&id))
                .cloned()
                .unwrap_or_else(|| {
                    RGBA.map(|name| ChannelName::try_from(name.to_owned()).unwrap())
                        .into()
                });
            match &node.parameters {
                crate::Parameters::Read {
                    source: crate::Source::Document { source, .. },
                    ..
                } => {
                    names = resolve(source)?;
                }
                crate::Parameters::Read {
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

                crate::Parameters::Shuffle { mappings } => {
                    names.extend(mappings.iter().map(|mapping| mapping.destination.clone()));
                }
                crate::Parameters::RemoveChannels { channels, keep } => {
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
