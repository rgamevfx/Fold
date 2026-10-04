//! Write a small animation project for native workflow review.
use fold_animation::{Curve, Interpolation, Tangents};
use fold_compositor::{Composite, Node, Parameters as P};
use fold_foundation::{DocumentId, Time};
use fold_project::{EditBatch, Mutation, Project};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .ok_or("Pass a new output project path")?;
    if std::path::Path::new(&path).exists() {
        return Err("Output already exists".into());
    }
    let solid = Node::new(
        P::Solid {
            rgba: [0.1, 0.4, 0.8, 1.],
        },
        vec![],
    );
    let mask = Node::new(
        P::Mask {
            rect: [40, 40, 200, 200],
        },
        vec![],
    );
    let masked = Node::new(P::ApplyMask, vec![solid.id, mask.id]);
    let mut transform = Node::new(
        P::Transform {
            translate: [0., 0.],
            scale: [1., 1.],
            opacity: 1.,
        },
        vec![masked.id],
    );
    for (path, values) in [
        ("position.0", [0., 200., 0.]),
        ("position.1", [0., 80., 0.]),
        ("opacity.0", [1., 0.5, 1.]),
    ] {
        let mut curve = Curve::default();
        for (index, value) in values.into_iter().enumerate() {
            curve.insert(Time::new(index as i64 * 2, 1)?, value);
        }
        for key in &mut curve.keys {
            key.interpolation = Interpolation::Bezier;
            key.tangents = Tangents::Broken;
        }
        transform.animation.insert(path.into(), curve);
    }
    let output = Node::new(P::Output, vec![transform.id]);
    let mut composite = Composite {
        info: fold_media::VideoInfo {
            width: 640,
            height: 360,
            rate: [24, 1],
            frames: 120,
        },
        output: output.id,
        nodes: vec![solid, mask, masked, transform, output],
        extensions: Default::default(),
    };
    composite.auto_layout()?;
    let mut project = Project::new(16);
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![Mutation::PutDocument(
            composite.document(DocumentId::new())?,
        )],
    })?;
    fold_project::save(&project.snapshot(), path)?;
    Ok(())
}
