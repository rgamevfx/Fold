//! Ordered image passes from one evaluation. Screen entries delimit spatial
//! passes: World and Camera Plane entries within one run share depth; separate runs composite Over.
use super::{Error, Evaluated, Limits, conversion_step as step, evaluate, prepare_spatial, screen};
use crate::{evaluation, execution::Budget};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::Snapshot;
use fold_render::{ImageOp, RenderGraph};
use std::sync::Arc;

/// Compile Screen/World paint order without re-evaluating the native construction.
/// Each contiguous spatial run is one depth-tested pass with the active camera and
/// scene lights. A Screen run composites in authored order and resets depth for
/// the next spatial run. Camera Plane entries share depth with World entries.
pub fn image_with_services(
    snapshot: &Snapshot,
    document: DocumentId,
    output: ObjectId,
    time: Time,
    dimensions: [u32; 2],
    limits: Limits,
    services: super::Services<'_>,
) -> Result<RenderGraph, Error> {
    let cancellation = services.cancellation;
    let evaluated = evaluate(
        snapshot,
        document,
        output,
        time,
        dimensions,
        limits.evaluation,
        services,
    )?;
    let mut budget = Budget::new(limits.conversion_steps, cancellation);
    let is_world = |entry: &evaluation::SceneEntry| {
        matches!(
            entry.placement.transform,
            evaluation::placement::Transform::World(_)
                | evaluation::placement::Transform::CameraPlane(_)
        )
    };
    let mut nodes = vec![];
    let mut output = 0;
    let mut start = 0;
    while start < evaluated.native.entries.len() {
        step(&mut budget)?;
        // At most 2048 passes fit the shared graph's 4096-node bound.
        if nodes.len() >= 4095 {
            return Err(Error::Budget);
        }
        let world = is_world(&evaluated.native.entries[start]);
        let mut end = start;
        while end < evaluated.native.entries.len()
            && is_world(&evaluated.native.entries[end]) == world
        {
            step(&mut budget)?;
            end += 1;
        }
        for _ in &evaluated.native.lights {
            step(&mut budget)?;
        }
        let pass = Evaluated {
            native: Arc::new(evaluation::Scene {
                entries: evaluated.native.entries[start..end].to_vec(),
                camera: evaluated.native.camera.clone(),
                cameras: vec![],
                lights: evaluated.native.lights.clone(),
            }),
            duration: evaluated.duration,
            canvas: evaluated.canvas,
        };
        let source = if world {
            let prepared =
                prepare_spatial(pass, snapshot, document, time, dimensions, &mut budget)?;
            ImageOp::Spatial {
                scene: Arc::new(prepared.scene),
                binding: prepared.binding,
            }
        } else {
            ImageOp::Vector(screen::prepare(&pass, dimensions, &mut budget)?)
        };
        let foreground = nodes.len();
        nodes.push(source);
        if foreground != 0 {
            nodes.push(ImageOp::Over {
                foreground,
                background: output,
            });
        }
        output = nodes.len() - 1;
        start = end;
    }
    if nodes.is_empty() {
        nodes.push(ImageOp::Solid { rgba: [0.; 4] });
    }
    step(&mut budget)?;
    Ok(RenderGraph {
        width: dimensions[0],
        height: dimensions[1],
        nodes,
        output,
    })
}

pub fn image(
    snapshot: &Snapshot,
    document: DocumentId,
    output: ObjectId,
    time: Time,
    dimensions: [u32; 2],
    limits: Limits,
    cancellation: &dyn Fn() -> Result<(), String>,
) -> Result<RenderGraph, Error> {
    image_with_services(
        snapshot,
        document,
        output,
        time,
        dimensions,
        limits,
        super::Services {
            cancellation,
            images: None,
        },
    )
}
