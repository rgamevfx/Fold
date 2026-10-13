//! Request-local reuse of constant sources and expensive filter results.
//! Run only after validation. Keys preserve float bits and canonical input IDs.
//! Do not share cheap pointwise results: this can disable consumer fusion and
//! extend dozens of full-image lifetimes. External sources/shaders stay explicit.
use crate::{ImageOp, RenderGraph};
use std::collections::HashMap;

#[derive(Hash, PartialEq, Eq)]
enum Key {
    Solid([u32; 4]),
    Rife(usize, usize, u32, u32, u32),
    Blur(usize, u32),
    FilteredBlur(usize, [u32; 2], u8, u8),
}
fn key(op: &ImageOp) -> Option<Key> {
    Some(match *op {
        ImageOp::Rife {
            first,
            second,
            fraction,
            motion_scale,
            cut_threshold,
        } => Key::Rife(
            first,
            second,
            fraction.to_bits(),
            motion_scale,
            cut_threshold.to_bits(),
        ),
        ImageOp::Solid { rgba } => Key::Solid(rgba.map(f32::to_bits)),
        ImageOp::Blur { input, radius } => Key::Blur(input, radius),
        ImageOp::FilteredBlur {
            input,
            size,
            filter,
            edges,
        } => Key::FilteredBlur(input, size.map(f32::to_bits), filter as u8, edges as u8),
        _ => return None,
    })
}
pub(crate) fn pure(graph: &mut RenderGraph) {
    let mut known = HashMap::new();
    let mut aliases = Vec::with_capacity(graph.nodes.len());
    for (id, op) in graph.nodes.iter_mut().enumerate() {
        op.remap_inputs(|input| aliases[input]);
        aliases.push(key(op).map_or(id, |key| *known.entry(key).or_insert(id)));
    }
    graph.output = aliases[graph.output];
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn common_subgraphs_reuse_but_different_parameters_do_not() {
        let solid = ImageOp::Solid {
            rgba: [0.2, 0.1, 0., 1.],
        };
        let mut graph = RenderGraph {
            width: 1,
            height: 1,
            output: 5,
            nodes: vec![
                solid.clone(),
                solid,
                ImageOp::Blur {
                    input: 0,
                    radius: 4,
                },
                ImageOp::Blur {
                    input: 1,
                    radius: 4,
                },
                ImageOp::Blur {
                    input: 1,
                    radius: 5,
                },
                ImageOp::Over {
                    foreground: 2,
                    background: 3,
                },
            ],
        };
        pure(&mut graph);
        assert!(matches!(
            graph.nodes[5],
            ImageOp::Over {
                foreground: 2,
                background: 2
            }
        ));
        graph.output = 4;
        pure(&mut graph);
        assert_eq!(graph.output, 4);
    }
    #[test]
    fn cheap_pointwise_nodes_remain_available_for_consumer_fusion() {
        let mut graph = RenderGraph {
            width: 1,
            height: 1,
            output: 2,
            nodes: vec![
                ImageOp::Solid {
                    rgba: [0.2, 0.1, 0., 1.],
                },
                ImageOp::Opacity {
                    input: 0,
                    opacity: 0.5,
                },
                ImageOp::Opacity {
                    input: 0,
                    opacity: 0.5,
                },
            ],
        };
        pure(&mut graph);
        assert_eq!(graph.output, 2);
    }
    #[test]
    fn signed_zero_is_not_canonicalized() {
        let mut graph = RenderGraph {
            width: 1,
            height: 1,
            output: 1,
            nodes: vec![
                ImageOp::Solid {
                    rgba: [0., 0., 0., 1.],
                },
                ImageOp::Solid {
                    rgba: [-0., 0., 0., 1.],
                },
            ],
        };
        pure(&mut graph);
        assert_eq!(graph.output, 1);
    }
}
