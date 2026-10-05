//! Editable path-attribute demonstration. Optional preview directory receives PPM frames.
use fold_app::{media_workflow, packages};
use fold_compositor::{Composite, Node, Parameters, Source};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_motion::{
    Motion,
    authoring::{node, scene::*},
    fields::Datum,
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use std::io::Write;
fn object(m: &mut Motion, id: ObjectId) -> &mut fold_motion::scene::Object {
    m.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap()
}
fn position(m: &mut Motion, id: ObjectId, xy: [f64; 2]) -> Result<(), String> {
    let transform = object(m, id).transform.unwrap();
    node(m, transform)?.set("translation", Datum::Vector(xy));
    Ok(())
}
fn color(m: &mut Motion, id: ObjectId, rgba: [f64; 4]) -> Result<(), String> {
    let fill = object(m, id).appearance.unwrap();
    node(m, fill)?.set("color", Datum::Color(rgba));
    Ok(())
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let output = std::env::args()
        .nth(1)
        .ok_or("Usage: path_parade <new.fold> [new-preview-directory]")?;
    if std::path::Path::new(&output).exists() {
        return Err("output already exists".into());
    }
    let mut m = Motion::new_scene();
    m.info.width = 960;
    m.info.height = 540;
    m.info.rate = [24, 1];
    m.info.frames = 240;
    let path = create_object(&mut m, "fold.motion.path")?;
    object(&mut m, path).name = "Spectrum path — edit points and color samples".into();
    object(&mut m, path).visible = false;
    node(&mut m, path)?.settings = serde_json::json!({"segments":[
        {"Move":[155.,295.]},{"Cubic":[[155.,125.],[365.,140.],[480.,295.]]},
        {"Cubic":[[625.,475.],[805.,450.],[805.,285.]]},
        {"Cubic":[[805.,115.],[615.,145.],[480.,295.]]},
        {"Cubic":[[325.,455.],[155.,455.],[155.,295.]]},"Close"
    ],"attributes":{"color":[
        {"position":0.,"value":{"Color":[0.03,0.85,1.,1.]}},
        {"position":0.25,"value":{"Color":[0.45,0.18,1.,1.]}},
        {"position":0.5,"value":{"Color":[1.,0.12,0.3,1.]}},
        {"position":0.75,"value":{"Color":[1.,0.75,0.1,1.]}},
        {"position":1.,"value":{"Color":[0.03,0.85,1.,1.]}}
    ]}});
    let group = create_group(&mut m)?;
    object(&mut m, group).name = "Traveling badges".into();
    let shape = create_object(&mut m, "fold.motion.rectangle")?;
    object(&mut m, shape).name = "Badge card".into();
    node(&mut m, shape)?.set("width", Datum::Scalar(86.));
    node(&mut m, shape)?.set("height", Datum::Scalar(38.));
    node(&mut m, shape)?.set("radius", Datum::Scalar(12.));
    position(&mut m, shape, [-43., -19.])?;
    color(&mut m, shape, [0.065, 0.085, 0.14, 1.])?;
    reparent(&mut m, shape, Some(group), Time::ZERO, true)?;
    let text = create_object(&mut m, "fold.motion.text")?;
    object(&mut m, text).name = "FLOW — receives path color".into();
    node(&mut m, text)?.set("text", Datum::Text("FLOW".into()));
    node(&mut m, text)?.set("size", Datum::Scalar(21.));
    position(&mut m, text, [-28., -14.])?;
    reparent(&mut m, text, Some(group), Time::ZERO, true)?;
    let definition = new_path_modifier_definition(&mut m, path)?;
    let modifier = add_modifier(&mut m, group, definition)?;
    for (key, value) in [
        ("count", 10.),
        ("speed", 0.1),
        ("stagger", 1.),
        ("wave", 9.),
        ("frequency", 0.4),
        ("pulse", 0.1),
    ] {
        node(&mut m, modifier)?.set(key, Datum::Scalar(value));
    }
    // Authored animation remains visible in the integrated dope sheet.
    for (frame, value) in [(0, 4.), (60, 14.), (120, 4.), (180, 14.), (240, 4.)] {
        let time = m.info.time(frame)?;
        node(&mut m, modifier)?
            .animation
            .entry("wave.0".into())
            .or_default()
            .insert(time, value);
    }
    // A guide uses the same path geometry; keep its spectrum source independently editable.
    let guide = create_procedural(&mut m)?;
    object(&mut m, guide).name = "Path guide".into();
    let def = node(&mut m, guide)?
        .settings::<fold_motion::nodes::interface::GroupSettings>()?
        .group
        .unwrap();
    let mut reference = fold_motion::graph::Node::new("fold.motion.object_reference")?;
    reference.settings = serde_json::json!({"object":path,"world":true});
    let mut fill = fold_motion::graph::Node::new("fold.motion.fill")?;
    fill.connect("content", reference.id, "content");
    fill.set("color", Datum::Color([0.; 4]));
    let mut stroke = fold_motion::graph::Node::new("fold.motion.stroke")?;
    stroke.connect("content", fill.id, "content");
    stroke.set("color", Datum::Color([0.08, 0.13, 0.22, 1.]));
    stroke.set("width", Datum::Scalar(2.));
    let g = m.groups.get_mut(&def).unwrap();
    g.graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.group_output")
        .unwrap()
        .connect("value", stroke.id, "content");
    g.graph.nodes.extend([reference, fill, stroke]);
    reorder(&mut m, guide, group, true)?;
    for (label, size, xy, rgba) in [
        ("CHROMA / PARADE", 34., [56., 38.], [0.86, 0.92, 1., 1.]),
        (
            "TEN BADGES. ONE LIVING SPECTRUM.",
            12.,
            [58., 82.],
            [0.35, 0.48, 0.65, 1.],
        ),
    ] {
        let title = create_object(&mut m, "fold.motion.text")?;
        object(&mut m, title).name = label.into();
        node(&mut m, title)?.set("text", Datum::Text(label.into()));
        node(&mut m, title)?.set("size", Datum::Scalar(size));
        position(&mut m, title, xy)?;
        color(&mut m, title, rgba)?;
    }
    // A separate accent follows the guide's transform while retaining its layer position.
    let accent = create_object(&mut m, "fold.motion.ellipse")?;
    object(&mut m, accent).name = "Orbit marker — Follow Path constraint".into();
    node(&mut m, accent)?.set("width", Datum::Scalar(9.));
    node(&mut m, accent)?.set("height", Datum::Scalar(9.));
    position(&mut m, accent, [0.; 2])?;
    color(&mut m, accent, [0.1, 0.9, 1., 1.])?;
    add_constraint(
        &mut m,
        accent,
        path,
        fold_motion::scene::ConstraintKind::Path,
        Time::ZERO,
        false,
    )?;
    object(&mut m, accent).constraints[0].progress = 0.15;
    for (frame, progress) in [(0, 0.), (240, 1.)] {
        let time = m.info.time(frame)?;
        object(&mut m, accent).constraints[0]
            .animation
            .entry("progress.0".into())
            .or_default()
            .insert(time, progress);
    }
    let mask = create_object(&mut m, "fold.motion.rectangle")?;
    object(&mut m, mask).name = "Soft stage mask".into();
    object(&mut m, mask).visible = false;
    node(&mut m, mask)?.set("width", Datum::Scalar(820.));
    node(&mut m, mask)?.set("height", Datum::Scalar(390.));
    node(&mut m, mask)?.set("radius", Datum::Scalar(70.));
    position(&mut m, mask, [70., 115.])?;
    let mut coverage = fold_motion::scene::Mask::new(mask, fold_motion::scene::MaskSpace::World);
    coverage.feather = 8.;
    object(&mut m, group).masks.push(coverage);
    object(&mut m, group).isolated = true;
    let motion_id = DocumentId::new();
    let composite_id = DocumentId::new();
    let read = Node::new(
        Parameters::Read {
            source: Source::Document {
                source: DocumentRef {
                    document: motion_id,
                    output: "video".into(),
                    extensions: Default::default(),
                },
                info: m.info.clone(),
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(10, 1)?,
        },
        vec![],
    );
    let bg = Node::new(
        Parameters::Solid {
            rgba: [0.009, 0.014, 0.026, 1.],
        },
        vec![],
    );
    let merge = Node::new(
        Parameters::Composite {
            mode: Default::default(),
        },
        vec![read.id, bg.id],
    );
    let out = Node::new(Parameters::Output, vec![merge.id]);
    let mut composite = Composite {
        info: m.info.clone(),
        nodes: vec![read, bg, merge, out.clone()],
        output: out.id,
        extensions: Default::default(),
    };
    composite.auto_layout()?;
    let mut project = Project::new(64);
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![
            Mutation::PutDocument(m.document(motion_id)?),
            Mutation::PutDocument(composite.document(composite_id)?),
        ],
    })?;
    let request = fold_platform::packages::CommandRequest {
        id: "fold.app.insert-document".into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(
            &serde_json::json!({"source":composite_id,"sequence":null,"at":Time::ZERO}),
        )?,
    };
    project.commit(packages::builtins().stage(
        &project.snapshot(),
        &request,
        &Default::default(),
    )?)?;
    let snapshot = project.snapshot();
    let mutations = snapshot
        .state()
        .organization
        .items
        .iter()
        .cloned()
        .map(|mut item| {
            item.name = match item.id {
                fold_project::ItemId::Document(id) if id == motion_id => "Chroma Parade — MoGraph",
                fold_project::ItemId::Document(id) if id == composite_id => {
                    "Chroma Parade — Composite"
                }
                _ => "Chroma Parade — Sequence",
            }
            .into();
            Mutation::PutItem(item)
        })
        .collect();
    project.commit(EditBatch {
        base: snapshot.revision(),
        mutations,
    })?;
    fold_project::save(&project.snapshot(), std::path::Path::new(&output))?;
    let preview = std::env::args().nth(2);
    if let Some(dir) = &preview {
        std::fs::create_dir(dir)?;
    }
    let mut renderer = fold_app::output::OutputRenderer::from_environment()?;
    let frames: Vec<_> = if preview.is_some() {
        (0..240).step_by(2).collect()
    } else {
        vec![0, 120, 48, 200, 0]
    };
    for frame in frames {
        let image = renderer.evaluate(
            &project.snapshot(),
            &media_workflow::SceneRequest {
                region: None,
                preview: None,
                source: DocumentRef {
                    document: composite_id,
                    output: "video".into(),
                    extensions: Default::default(),
                },
                time: m.info.time(frame)?,
                dimensions: [960, 540],
            },
            &Default::default(),
        )?;
        if let Some(dir) = &preview {
            let mut f = std::io::BufWriter::new(std::fs::File::create(format!(
                "{dir}/{:04}.ppm",
                frame / 2
            ))?);
            writeln!(f, "P6\n960 540\n255")?;
            for pixel in image.rgba().chunks_exact(4) {
                f.write_all(&pixel[..3])?;
            }
        }
    }
    println!("{output}");
    Ok(())
}
