//! A single-use opacity can be evaluated at its over consumer. No authoring or
//! logical graph changes: all logical nodes are still validated before planning.
use crate::{ImageOp, RenderGraph};

pub(super) fn opacity_over(
    graph: &RenderGraph,
    needed: &mut [bool],
    uses: &mut [usize],
) -> Vec<Option<(usize, f32)>> {
    let mut fused = vec![None; graph.nodes.len()];
    for (id, op) in graph.nodes.iter().enumerate() {
        if !needed[id] {
            continue;
        }
        if let ImageOp::Over { foreground, .. } = *op
            && foreground != graph.output
            && uses[foreground] == 1
            && let ImageOp::Opacity { input, opacity } = graph.nodes[foreground]
        {
            fused[id] = Some((input, opacity));
            needed[foreground] = false;
            uses[foreground] = 0;
            // The opacity's input retains one read, now at the over pass. Its
            // lifetime extends to that consumer even with intervening nodes.
        }
    }
    fused
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_single_use_reachable_opacity_is_fused() {
        let mut graph = RenderGraph {
            width: 1,
            height: 1,
            nodes: vec![
                ImageOp::Solid { rgba: [0.5; 4] },
                ImageOp::Opacity {
                    input: 0,
                    opacity: 0.25,
                },
                ImageOp::Solid { rgba: [0.; 4] },
                ImageOp::Over {
                    foreground: 1,
                    background: 2,
                },
            ],
            output: 3,
        };
        let (mut needed, mut uses) = crate::graph::dependencies(&graph);
        let fused = opacity_over(&graph, &mut needed, &mut uses);
        assert_eq!(fused[3], Some((0, 0.25)));
        assert_eq!(needed, [true, false, true, true]);
        assert_eq!(uses, [1, 0, 1, 0]);
        graph.nodes.push(ImageOp::Over {
            foreground: 3,
            background: 1,
        });
        graph.output = 4;
        let (mut needed, mut uses) = crate::graph::dependencies(&graph);
        assert!(
            opacity_over(&graph, &mut needed, &mut uses)
                .iter()
                .all(Option::is_none)
        );
        assert!(needed[1]);
        graph.output = 1;
        let (mut needed, mut uses) = crate::graph::dependencies(&graph);
        assert!(
            opacity_over(&graph, &mut needed, &mut uses)
                .iter()
                .all(Option::is_none)
        );
        assert!(needed[1]);
    }
}
