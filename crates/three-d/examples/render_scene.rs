//! Native GPU qualification through the new package's public video capability.
//! Explicit working-image readback is confined to this qualification example.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_graph::{
    binding::{Binding, Track},
    types::{Literal, Space, Unit},
};
use fold_media::{Cancel, Decoder, VideoInfo};
use fold_platform::packages::PackageRegistry;
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use fold_render::gpu::{Host, Renderer};
use fold_three_d::{model::Scene, package};
use std::{io::Write, path::PathBuf, time::Instant};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args_os()
            .nth(1)
            .ok_or("Expected output directory")?,
    );
    let copies: usize = std::env::args()
        .nth(2)
        .map_or(Ok(1), |value| value.parse())?;
    if !(1..=256).contains(&copies) {
        return Err("Expected 1..=256 shared-geometry copies".into());
    }
    std::fs::create_dir_all(&directory)?;
    let mut registry = PackageRegistry::default();
    package::register(&mut registry)?;
    let id = DocumentId::new();
    let mut scene = Scene::new(VideoInfo {
        width: 320,
        height: 180,
        rate: [24000, 1001],
        frames: 48,
    });
    let object = scene.roots[0];
    scene
        .objects
        .get_mut(&object)
        .ok_or("Missing box")?
        .transform
        .rotation
        .value = serde_json::to_value(Binding::Literal {
        value: Literal::Rotation {
            value: fold_foundation::rotation::normalize([-0.2, 0.3, 0., 1.]),
            space: Space::World,
        },
    })?;
    let mut track = Track::constant(
        Literal::Vector {
            value: vec![0.; 3],
            unit: Unit::Meters,
            space: Space::World,
        },
        Time::ZERO,
    )?;
    let mut end = track.keys[0].clone();
    end.id = ObjectId::new();
    end.time = Time::new(1, 1)?;
    end.value = Literal::Vector {
        value: vec![0.7, 0., 0.],
        unit: Unit::Meters,
        space: Space::World,
    };
    track.keys.push(end);
    scene
        .objects
        .get_mut(&object)
        .ok_or("Missing box")?
        .transform
        .position
        .value = serde_json::to_value(Binding::Track { track })?;
    let cancel = Cancel::default();
    let mut instances = vec![object];
    for _ in 1..copies {
        let (id, edits) = fold_three_d::tools::duplicate(&scene, object)?;
        scene = fold_three_d::editing::apply(&scene, &edits, &cancel)?;
        instances.push(id);
    }
    if copies > 1 {
        let columns = (copies as f64).sqrt().ceil() as usize;
        let rows = copies.div_ceil(columns);
        for (index, id) in instances.iter().enumerate() {
            let object = scene.objects.get_mut(id).ok_or("Missing copy")?;
            let mut position = object.transform.position.binding()?;
            let Binding::Track { track } = &mut position else {
                return Err("Copy lost its position track".into());
            };
            for key in &mut track.keys {
                let Literal::Vector { value, .. } = &mut key.value else {
                    return Err("Copy has invalid position key".into());
                };
                value[0] += (index % columns) as f64 * 0.4 - (columns - 1) as f64 * 0.2;
                value[1] += (index / columns) as f64 * 0.4 - (rows - 1) as f64 * 0.2;
            }
            object.transform.position.value = serde_json::to_value(position)?;
            object.transform.scale.value = serde_json::to_value(Binding::Literal {
                value: Literal::Vector {
                    value: vec![0.25; 3],
                    unit: Unit::Unitless,
                    space: Space::World,
                },
            })?;
        }
        assert_eq!(scene.geometry.len(), 1);
        assert_eq!(scene.materials.len(), 1);
    }
    let mut project = Project::new(8);
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![Mutation::PutDocument(scene.document(id)?)],
    })?;
    let snapshot = project.snapshot();
    fold_project::save(&snapshot, directory.join("scene.fold"))?;
    let reference = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let (host, adapter) = pollster::block_on(Host::headless(256 * 1024 * 1024))?;
    let mut renderer = Renderer::new(host.clone())?;
    let mut decoder = Decoder::default();
    let mut frames = vec![];
    let mut evidence = vec![];
    for (index, time) in [Time::ZERO, Time::new(1, 2)?, Time::new(1, 1)?]
        .into_iter()
        .enumerate()
    {
        let graph = registry.video_with(&snapshot, &reference, time, [320, 180], &cancel)?;
        let start = Instant::now();
        let mut frame = renderer.evaluate(graph, None, &mut decoder, &cancel)?;
        assert_eq!(frame.statistics.cpu_adapter_nodes, 0);
        assert_eq!(frame.statistics.readback_bytes, 0);
        let pixels = frame.readback(&cancel)?;
        let values = pixels.pixels();
        assert_eq!(values.len(), 320 * 180);
        assert!(values.iter().flatten().all(|value| value.is_finite()));
        assert!(values.iter().all(|pixel| (0. ..=1.).contains(&pixel[3])));
        let coverage = values.iter().filter(|pixel| pixel[3] > 0.01).count();
        let color_max = values
            .iter()
            .flat_map(|pixel| pixel[..3].iter())
            .copied()
            .fold(0_f32, f32::max);
        assert!(
            coverage > 100 && color_max > 1e-6,
            "Empty or unlit scene output"
        );
        let mut output = std::io::BufWriter::new(std::fs::File::create(
            directory.join(format!("frame-{index}.rgba32f")),
        )?);
        for pixel in values {
            for channel in pixel {
                output.write_all(&channel.to_le_bytes())?;
            }
        }
        output.flush()?;
        evidence.push(
            serde_json::json!({ "time": time,"coverage_pixels": coverage,"maximum_rgb": color_max,
            "cpu_raster_adapters": 0,"working_readback_before_qualification": 0,
            "evaluation_and_qualification_ms": start.elapsed().as_secs_f64()*1000. }),
        );
        frames.push(values.to_vec());
    }
    let difference = frames[0]
        .iter()
        .zip(&frames[2])
        .filter(|(a, b)| a.iter().zip(b.iter()).any(|(x, y)| (x - y).abs() > 0.001))
        .count();
    assert!(difference > 100, "Animation did not change GPU pixels");
    std::fs::write(
        directory.join("evidence.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "adapter": adapter.name,"dimensions": [320,180],"package_video_graph": true,"frames": evidence,
            "instances": copies,"geometry_resources":scene.geometry.len(),"material_resources":scene.materials.len(),
            "animation_changed_pixels": difference,"artist_qualified": false,"application_registered": false,
            "performance_qualified": false,
        }))?,
    )?;
    Ok(())
}
