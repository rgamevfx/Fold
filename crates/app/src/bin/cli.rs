//! Minimal headless foundation checkpoint and saved-project rendering.
use fold_foundation::{DocumentId, Time};
use fold_motion::{Solid, SolidProvider};
use fold_platform::VideoRegistry;
use fold_project::{DocumentRef, EditBatch, Mutation, Project, load, save};
use std::{error::Error, io::Write, path::Path};

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "import-video") {
        if !(3..=4).contains(&args.len()) {
            return Err(
                "usage: fold-cli import-video <new-project.fold> <background.mp4> [foreground.mp4]"
                    .into(),
            );
        }
        let path = Path::new(&args[1]);
        if path.exists() {
            return Err("import project already exists".into());
        }
        let mut project = Project::new(32);
        let paths = args[2..]
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();
        let batch = fold_app::media_workflow::import(
            &project.snapshot(),
            &paths,
            &fold_media::Cancel::default(),
        )?;
        project.commit(batch)?;
        save(&project.snapshot(), path)?;
        println!(
            "Imported video layers to {} (audio ignored)",
            path.display()
        );
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "export-video") {
        if args.len() != 5 {
            return Err("usage: fold-cli export-video <project.fold> <new-output.mp4> <start-frame> <end-frame-exclusive>".into());
        }
        let project = load(Path::new(&args[1]), 0)?;
        fold_app::media_workflow::export(
            &project.snapshot(),
            Path::new(&args[2]),
            args[3].to_str().ok_or("invalid start")?.parse()?,
            args[4].to_str().ok_or("invalid end")?.parse()?,
            &fold_media::Cancel::default(),
        )?;
        println!("Export complete (video only)");
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "import-sequence") {
        if args.len() < 5 {
            return Err("usage: fold-cli import-sequence <new-project.fold> <fps-num> <fps-den> <frame.ppm>...".into());
        }
        let path = Path::new(&args[1]);
        if path.exists() {
            return Err("import project already exists".into());
        }
        let rate = [
            args[2].to_str().ok_or("invalid rate")?.parse()?,
            args[3].to_str().ok_or("invalid rate")?.parse()?,
        ];
        // Bound input before allocating; decode/import further validates the profile.
        let mut frames = Vec::new();
        let mut total = 12usize;
        for path in &args[4..] {
            use std::io::Read;
            let mut bytes = Vec::new();
            std::fs::File::open(path)?
                .take((fold_media::MAX_SOURCE_BYTES + 1) as u64)
                .read_to_end(&mut bytes)?;
            total += bytes.len() + 4;
            if total > 16 * 1024 * 1024 || frames.len() >= 4096 {
                return Err("sequence exceeds import budget".into());
            }
            frames.push(bytes);
        }
        let document = fold_timeline::import_sequence(DocumentId::new(), rate, &frames)?;
        let mut project = Project::new(16);
        project.commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })?;
        save(&project.snapshot(), path)?;
        println!(
            "Imported {} embedded frames to {}",
            frames.len(),
            path.display()
        );
        return Ok(());
    }
    if !((args.len() == 3 && args[0] == "checkpoint")
        || ([3, 7].contains(&args.len()) && args[0] == "render"))
    {
        return Err("usage: fold-cli checkpoint <project.fold> <new-output.ppm> | render <project.fold> <new-output.ppm> [width height time-num time-den] | import-sequence <new-project.fold> <fps-num> <fps-den> <frame.ppm>...".into());
    }
    let (width, height, time) = if args.len() == 7 {
        (
            args[3].to_str().ok_or("invalid width")?.parse()?,
            args[4].to_str().ok_or("invalid height")?.parse()?,
            Time::new(
                args[5].to_str().ok_or("invalid time")?.parse()?,
                args[6].to_str().ok_or("invalid time")?.parse()?,
            )?,
        )
    } else {
        (64, 64, Time::ZERO)
    };
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
    registry.register(fold_timeline::ImageSequenceProvider)?;
    registry.register(fold_timeline::VideoLayersProvider)?;
    let source = DocumentRef {
        document: id,
        output: "video".into(),
        extensions: Default::default(),
    };
    let frame = fold_render::render_with(
        registry.compile(&snapshot, &source, time, width, height)?,
        &mut fold_media::Decoder::default(),
        &fold_media::Cancel::default(),
    )?;
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
