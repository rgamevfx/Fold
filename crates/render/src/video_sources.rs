//! Request-local source reuse, after logical/color validation and before demand
//! planning. Provider fragments remain independent; equal video reads share the
//! decoded, color-converted and reconstructed image on both physical backends.
use crate::{ImageOp, RenderGraph, region::Region};
use std::{collections::HashMap, path::Path};

#[derive(PartialEq, Eq, Hash)]
struct Key<'a> {
    path: &'a Path,
    fingerprint: &'a str,
    info: [u32; 5],
    frame: u32,
    space: &'a str,
    window: Option<([u32; 2], Region)>,
}
fn key(op: &ImageOp) -> Option<Key<'_>> {
    let (op, window) = match op {
        ImageOp::WindowedSource {
            source,
            dimensions,
            region,
        } => (source.as_ref(), Some((*dimensions, *region))),
        _ => (op, None),
    };
    let (source, time, space) = match op {
        ImageOp::Video { source, time } => (source, time, fold_color::settings::VIDEO_INPUT),
        ImageOp::VideoInput {
            source,
            time,
            space,
        } => (source, time, space.as_str()),
        _ => return None,
    };
    let info = &source.info;
    Some(Key {
        path: &source.path,
        fingerprint: &source.fingerprint,
        info: [
            info.width,
            info.height,
            info.rate[0],
            info.rate[1],
            info.frames,
        ],
        frame: info.frame_at(*time).ok()?,
        space,
        window,
    })
}

pub(crate) fn reuse(graph: &mut RenderGraph) {
    let aliases = {
        let mut sources = HashMap::new();
        graph
            .nodes
            .iter()
            .enumerate()
            .map(|(id, op)| key(op).map_or(id, |key| *sources.entry(key).or_insert(id)))
            .collect::<Vec<_>>()
    };
    for op in &mut graph.nodes {
        op.remap_inputs(|input| aliases[input]);
    }
    graph.output = aliases[graph.output];
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_foundation::Time;

    fn source() -> ImageOp {
        ImageOp::Video {
            source: fold_media::VideoSource {
                path: "/fixture.mov".into(),
                fingerprint: "fixture".into(),
                info: fold_media::VideoInfo {
                    width: 1920,
                    height: 1080,
                    rate: [24, 1],
                    frames: 100,
                },
            },
            time: Time::ZERO,
        }
    }

    #[test]
    fn shared_reads_retain_all_consumers_and_output() {
        let mut graph = RenderGraph {
            width: 1920,
            height: 1080,
            nodes: vec![
                source(),
                source(),
                ImageOp::Opacity {
                    input: 1,
                    opacity: 0.5,
                },
                ImageOp::Over {
                    foreground: 2,
                    background: 0,
                },
            ],
            output: 3,
        };
        reuse(&mut graph);
        let (needed, uses) = crate::graph::dependencies(&graph);
        assert_eq!(needed, [true, false, true, true]);
        assert_eq!(uses, [2, 0, 1, 0]);
        assert_eq!(graph.nodes[2].inputs().collect::<Vec<_>>(), [0]);
        graph.output = 1;
        reuse(&mut graph);
        assert_eq!(graph.output, 0);
    }

    #[test]
    fn reuse_requires_equal_source_sample_interpretation_and_window() {
        let original = source();
        let ImageOp::Video { source, .. } = &original else {
            unreachable!()
        };
        let mut changed = source.clone();
        changed.fingerprint = "changed".into();
        let mut moved = source.clone();
        moved.path = "/other.mov".into();
        let mut retimed = source.clone();
        retimed.info.rate = [25, 1];
        let reads = vec![
            original.clone(),
            ImageOp::Video {
                source: source.clone(),
                time: Time::new(1, 100).unwrap(),
            },
            ImageOp::Video {
                source: source.clone(),
                time: Time::new(1, 24).unwrap(),
            },
            ImageOp::Video {
                source: changed,
                time: Time::ZERO,
            },
            ImageOp::Video {
                source: moved,
                time: Time::ZERO,
            },
            ImageOp::Video {
                source: retimed,
                time: Time::ZERO,
            },
            ImageOp::VideoInput {
                source: source.clone(),
                time: Time::ZERO,
                space: fold_color::WORKING_SPACE.into(),
            },
            ImageOp::WindowedSource {
                source: Box::new(original.clone()),
                dimensions: [960, 540],
                region: Region::full([960, 540]),
            },
            ImageOp::WindowedSource {
                source: Box::new(original),
                dimensions: [960, 540],
                region: Region {
                    x: 1,
                    y: 0,
                    width: 959,
                    height: 540,
                },
            },
        ];
        let mut graph = RenderGraph {
            width: 1920,
            height: 1080,
            nodes: reads,
            output: 0,
        };
        // Probe aliasing through consumers, without removing the logical reads.
        for input in 0..graph.nodes.len() {
            graph.nodes.push(ImageOp::Opacity {
                input,
                opacity: 0.5,
            });
        }
        reuse(&mut graph);
        let inputs = graph.nodes[9..]
            .iter()
            .map(|op| op.inputs().next().unwrap())
            .collect::<Vec<_>>();
        assert_eq!(inputs, [0, 0, 2, 3, 4, 5, 6, 7, 8]);
    }
}
