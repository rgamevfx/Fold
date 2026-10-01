//! Minimal headless foundation checkpoint and saved-project rendering.
use fold_foundation::{DocumentId, Time};
use fold_motion::{Solid, SolidProvider};
use fold_platform::VideoRegistry;
use fold_project::{DocumentRef, EditBatch, Mutation, Project, load, save};
use std::{error::Error, io::Write, path::Path};

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 3 || (args[0] != "checkpoint" && args[0] != "render") {
        return Err("usage: fold-cli <checkpoint|render> <project.fold> <new-output.ppm>".into());
    }
    let project_path = Path::new(&args[1]);
    let image_path = Path::new(&args[2]);
    if args[0] == "checkpoint" {
        if project_path.exists() {
            return Err("checkpoint project already exists".into());
        }
        let mut project = Project::new(16);
        let id = DocumentId::new();
        for rgb in [[0.0, 0.5, 1.0], [1.0, 0.0, 0.0]] {
            project.commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(Solid { rgb }.document(id)?)],
            })?;
        }
        project.undo()?;
        save(&project.snapshot(), project_path)?;
    }
    let project = load(project_path, 16)?;
    let snapshot = project.snapshot();
    if snapshot.state().documents.len() != 1 {
        return Err("this initial CLI requires exactly one source document".into());
    }
    let id = *snapshot.state().documents.keys().next().unwrap();
    let mut registry = VideoRegistry::default();
    registry.register(SolidProvider)?;
    let source = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let frame = fold_render::render(registry.compile(&snapshot, &source, Time::ZERO, 64, 64)?)?;
    let parent = image_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    frame.write_ppm(file.as_file_mut())?;
    file.flush()?;
    file.as_file().sync_all()?;
    file.persist_noclobber(image_path)?;
    println!(
        "Rendered committed revision {} to {}",
        snapshot.revision().0,
        image_path.display()
    );
    Ok(())
}
fn main() {
    if let Err(error) = run() {
        eprintln!("fold-cli: {error}");
        std::process::exit(1);
    }
}
