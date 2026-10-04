//! Lower source channel catalogs into lazy render inputs. Unrequested inputs
//! remain unreachable and are never decoded or uploaded by the evaluator.
use fold_render::{
    ImageOp, RenderGraph,
    channels::{Channel, Channels, Interpretation, Value},
};

pub(super) fn exr(
    graph: &mut RenderGraph,
    source: fold_media::exr::Source,
    space: Option<String>,
    color_layers: &[String],
    active: bool,
) -> Result<Channels, String> {
    let mut result = Channels::default();
    let mut assigned = std::collections::BTreeSet::new();
    for channel in &source.info.channels {
        if assigned.contains(&channel.name) {
            continue;
        }
        if active && graph.nodes.len() >= 4096 {
            return Err("Composite exceeds 4096 render operations".into());
        }
        let layer = channel
            .name
            .rsplit_once('.')
            .ok_or("Invalid EXR channel name")?
            .0;
        let names =
            ["red", "green", "blue", "alpha"].map(|component| format!("{layer}.{component}"));
        let color = names[..3].iter().all(|name| {
            source
                .info
                .channels
                .iter()
                .any(|c| &c.name == name && c.color)
        });
        if color && channel.color {
            let selected = names.each_ref().map(|name| {
                source
                    .info
                    .channels
                    .iter()
                    .find(|c| &c.name == name)
                    .map(|c| c.source.clone())
            });
            let id = graph.nodes.len();
            if active {
                graph.nodes.push(ImageOp::Exr {
                    source: source.clone(),
                    channels: selected,
                    space: if color_layers.iter().any(|name| name == layer) {
                        space.clone()
                    } else {
                        None
                    },
                });
            }
            for (component, name) in names.into_iter().enumerate() {
                result.insert(
                    name.clone().try_into()?,
                    Channel {
                        value: if active {
                            Value::Component {
                                image: id,
                                component: component as u8,
                            }
                        } else {
                            Value::Zero
                        },
                        interpretation: if component == 3 {
                            Interpretation::Alpha
                        } else {
                            if color_layers.iter().any(|name| name == layer) {
                                Interpretation::Color
                            } else {
                                Interpretation::Data
                            }
                        },
                    },
                );
                assigned.insert(name);
            }
        } else {
            let id = graph.nodes.len();
            if active {
                graph.nodes.push(ImageOp::Exr {
                    source: source.clone(),
                    channels: [Some(channel.source.clone()), None, None, None],
                    space: None,
                });
            }
            result.insert(
                channel.name.clone().try_into()?,
                Channel {
                    value: if active {
                        Value::Component {
                            image: id,
                            component: 0,
                        }
                    } else {
                        Value::Zero
                    },
                    interpretation: if channel.name.ends_with(".alpha") {
                        Interpretation::Alpha
                    } else {
                        Interpretation::Data
                    },
                },
            );
            assigned.insert(channel.name.clone());
        }
    }
    Ok(result)
}
