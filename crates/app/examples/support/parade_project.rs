use fold_app::{media_workflow, packages};
use fold_compositor::{Composite, Node, Parameters, Source};
use fold_foundation::{DocumentId, Time};
use fold_motion::Motion;
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use std::io::Write;
pub fn save(
    m: Motion,
    output: String,
    preview: Option<String>,
    names: [&str; 3],
) -> Result<(), Box<dyn std::error::Error>> {
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
                fold_project::ItemId::Document(id) if id == motion_id => names[0],
                fold_project::ItemId::Document(id) if id == composite_id => names[1],
                _ => names[2],
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
