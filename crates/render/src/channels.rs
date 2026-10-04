//! Named image channels are logical references, not additional pixel allocations.
//! Repacking happens only when an operation or consumer requests a pixel tuple.
use crate::{ImageId, ImageOp, RenderGraph};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

pub const RGBA: [&str; 4] = ["rgba.red", "rgba.green", "rgba.blue", "rgba.alpha"];

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Interpretation {
    Color,
    Alpha,
    Data,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct ChannelName(String);
impl ChannelName {
    pub fn as_str(&self) -> &str {
        &self.0
    }
    pub fn layer(&self) -> &str {
        self.0.rsplit_once('.').unwrap().0
    }
}
impl TryFrom<String> for ChannelName {
    type Error = String;
    fn try_from(value: String) -> Result<Self, Self::Error> {
        if value.len() > 255
            || value.chars().any(char::is_control)
            || value.split('.').any(|part| part.trim().is_empty())
            || !value.contains('.')
        {
            return Err(
                "channel names require a nonempty layer and component (for example depth.Z)".into(),
            );
        }
        Ok(Self(value))
    }
}
impl From<ChannelName> for String {
    fn from(value: ChannelName) -> Self {
        value.0
    }
}
impl std::fmt::Display for ChannelName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Component { image: ImageId, component: u8 },
    Zero,
    One,
}
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Channel {
    pub value: Value,
    pub interpretation: Interpretation,
}
#[derive(Clone, Debug, Default)]
pub struct Channels(BTreeMap<ChannelName, Channel>);
impl Channels {
    pub fn rgba(image: ImageId) -> Self {
        Self(
            RGBA.into_iter()
                .enumerate()
                .map(|(component, name)| {
                    (
                        ChannelName::try_from(name.to_owned()).unwrap(),
                        Channel {
                            value: Value::Component {
                                image,
                                component: component as u8,
                            },
                            interpretation: if component == 3 {
                                Interpretation::Alpha
                            } else {
                                Interpretation::Color
                            },
                        },
                    )
                })
                .collect(),
        )
    }
    pub fn get(&self, name: &str) -> Result<Channel, String> {
        let name = ChannelName::try_from(name.to_owned())?;
        self.0
            .get(&name)
            .copied()
            .ok_or_else(|| format!("Missing channel '{name}'"))
    }
    pub fn insert(&mut self, name: ChannelName, channel: Channel) {
        self.0.insert(name, channel);
    }
    pub fn iter(&self) -> impl Iterator<Item = (&ChannelName, &Channel)> {
        self.0.iter()
    }
    pub fn retain(&mut self, names: &[ChannelName], keep: bool) {
        self.0.retain(|name, _| names.contains(name) == keep);
    }
    pub fn select(
        &self,
        graph: &mut RenderGraph,
        selection: &crate::view::Selection,
    ) -> Result<ImageId, String> {
        let mut values = [Value::Zero, Value::Zero, Value::Zero, Value::One];
        for (i, name) in selection.iter().enumerate() {
            if let Some(name) = name {
                values[i] = self.get(name.as_str())?.value;
            }
        }
        pack(graph, values)
    }
    pub fn pack(&self, graph: &mut RenderGraph, names: [&str; 4]) -> Result<ImageId, String> {
        let values = names.map(|name| self.get(name).map(|channel| channel.value));
        let [a, b, c, d] = values;
        pack(graph, [a?, b?, c?, d?])
    }
}

/// Gather from any number of source images using the shared CPU/GPU Shuffle
/// operator. Direct tuples reuse their original image without a copy/pass.
pub fn pack(graph: &mut RenderGraph, values: [Value; 4]) -> Result<ImageId, String> {
    let mut sources = Vec::new();
    for value in values {
        if let Value::Component { image, component } = value {
            if image >= graph.nodes.len() || component > 3 {
                return Err("invalid channel image/component reference".into());
            }
            if !sources.contains(&image) {
                sources.push(image);
            }
        }
    }
    if sources.len() == 1
        && values
            == std::array::from_fn(|component| Value::Component {
                image: sources[0],
                component: component as u8,
            })
    {
        return Ok(sources[0]);
    }
    if graph.nodes.len() + if sources.is_empty() { 2 } else { sources.len() } > 4096 {
        return Err("Channel packing exceeds 4096 render operations".into());
    }
    if sources.is_empty() {
        let source = graph.nodes.len();
        graph.nodes.push(ImageOp::Solid {
            rgba: [0., 0., 0., 1.],
        });
        let result = graph.nodes.len();
        graph.nodes.push(ImageOp::Shuffle {
            first: source,
            second: None,
            mapping: values.map(|value| if value == Value::One { 9 } else { 8 }),
        });
        return Ok(result);
    }
    let mut result = sources[0];
    let mut mapping = values.map(|value| match value {
        Value::Component { image, component } if image == result => component,
        Value::One => 9,
        _ => 8,
    });
    for (index, source) in sources.iter().copied().enumerate() {
        if index > 0 {
            mapping = std::array::from_fn(|c| match values[c] {
                Value::Component { image, component } if image == source => 4 + component,
                _ => c as u8,
            });
        }
        let next = graph.nodes.len();
        graph.nodes.push(ImageOp::Shuffle {
            first: result,
            second: (index > 0).then_some(source),
            mapping,
        });
        result = next;
    }
    Ok(result)
}

/// A lazy image with named outputs. It becomes a renderable graph only when a
/// consumer selects channels. Nesting transfers references, never pixel buffers.
#[derive(Clone, Debug)]
pub struct ChannelGraph {
    pub graph: RenderGraph,
    pub channels: Channels,
}
impl ChannelGraph {
    pub fn rgba(graph: RenderGraph) -> Self {
        let channels = Channels::rgba(graph.output);
        Self { graph, channels }
    }
    pub fn select(
        mut self,
        selection: Option<&crate::view::Selection>,
    ) -> Result<RenderGraph, String> {
        self.graph.output = match selection {
            Some(selection) => self.channels.select(&mut self.graph, selection)?,
            None => self.channels.pack(&mut self.graph, RGBA)?,
        };
        Ok(self.graph)
    }
    pub fn append_to(mut self, graph: &mut RenderGraph) -> Result<Channels, String> {
        if [graph.width, graph.height] != [self.graph.width, self.graph.height] {
            return Err("Nested image dimensions differ".into());
        }
        let count = self.graph.nodes.len();
        let offset = graph.nodes.len();
        for channel in self.channels.0.values_mut() {
            if let Value::Component { image, component } = &mut channel.value {
                if *image >= count || *component > 3 {
                    return Err("Invalid nested channel reference".into());
                }
                *image += offset;
            }
        }
        if count > 0 {
            // Named outputs, rather than RenderGraph::output, are authoritative.
            self.graph.output = 0;
            graph.append(self.graph)?;
        }
        Ok(self.channels)
    }
}
