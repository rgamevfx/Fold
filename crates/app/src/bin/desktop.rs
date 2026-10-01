//! Desktop composition root. Backend details stay inside the shared UI SDK.
use fold_foundation::{DocumentId, Time};
use fold_motion::{Solid, SolidProvider};
use fold_platform::{DisplayFrame, VideoRegistry};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use std::sync::mpsc::sync_channel;

fn render_demo() -> Result<DisplayFrame, String> {
    let mut project = Project::new(1);
    let id = DocumentId::new();
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                Solid {
                    rgb: [0.0, 0.5, 1.0],
                }
                .document(id)?,
            )],
        })
        .map_err(|error| error.to_string())?;
    let snapshot = project.snapshot();
    let source = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let mut registry = VideoRegistry::default();
    registry.register(SolidProvider)?;
    let plan = registry.compile(&snapshot, &source, Time::ZERO, 640, 360)?;
    fold_render::render(plan)?
        .to_display()
        .map_err(str::to_owned)
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let (sender, receiver) = sync_channel(1);
    // One immutable render request, no live project state shared with the UI.
    std::thread::Builder::new()
        .name("fold-preview".into())
        .spawn(move || {
            let _ = sender.send(render_demo());
        })?;
    fold_ui::run_desktop(receiver)
}

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Fold desktop failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn demo_uses_registered_engine_provider() {
        let display = render_demo().unwrap();
        assert_eq!(display.dimensions(), [640, 360]);
        assert!(
            display
                .rgba()
                .chunks_exact(4)
                .all(|p| p == [0, 188, 255, 255])
        );
    }
}
