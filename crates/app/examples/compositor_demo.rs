//! Generate a self-contained multilayer compositor project for hands-on review.
use exr::prelude::*;
use fold_app::{media_workflow, packages};
use fold_compositor::{ChannelMapping, ChannelSource, Composite, Node, Parameters as P, Source};
use fold_foundation::{DocumentId, Time};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use fold_render::operations::{Edges, Grade, MergeMode};
use std::path::Path;
use std::result::Result;

const WIDTH: usize = 640;
const HEIGHT: usize = 360;

fn source(path: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let mut beauty = [vec![], vec![], vec![], vec![]];
    let mut depth = Vec::new();
    let mut matte = Vec::new();
    for y in 0..HEIGHT {
        for x in 0..WIDTH {
            let mut pixel = [0.; 4];
            let mut z = 1.;
            let mut subject = 0.;
            for (index, (cx, cy, radius, color)) in [
                (175., 185., 100., [0.03, 0.65, 0.9]),
                (340., 145., 92., [1.0, 0.23, 0.035]),
                (470., 215., 75., [0.4, 0.08, 0.85]),
            ]
            .into_iter()
            .enumerate()
            {
                let nx = (x as f32 - cx) / radius;
                let ny = (y as f32 - cy) / radius;
                let r2 = nx * nx + ny * ny;
                if r2 < 1. {
                    let nz = (1. - r2).sqrt();
                    let coverage = ((1. - r2.sqrt()) * radius).clamp(0., 1.);
                    let diffuse = (-0.35 * nx - 0.5 * ny + 0.8 * nz).max(0.);
                    let specular = (-0.25 * nx - 0.4 * ny + 0.88 * nz).max(0.).powf(48.);
                    pixel = [
                        (color[0] * (0.12 + diffuse) + specular) * coverage,
                        (color[1] * (0.12 + diffuse) + specular) * coverage,
                        (color[2] * (0.12 + diffuse) + specular) * coverage,
                        coverage,
                    ];
                    z = 0.2 + index as f32 * 0.18 + (1. - nz) * 0.15;
                    subject = if index == 1 { coverage } else { 0. };
                }
            }
            for (plane, value) in beauty.iter_mut().zip(pixel) {
                plane.push(value);
            }
            depth.push(z);
            matte.push(subject);
        }
    }
    let mut channels: Vec<_> = ["R", "G", "B", "A"]
        .into_iter()
        .zip(beauty.iter())
        .map(|(name, samples)| AnyChannel::new(name, FlatSamples::F32(samples.clone())))
        .collect();
    channels.push(AnyChannel::new("Z", FlatSamples::F32(depth)));
    channels.push(AnyChannel::new("matte.subject", FlatSamples::F32(matte)));
    for layer in 0..20 {
        for (component, name) in ["R", "G", "B"].into_iter().enumerate() {
            let samples = beauty[(component + layer) % 3]
                .iter()
                .map(|value| value * (0.35 + layer as f32 / 24.))
                .collect();
            channels.push(AnyChannel::new(
                format!("aov{layer:02}.{name}").as_str(),
                FlatSamples::F32(samples),
            ));
        }
    }
    Image::from_layer(Layer::new(
        (WIDTH, HEIGHT),
        LayerAttributes::default(),
        Encoding {
            compression: Compression::ZIP16,
            blocks: Blocks::ScanLines,
            line_order: LineOrder::Increasing,
        },
        AnyChannels::sort(channels.into_iter().collect()),
    ))
    .write()
    .non_parallel()
    .to_file(path)?;
    Ok(())
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::args().nth(1).ok_or("Pass a new demo directory")?;
    let directory = Path::new(&directory);
    std::fs::create_dir(directory)?;
    let directory = directory.canonicalize()?;
    let image = directory.join("twenty-aovs.exr");
    source(&image)?;
    let info = fold_media::VideoInfo {
        width: WIDTH as u32,
        height: HEIGHT as u32,
        rate: [24, 1],
        frames: 120,
    };
    let read = Node::new(
        P::Read {
            source: Source::Unassigned { info: info.clone() },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(5, 1)?,
        },
        vec![],
    );
    let read_id = read.id;
    let shape = Node::new(
        P::Shape {
            shape: fold_compositor::Shape::Ellipse,
            bounds: [235., 45., 435., 245.],
        },
        vec![],
    );
    let mask = Node::new(
        P::GaussianBlur {
            size: [16.; 2],
            edges: Edges::Transparent,
        },
        vec![shape.id],
    );
    let mut grade = Node::new(
        P::ColorGrade {
            settings: Grade {
                multiply: [1.1, 1.3, 1.0],
                ..Default::default()
            },
        },
        vec![read.id],
    );
    grade.effect.mask = Some(mask.id);
    grade.effect.mix = 0.8;
    let mut transform = Node::new(
        P::TransformImage {
            settings: fold_compositor::Transform {
                pivot: [320., 180.],
                ..Default::default()
            },
        },
        vec![grade.id],
    );
    for (second, degrees) in [(0, -3.), (2, 3.), (4, -3.)] {
        transform
            .animation
            .entry("rotate.0".into())
            .or_default()
            .insert(Time::new(second, 1)?, degrees);
    }
    let mut shuffle = Node::disconnected(P::Shuffle {
        mappings: ["red", "green", "blue"]
            .into_iter()
            .map(|component| {
                Ok(ChannelMapping {
                    destination: format!("inspection.{component}").try_into()?,
                    source: ChannelSource::A(format!("aov07.{component}").try_into()?),
                })
            })
            .collect::<Result<_, String>>()?,
    });
    shuffle.inputs[0] = Some(transform.id);
    let output = Node::new(P::Output, vec![shuffle.id]);
    let background = Node::new(
        P::Solid {
            rgba: [0.025, 0.035, 0.055, 1.],
        },
        vec![],
    );
    let merge = Node::new(
        P::Composite {
            mode: MergeMode::Over,
        },
        vec![shuffle.id, background.id],
    );
    let mut composite = Composite {
        info,
        output: output.id,
        nodes: vec![
            read, shape, mask, grade, transform, shuffle, output, background, merge,
        ],
        extensions: Default::default(),
    };
    composite.auto_layout()?;
    let id = DocumentId::new();
    let mut project = Project::new(32);
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![Mutation::PutDocument(composite.document(id)?)],
    })?;
    let request = fold_platform::packages::CommandRequest {
        id: "fold.app.read-source".into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(
            &serde_json::json!({"document":id,"node":read_id,"path":image}),
        )?,
    };
    let batch = packages::builtins().stage(&project.snapshot(), &request, &Default::default())?;
    project.commit(batch)?;
    let path = directory.join("compositor-demo.fold");
    fold_project::save(&project.snapshot(), &path)?;
    let reference = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    for preview in [
        None,
        Some(fold_render::view::View::Channel {
            name: "depth.Z".to_owned().try_into()?,
            range: fold_render::view::Range::new(0., 1.)?,
        }),
    ] {
        media_workflow::evaluate_scene(
            &project.snapshot(),
            &media_workflow::SceneRequest {
                preview,
                source: reference.clone(),
                time: Time::new(2, 1)?,
                dimensions: [160, 90],
            },
            &mut Default::default(),
            &Default::default(),
        )?;
    }
    println!("{}", path.display());
    Ok(())
}
