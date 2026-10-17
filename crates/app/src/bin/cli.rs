//! Minimal headless foundation checkpoint and saved-project rendering.
use fold_foundation::{DocumentId, Time};
use fold_project::{DocumentRef, EditBatch, Mutation, load, save};
use std::{error::Error, io::Write, path::Path};

fn run() -> Result<(), Box<dyn Error>> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    #[cfg(target_os = "linux")]
    if args.first().is_some_and(|arg| arg == "mcp") {
        if args.len() != 2 {
            return Err("usage: fold-cli mcp <socket>".into());
        }
        fold_app::agent::mcp::serve(
            Path::new(&args[1]),
            std::io::stdin().lock(),
            std::io::stdout().lock(),
        )?;
        return Ok(());
    }
    #[cfg(target_os = "linux")]
    if args.first().is_some_and(|arg| arg == "protocol") {
        use std::io::Read;
        if args.len() != 2 {
            return Err("usage: fold-cli protocol <socket> (request JSON on stdin)".into());
        }
        let mut bytes = Vec::new();
        std::io::stdin()
            .take(fold_app::agent::retry::MAX_REQUEST_BYTES as u64 + 1)
            .read_to_end(&mut bytes)?;
        let response = fold_app::agent::local::request(Path::new(&args[1]), &bytes)?;
        std::io::stdout().write_all(&response)?;
        return Ok(());
    }
    fold_platform::color::validate_runtime()?;
    if args.first().is_some_and(|arg| arg == "color-config") {
        if args.len() != 3 {
            return Err(
                "usage: fold-cli color-config <project.fold> <absolute-config.ocio>".into(),
            );
        }
        let path = Path::new(&args[1]);
        let mut project = load(path, 32)?;
        project.commit(fold_app::color::external(
            &project.snapshot(),
            Path::new(&args[2]),
        )?)?;
        save(&project.snapshot(), path)?;
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "output-color") {
        if !(4..=5).contains(&args.len()) {
            return Err(
                "usage: fold-cli output-color <project.fold> <display> <view> [look]".into(),
            );
        }
        let path = Path::new(&args[1]);
        let mut project = load(path, 32)?;
        let snapshot = project.snapshot();
        let (output, _) = fold_app::output::output(&snapshot)?;
        let mut transform = fold_platform::color::output(&snapshot, output.document)?;
        transform.display = args[2].to_str().ok_or("Invalid display")?.into();
        transform.view = args[3].to_str().ok_or("Invalid view")?.into();
        transform.look = args
            .get(4)
            .map(|v| v.to_str().ok_or("Invalid look").map(str::to_owned))
            .transpose()?;
        fold_app::color::with_config(&snapshot, |config| {
            config
                .ok_or("Legacy output retains its original transform")?
                .display(fold_color::WORKING_SPACE, &transform.display_transform())?;
            Ok(())
        })?;
        project.commit(fold_app::color::set_output(
            &snapshot,
            output.document,
            transform,
        )?)?;
        save(&project.snapshot(), path)?;
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "import-timeline") {
        if args.len() < 3 {
            return Err("usage: fold-cli import-timeline <new-project.fold> <video.mp4>...".into());
        }
        let path = Path::new(&args[1]);
        if path.exists() {
            return Err("project already exists".into());
        }
        let mut project = fold_app::color::new_project(32);
        let paths = args[2..]
            .iter()
            .map(std::path::PathBuf::from)
            .collect::<Vec<_>>();
        let snapshot = project.snapshot();
        let request = fold_timeline::package::request(
            &snapshot,
            fold_timeline::package::IMPORT,
            &fold_timeline::ImportArgs {
                document: None,
                paths,
                at: None,
                video_track: None,
                audio_track: None,
                audio_only: false,
            },
        )?;
        project.commit(fold_app::packages::builtins().stage(
            &snapshot,
            &request,
            &fold_media::Cancel::default(),
        )?)?;
        save(&project.snapshot(), path)?;
        println!("Imported editable timeline with linked audio");
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "edit-timeline") {
        if !(5..=6).contains(&args.len()) {
            return Err("usage: fold-cli edit-timeline <project.fold> <clip-index-0-based> move|trim|split <frame> [end-frame]".into());
        }
        let path = Path::new(&args[1]);
        let mut project = load(path, 32)?;
        let snapshot = project.snapshot();
        let (reference, sequence) = fold_app::timeline_workflow::active(&snapshot)?;
        let index: usize = args[2].to_str().ok_or("invalid clip")?.parse()?;
        let clip = sequence
            .clips
            .get(index)
            .ok_or("clip index outside sequence")?;
        let info = sequence.info()?;
        let time = info.time(args[4].to_str().ok_or("invalid frame")?.parse()?)?;
        let edit = match args[3].to_str() {
            Some("move") if args.len() == 5 => fold_timeline::ClipEdit::Move { start: time },
            Some("split") if args.len() == 5 => fold_timeline::ClipEdit::Split {
                at: time,
                right_id: fold_foundation::ObjectId::new(),
            },
            Some("trim") if args.len() == 6 => fold_timeline::ClipEdit::Trim {
                start: time,
                end: info.time(args[5].to_str().ok_or("invalid end")?.parse()?)?,
            },
            _ => return Err("invalid timeline edit".into()),
        };
        let request = fold_timeline::package::request(
            &snapshot,
            fold_timeline::package::EDIT,
            &fold_timeline::package::EditArgs {
                document: reference.document,
                edit: fold_timeline::SequenceEdit::Clip {
                    id: clip.id,
                    linked: true,
                    edit,
                },
            },
        )?;
        project.commit(fold_app::packages::builtins().stage(
            &snapshot,
            &request,
            &fold_media::Cancel::default(),
        )?)?;
        save(&project.snapshot(), path)?;
        println!("Timeline edit committed and saved");
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "command") {
        if args.len() != 4 {
            return Err(
                "usage: fold-cli command <project.fold> <registered-command-id> <json-arguments>"
                    .into(),
            );
        }
        let path = Path::new(&args[1]);
        let mut project = load(path, 32)?;
        let snapshot = project.snapshot();
        let request = fold_platform::packages::CommandRequest {
            id: args[2].to_str().ok_or("invalid command ID")?.into(),
            base: snapshot.revision(),
            arguments: args[3]
                .to_str()
                .ok_or("invalid arguments")?
                .as_bytes()
                .to_vec(),
        };
        project.commit(fold_app::packages::builtins().stage(
            &snapshot,
            &request,
            &fold_media::Cancel::default(),
        )?)?;
        save(&project.snapshot(), path)?;
        return Ok(());
    }
    if args.first().is_some_and(|arg| arg == "export-video") {
        if args.len() != 5 {
            return Err("usage: fold-cli export-video <project.fold> <new-output.mp4|new-output.mov> <start-frame> <end-frame-exclusive>".into());
        }
        let project = load(Path::new(&args[1]), 0)?;
        fold_app::output::export(
            &project.snapshot(),
            Path::new(&args[2]),
            args[3].to_str().ok_or("invalid start")?.parse()?,
            args[4].to_str().ok_or("invalid end")?.parse()?,
            &fold_media::Cancel::default(),
        )?;
        println!("Export complete (sequence exports include audio; legacy layers are video-only)");
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
        let mut project = fold_app::color::new_project(16);
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
        let mut project = fold_app::color::new_project(16);
        let id = DocumentId::new();
        for rgb in [[0.0, 0.5, 1.0], [1.0, 0.0, 0.0]] {
            project.commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(
                    checkpoint_composite(rgb).document(id)?,
                )],
            })?;
        }
        project.undo()?;
        save(&project.snapshot(), project_path)?;
    }
    let load_start = std::time::Instant::now();
    let project = load(project_path, 16)?;
    let project_load_ms = load_start.elapsed().as_secs_f64() * 1000.;
    let snapshot = project.snapshot();
    let source = if snapshot.state().documents.len() == 1 {
        DocumentRef {
            document: *snapshot.state().documents.keys().next().unwrap(),
            output: "video".into(),
            extensions: Default::default(),
        }
    } else {
        fold_app::output::output(&snapshot)?.0
    };
    let renderer_start = std::time::Instant::now();
    let mut renderer = fold_app::output::OutputRenderer::from_environment()?;
    let renderer_init_ms = renderer_start.elapsed().as_secs_f64() * 1000.;
    let evaluate_start = std::time::Instant::now();
    let output = renderer.evaluate(
        &snapshot,
        &fold_app::output::SceneRequest {
            region: None,
            preview: None,
            source,
            time,
            dimensions: [width, height],
        },
        &fold_media::Cancel::default(),
    )?;
    if std::env::var_os("FOLD_PROFILE_RENDER").is_some() {
        eprintln!(
            "{}",
            serde_json::json!({
                "project_load_ms": project_load_ms,
                "renderer_init_ms": renderer_init_ms,
                "evaluate_ms": evaluate_start.elapsed().as_secs_f64() * 1000.,
            })
        );
    }
    let parent = image_path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    writeln!(file, "P6\n{width} {height}\n255")?;
    for pixel in output.rgba().chunks_exact(4) {
        file.write_all(&pixel[..3])?;
    }
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

fn checkpoint_composite(rgb: [f32; 3]) -> fold_compositor::Composite {
    use fold_compositor::{Composite, Node, OperationConfig};
    let solid = Node::new(
        OperationConfig::Solid {
            rgba: [rgb[0], rgb[1], rgb[2], 1.],
        },
        vec![],
    );
    let output = Node::new(OperationConfig::Output, vec![solid.id]);
    Composite {
        info: fold_media::VideoInfo {
            width: 64,
            height: 64,
            rate: [24, 1],
            frames: 24,
        },
        output: output.id,
        nodes: vec![solid, output],
        extensions: Default::default(),
    }
}
