//! Self-contained scene/modifier/compositor demo for phase 25 acceptance.
use fold_app::{media_workflow, packages};
use fold_compositor::{Composite, Node, Parameters, Source};
use fold_foundation::{DocumentId, Time};
use fold_motion::{
    Motion,
    authoring::{node, scene::*},
    fields::Datum,
    nodes::interface::GroupSettings,
};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args().nth(1).ok_or("Pass an output .fold path")?;
    if std::path::Path::new(&path).exists() {
        return Err("Demo output already exists".into());
    }
    let mut motion = Motion::empty();
    motion.info.width = 640;
    motion.info.height = 360;
    let text = create_object(&mut motion, "fold.motion.text")?;
    node(&mut motion, text)?.set("text", Datum::Text("FOLD".into()));
    node(&mut motion, text)?.set("size", Datum::Scalar(18.));
    let shape = create_object(&mut motion, "fold.motion.rectangle")?;
    node(&mut motion, shape)?.set("width", Datum::Scalar(64.));
    node(&mut motion, shape)?.set("height", Datum::Scalar(32.));
    let objects = motion.scene.as_ref().unwrap().objects.clone();
    for object in &objects {
        let color = if object.id == text {
            [1., 1., 1., 1.]
        } else {
            [0.05, 0.45, 0.8, 1.]
        };
        node(&mut motion, object.appearance.unwrap())?.set("color", Datum::Color(color));
        node(&mut motion, object.transform.unwrap())?.set(
            "translation",
            Datum::Vector(if object.id == text {
                [-23., 6.]
            } else {
                [-32., -16.]
            }),
        );
        motion
            .scene
            .as_mut()
            .unwrap()
            .objects
            .iter_mut()
            .find(|o| o.id == object.id)
            .unwrap()
            .visible = false;
    }
    let result = create_procedural(&mut motion)?;
    let group = motion
        .graph
        .node(result)?
        .settings::<GroupSettings>()?
        .group
        .unwrap();
    let mut a = fold_motion::graph::Node::new("fold.motion.object_reference")?;
    a.settings = serde_json::json!({"object":text});
    a.position = [0., 0.];
    let mut b = fold_motion::graph::Node::new("fold.motion.object_reference")?;
    b.settings = serde_json::json!({"object":shape});
    b.position = [0., 220.];
    let mut combine = fold_motion::graph::Node::new("fold.motion.group")?;
    combine.connect("front", a.id, "content");
    combine.connect("back", b.id, "content");
    combine.position = [280., 0.];
    let definition = motion.groups.get_mut(&group).unwrap();
    definition.name = "Badge from scene objects".into();
    definition
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.group_output")
        .unwrap()
        .connect("value", combine.id, "content");
    definition.graph.nodes.extend([a, b, combine]);
    let radial = new_modifier_definition(&mut motion, true)?;
    let modifier = add_modifier(&mut motion, result, radial)?;
    node(&mut motion, modifier)?.set("count", Datum::Scalar(6.));
    node(&mut motion, modifier)?.set("radius", Datum::Scalar(105.));
    node(&mut motion, modifier)?.set("orient", Datum::Bool(false));
    for (time, radius) in [(0, 70.), (2, 120.), (4, 70.)] {
        node(&mut motion, modifier)?
            .animation
            .entry("radius.0".into())
            .or_default()
            .insert(Time::new(time, 1)?, radius);
    }
    let transform = motion
        .scene
        .as_ref()
        .unwrap()
        .objects
        .iter()
        .find(|o| o.id == result)
        .unwrap()
        .transform
        .unwrap();
    node(&mut motion, transform)?.set("translation", Datum::Vector([320., 180.]));
    motion
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == result)
        .unwrap()
        .name = "Radial badges".into();
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
                info: motion.info.clone(),
            },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(5, 1)?,
        },
        vec![],
    );
    let background = Node::new(
        Parameters::Solid {
            rgba: [0.015, 0.022, 0.04, 1.],
        },
        vec![],
    );
    let merge = Node::new(
        Parameters::Composite {
            mode: Default::default(),
        },
        vec![read.id, background.id],
    );
    let output = Node::new(Parameters::Output, vec![merge.id]);
    let mut composite = Composite {
        info: motion.info.clone(),
        nodes: vec![read, background, merge, output.clone()],
        output: output.id,
        extensions: Default::default(),
    };
    composite.auto_layout()?;
    let mut project = Project::new(64);
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![
            Mutation::PutDocument(motion.document(motion_id)?),
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
    let batch = packages::builtins().stage(&project.snapshot(), &request, &Default::default())?;
    project.commit(batch)?;
    let snapshot = project.snapshot();
    let names = snapshot
        .state()
        .organization
        .items
        .iter()
        .cloned()
        .map(|mut item| {
            item.name = match item.id {
                fold_project::ItemId::Document(id) if id == motion_id => "Radial Badge Design",
                fold_project::ItemId::Document(id) if id == composite_id => "Badge Composite",
                _ => "Demo Sequence",
            }
            .into();
            Mutation::PutItem(item)
        })
        .collect();
    project.commit(EditBatch {
        base: snapshot.revision(),
        mutations: names,
    })?;
    fold_project::save(&project.snapshot(), std::path::Path::new(&path))?;
    for frame in [0, 48, 24, 96, 0] {
        media_workflow::evaluate_scene(
            &project.snapshot(),
            &media_workflow::SceneRequest {
                preview: None,
                source: DocumentRef {
                    document: composite_id,
                    output: "video".into(),
                    extensions: Default::default(),
                },
                time: motion.info.time(frame)?,
                dimensions: [640, 360],
            },
            &mut Default::default(),
            &Default::default(),
        )?;
    }
    println!("{path}");
    Ok(())
}
