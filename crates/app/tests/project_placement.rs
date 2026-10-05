use fold_app::{browser, ingest, packages};
use fold_foundation::{DocumentId, Time};
use fold_media::Cancel;
use fold_platform::browser::{BrowserCommand as Command, Entry, Placement, PlacementSource};
use fold_project::{DocumentRef, ItemId, Parent, Project};

fn edit(project: &mut Project, command: Command) {
    let batch = browser::edit(&project.snapshot(), command).unwrap();
    project.commit(batch).unwrap();
}
fn create(project: &mut Project, kind: &str) -> DocumentId {
    let before = project.snapshot();
    edit(
        project,
        Command::Create {
            kind: kind.into(),
            parent: Parent::Root,
        },
    );
    *project
        .snapshot()
        .state()
        .documents
        .keys()
        .find(|id| !before.state().documents.contains_key(id))
        .unwrap()
}
fn placement(source: PlacementSource, target: DocumentId) -> Placement {
    Placement {
        source,
        target,
        track: None,
        at: Time::ZERO,
        source_start: Time::ZERO,
        duration: None,
        audio_only: false,
    }
}
fn output(document: DocumentId) -> PlacementSource {
    PlacementSource::Output(DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    })
}
#[test]
fn desktop_ingest_completion_keeps_the_proposal_live_and_import_is_asset_only() {
    use fold_platform::desktop::{DesktopClient, DesktopCommand};
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ppm");
    std::fs::write(&path, b"P6\n1 1\n255\n\x01\x02\x03").unwrap();
    let mut project = Project::new(16);
    create(&mut project, fold_timeline::SEQUENCE);
    let mut session = fold_app::session::Session::new(project);
    let before = session.snapshot().unwrap();
    let selection = session.state().selection.clone();
    let viewer = session.state().viewer_document;
    for _ in 0..2 {
        session.command(DesktopCommand::Browser(Command::Import {
            paths: vec![path.clone()],
            destination: Parent::Root,
        }));
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
        while session.state().busy {
            assert!(std::time::Instant::now() < deadline);
            session.poll();
            std::thread::sleep(std::time::Duration::from_millis(2));
        }
        assert!(
            session.state().status.is_empty(),
            "{}",
            session.state().status
        );
        assert_eq!(session.snapshot().unwrap().state().assets.len(), 1);
        assert_eq!(session.take_imported_items().len(), 1);
    }
    let after = session.snapshot().unwrap();
    assert_eq!(after.state().documents, before.state().documents);
    assert_eq!(after.state().settings, before.state().settings);
    assert_eq!(session.state().selection.document, selection.document);
    assert_eq!(session.state().selection.objects, selection.objects);
    assert_eq!(session.state().viewer_document, viewer);
    session.command(DesktopCommand::Undo);
    assert!(session.snapshot().unwrap().state().assets.is_empty());
}

fn movie(path: &std::path::Path) {
    assert!(
        std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-y",
                "-f",
                "lavfi",
                "-i",
                "color=red:size=64x36:rate=24",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=440:sample_rate=48000",
                "-t",
                "1",
                "-c:v",
                "libx264",
                "-threads",
                "1",
                "-pix_fmt",
                "yuv420p",
                "-color_primaries",
                "bt709",
                "-color_trc",
                "bt709",
                "-colorspace",
                "bt709",
                "-color_range",
                "tv",
                "-c:a",
                "aac"
            ])
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
}
#[test]
fn project_workflow_reuses_assets_and_outputs_and_preserves_history_and_content() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.mp4");
    movie(&path);
    let mut project = Project::new(32);
    let sequence = create(&mut project, fold_timeline::SEQUENCE);
    let composite = create(&mut project, fold_compositor::COMPOSITE);
    let motion = create(&mut project, fold_motion::MOTION);
    let mut authored_motion =
        fold_motion::Motion::from_document(&project.snapshot().state().documents[&motion]).unwrap();
    fold_motion::authoring::add_content(&mut authored_motion, "fold.motion.rectangle").unwrap();
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                authored_motion.document(motion).unwrap(),
            )],
        })
        .unwrap();
    let before = project.snapshot();
    let proposal = ingest::import(
        &before,
        &ingest::ImportRequest {
            base: before.revision(),
            paths: vec![path.clone()],
            destination: Parent::Root,
        },
        &Cancel::default(),
    )
    .unwrap();
    let ItemId::Asset(asset) = proposal.items[0] else {
        panic!()
    };
    proposal.commit(&mut project).unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        before.state().documents
    );
    let before_place = project.snapshot();
    edit(
        &mut project,
        Command::Place(placement(PlacementSource::Asset(asset), sequence)),
    );
    let sequence_doc = project.snapshot().state().documents[&sequence].clone();
    let seq = fold_timeline::Sequence::from_document(&sequence_doc).unwrap();
    assert_eq!(seq.clips.len(), 2);
    assert_eq!(seq.clips[0].link, seq.clips[1].link);
    assert!(seq.clips[0].link.is_some());
    assert!(seq.clips.iter().all(|c| c.asset == asset));
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        before_place.state().documents
    );
    assert_eq!(
        project.snapshot().state().assets,
        before_place.state().assets
    );
    assert_eq!(
        project.snapshot().state().organization,
        before_place.state().organization
    );
    project.redo().unwrap();
    edit(
        &mut project,
        Command::Place(placement(PlacementSource::Asset(asset), composite)),
    );
    let graph = fold_compositor::Composite::from_document(
        &project.snapshot().state().documents[&composite],
    )
    .unwrap();
    assert!(graph.nodes.iter().any(|n| matches!(n.parameters, fold_compositor::Parameters::Read { source: fold_compositor::Source::Asset { asset: id, .. }, .. } if id == asset)));
    let mut nested = placement(output(motion), sequence);
    nested.at = Time::new(1, 1).unwrap();
    edit(&mut project, Command::Place(nested));
    edit(
        &mut project,
        Command::Place(placement(output(motion), composite)),
    );
    let snapshot = project.snapshot();
    let graph = packages::builtins()
        .video(
            &snapshot,
            &DocumentRef {
                document: sequence,
                output: "video".into(),
                extensions: Default::default(),
            },
            Time::new(1, 1).unwrap(),
            64,
            36,
        )
        .unwrap();
    assert!(
        graph.nodes.iter().any(
            |node| matches!(node, fold_render::ImageOp::Vector(drawings) if !drawings.is_empty())
        ),
        "nested motion remains native vector work, not a flattened file"
    );
    assert_eq!(snapshot.state().assets.len(), 1);
    assert!(
        snapshot.state().documents[&sequence]
            .dependencies
            .iter()
            .any(|r| r.document == motion)
    );
    assert!(
        snapshot.state().documents[&composite]
            .dependencies
            .iter()
            .any(|r| r.document == motion)
    );
    assert!(
        browser::edit(
            &snapshot,
            Command::Delete(Entry::Item(ItemId::Asset(asset)))
        )
        .is_err()
    );
    assert!(
        browser::edit(
            &snapshot,
            Command::Delete(Entry::Item(ItemId::Document(motion)))
        )
        .is_err()
    );
    let content = fold_app::media_workflow::content_for(&snapshot, sequence).unwrap();
    edit(
        &mut project,
        Command::NewBin {
            parent: Parent::Root,
            name: "Footage".into(),
        },
    );
    let bin = *project
        .snapshot()
        .state()
        .organization
        .bins
        .keys()
        .next()
        .unwrap();
    edit(
        &mut project,
        Command::Move {
            entry: Entry::Item(ItemId::Asset(asset)),
            parent: Parent::Bin(bin),
        },
    );
    edit(
        &mut project,
        Command::Rename {
            entry: Entry::Item(ItemId::Asset(asset)),
            name: "Reusable footage".into(),
        },
    );
    assert_eq!(
        fold_app::media_workflow::content_for(&project.snapshot(), sequence).unwrap(),
        content
    );
    assert!(
        browser::edit(&project.snapshot(), Command::Delete(Entry::Bin(bin)))
            .and_then(|batch| project.commit(batch).map_err(|e| e.to_string()))
            .is_err()
    );
    project.undo().unwrap();
    project.redo().unwrap();
    let archive = dir.path().join("project.fold");
    fold_project::save(&project.snapshot(), &archive).unwrap();
    let reopened = fold_project::load(archive, 32).unwrap();
    assert_eq!(reopened.snapshot().state(), project.snapshot().state());
    assert!(path.exists());
    let metadata = fold_platform::browser::asset_metadata(&project.snapshot(), asset).unwrap();
    let pixels = fold_media::thumbnail::thumbnail(
        &path,
        &project.snapshot().state().assets[&asset].fingerprint,
        &metadata.profile,
        &Cancel::default(),
    )
    .unwrap();
    assert_eq!(pixels.len(), 128 * 72 * 4);
}
#[test]
fn invalid_targets_ranges_locked_tracks_cycles_and_unavailable_providers_are_atomic() {
    let mut project = Project::new(32);
    let sequence = create(&mut project, fold_timeline::SEQUENCE);
    let composite = create(&mut project, fold_compositor::COMPOSITE);
    let motion = create(&mut project, fold_motion::MOTION);
    let before = project.snapshot();
    for request in [
        placement(output(sequence), sequence),
        placement(output(DocumentId::new()), sequence),
        placement(output(motion), motion),
    ] {
        assert!(browser::edit(&before, Command::Place(request)).is_err());
        assert_eq!(project.snapshot().state(), before.state());
    }
    let mut request = placement(output(motion), sequence);
    request.at = Time::new(-1, 1).unwrap();
    assert!(browser::edit(&before, Command::Place(request.clone())).is_err());
    request.at = Time::ZERO;
    request.duration = Some(Time::new(100, 1).unwrap());
    assert!(browser::edit(&before, Command::Place(request)).is_err());
    let mut seq =
        fold_timeline::Sequence::from_document(&before.state().documents[&sequence]).unwrap();
    seq.tracks[0].locked = true;
    let track = seq.tracks[0].id;
    project
        .commit(fold_project::EditBatch {
            base: before.revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                seq.document(sequence).unwrap(),
            )],
        })
        .unwrap();
    let mut request = placement(output(motion), sequence);
    request.track = Some(track);
    assert!(
        browser::edit(&project.snapshot(), Command::Place(request))
            .unwrap_err()
            .contains("locked")
    );
    edit(
        &mut project,
        Command::Place(placement(output(sequence), composite)),
    );
    assert!(
        browser::edit(
            &project.snapshot(),
            Command::Place(placement(output(composite), sequence))
        )
        .unwrap_err()
        .contains("cycle")
    );
    assert!(
        packages::builtins()
            .create("unavailable.document", DocumentId::new())
            .is_err()
    );
}
