mod support;
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
    // Graph providers are layer content, no longer standalone Project creation kinds.
    if kind == fold_compositor::COMPOSITE {
        let id = DocumentId::new();
        project
            .commit(fold_project::EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![fold_project::Mutation::PutDocument(
                    packages::builtins().create(kind, id).unwrap(),
                )],
            })
            .unwrap();
        return id;
    }
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
        graph: None,
        source,
        target,
        track: None,
        at: Time::ZERO,
        source_start: Time::ZERO,
        duration: None,
        audio_only: false,
        new_track: false,
        mode: Default::default(),
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
    let image = create(&mut project, fold_compositor::COMPOSITE);
    let mut authored_image = support::image([1.; 4]);
    authored_image.nodes[0]
        .set_configuration(fold_compositor::OperationConfig::Shape {
            shape: fold_compositor::Shape::Rectangle,
            bounds: [0., 0., 64., 36.],
        })
        .unwrap();
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                authored_image.document(image).unwrap(),
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
    assert!(graph.nodes.iter().any(|n| matches!(n.configuration(), fold_compositor::OperationConfig::Read { source: fold_compositor::Source::Asset { asset: id, .. }, .. } if id == asset)));
    let mut nested = placement(output(image), sequence);
    nested.at = Time::new(1, 1).unwrap();
    edit(&mut project, Command::Place(nested));
    edit(
        &mut project,
        Command::Place(placement(output(image), composite)),
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
        "nested image remains native vector work, not a flattened file"
    );
    assert_eq!(snapshot.state().assets.len(), 1);
    assert!(
        snapshot.state().documents[&sequence]
            .dependencies
            .iter()
            .any(|r| r.document == image)
    );
    assert!(
        snapshot.state().documents[&composite]
            .dependencies
            .iter()
            .any(|r| r.document == image)
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
            Command::Delete(Entry::Item(ItemId::Document(image)))
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
    let image = create(&mut project, fold_compositor::COMPOSITE);
    let before = project.snapshot();
    for request in [
        placement(output(sequence), sequence),
        placement(output(DocumentId::new()), sequence),
        placement(output(image), image),
    ] {
        assert!(browser::edit(&before, Command::Place(request)).is_err());
        assert_eq!(project.snapshot().state(), before.state());
    }
    let mut request = placement(output(image), sequence);
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
    let mut request = placement(output(image), sequence);
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

#[test]
fn common_composition_accepts_still_assets_as_full_duration_layers() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("background.ppm");
    std::fs::write(&path, b"P6\n1 1\n255\n\xff\x00\x00").unwrap();
    let mut project = Project::new(16);
    let proposal = ingest::import(
        &project.snapshot(),
        &ingest::ImportRequest {
            base: project.snapshot().revision(),
            paths: vec![path],
            destination: Parent::Root,
        },
        &Cancel::default(),
    )
    .unwrap();
    proposal.commit(&mut project).unwrap();
    let asset = *project.snapshot().state().assets.keys().next().unwrap();
    let document = create(&mut project, fold_composition::COMPOSITION);
    edit(
        &mut project,
        Command::Place(placement(PlacementSource::Asset(asset), document)),
    );
    let snapshot = project.snapshot();
    let composition =
        fold_composition::Composition::from_document(&snapshot.state().documents[&document])
            .unwrap();
    assert_eq!(composition.layers.len(), 1);
    assert_eq!(
        composition.layers[0].end,
        composition.info.time(composition.info.frames).unwrap()
    );
    assert_eq!(composition.layers[0].name, "background.ppm");
    let layer = composition.layers[0].id;
    let request = fold_platform::packages::CommandRequest {
        id: fold_app::composition_layers::ADD.into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(
            &serde_json::json!({"document":document,"kind":"process","layer":layer}),
        )
        .unwrap(),
    };
    let processing = packages::builtins()
        .stage(&project.snapshot(), &request, &Cancel::default())
        .unwrap();
    project.commit(processing).unwrap();
    let reference = DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    };
    let graph = packages::builtins()
        .video(&project.snapshot(), &reference, Time::ZERO, 1280, 720)
        .unwrap();
    assert!(
        fold_render::render(graph)
            .unwrap()
            .pixels()
            .iter()
            .any(|pixel| pixel[0] > 0.9)
    );

    let before = project.snapshot().state().documents.clone();
    project.undo().unwrap();
    project.redo().unwrap();
    assert_eq!(project.snapshot().state().documents, before);
}

#[test]
fn timeline_new_track_placement_and_collision_modes_share_atomic_history() {
    let mut project = Project::new(16);
    let sequence = create(&mut project, fold_timeline::SEQUENCE);
    let source = create(&mut project, fold_compositor::COMPOSITE);
    let mut request = placement(output(source), sequence);
    request.new_track = true;
    request.duration = Some(Time::new(2, 1).unwrap());
    project
        .commit(
            packages::builtins()
                .place(&project.snapshot(), &request)
                .unwrap(),
        )
        .unwrap();
    let read = |p: &Project| {
        fold_timeline::Sequence::from_document(&p.snapshot().state().documents[&sequence]).unwrap()
    };
    let placed = read(&project);
    assert_eq!(placed.tracks.len(), 3);
    let track = placed.clips[0].track;
    project.undo().unwrap();
    assert_eq!(read(&project).tracks.len(), 2);
    project.redo().unwrap();
    request.new_track = false;
    request.track = Some(track);
    assert!(
        packages::builtins()
            .place(&project.snapshot(), &request)
            .is_err()
    );
    request.mode = fold_platform::browser::PlacementMode::Insert;
    project
        .commit(
            packages::builtins()
                .place(&project.snapshot(), &request)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(read(&project).end().unwrap(), Time::new(4, 1).unwrap());
    request.mode = fold_platform::browser::PlacementMode::Overwrite;
    project
        .commit(
            packages::builtins()
                .place(&project.snapshot(), &request)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(read(&project).clips.len(), 2);
}

fn wait_for_ingest(session: &mut fold_app::session::Session) {
    use fold_platform::desktop::DesktopClient;
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(15);
    while session.state().busy {
        assert!(std::time::Instant::now() < deadline, "ingest timeout");
        session.poll();
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
fn still(dir: &std::path::Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, b"P6\n1 1\n255\n\xff\x00\x00").unwrap();
    path
}
#[test]
fn folder_import_preserves_bins_skips_sidecars_and_undoes_together() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("Footage");
    let child = root.join("Day 1");
    std::fs::create_dir_all(&child).unwrap();
    still(&root, "a.ppm");
    still(&child, "b.ppm");
    std::fs::write(child.join("notes.txt"), "sidecar").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(&root, child.join("cycle")).unwrap();
    let mut project = Project::new(16);
    let proposal = ingest::import(
        &project.snapshot(),
        &ingest::ImportRequest {
            base: project.snapshot().revision(),
            destination: Parent::Root,
            paths: vec![root],
        },
        &Cancel::default(),
    )
    .unwrap();
    proposal.commit(&mut project).unwrap();
    let snapshot = project.snapshot();
    let organization = &snapshot.state().organization;
    let root = organization
        .bins
        .values()
        .find(|b| b.name == "Footage")
        .unwrap();
    let child = organization
        .bins
        .values()
        .find(|b| b.name == "Day 1")
        .unwrap();
    assert_eq!(child.parent, Parent::Bin(root.id));
    assert_eq!(
        organization
            .items
            .iter()
            .find(|i| i.name == "b.ppm")
            .unwrap()
            .parent,
        Parent::Bin(child.id)
    );
    assert_eq!(snapshot.state().assets.len(), 2);
    assert_eq!(organization.bins.len(), 2);
    project.undo().unwrap();
    assert!(project.snapshot().state().assets.is_empty());
    assert!(project.snapshot().state().organization.bins.is_empty());
    project.redo().unwrap();
    let archive = dir.path().join("folders.fold");
    fold_project::save(&project.snapshot(), &archive).unwrap();
    assert_eq!(
        fold_project::load(archive, 16)
            .unwrap()
            .snapshot()
            .state()
            .organization,
        project.snapshot().state().organization
    );
}
#[test]
fn native_drop_imports_and_places_all_targets_in_one_undo() {
    use fold_platform::desktop::{DesktopClient, DesktopCommand};
    let dir = tempfile::tempdir().unwrap();
    let path = still(dir.path(), "source.ppm");
    for (kind, graph) in [
        (fold_timeline::SEQUENCE, false),
        (fold_composition::COMPOSITION, false),
        (fold_compositor::COMPOSITE, true),
    ] {
        let mut project = Project::new(16);
        let target = create(&mut project, kind);
        let before = project.snapshot();
        let mut request = placement(PlacementSource::Asset(Default::default()), target);
        if graph {
            request.graph = Some(fold_platform::browser::GraphPlacement {
                position: [123., 456.],
                group: None,
                object: None,
            });
        }
        let mut session = fold_app::session::Session::new(project);
        session.command(DesktopCommand::Browser(Command::ImportAndPlace {
            paths: vec![path.clone()],
            placement: request.clone(),
        }));
        wait_for_ingest(&mut session);
        assert!(
            session.state().status.is_empty(),
            "{kind}: {}",
            session.state().status
        );
        let after = session.snapshot().unwrap();
        assert_eq!(after.state().assets.len(), 1);
        assert_ne!(
            after.state().documents[&target],
            before.state().documents[&target]
        );
        let archive = dir.path().join("drop.fold");
        fold_project::save(&after, &archive).unwrap();
        assert_eq!(
            fold_project::load(&archive, 16).unwrap().snapshot().state(),
            after.state()
        );
        if kind == fold_timeline::SEQUENCE {
            let reference = DocumentRef {
                document: target,
                output: "video".into(),
                extensions: Default::default(),
            };
            let graph = packages::builtins()
                .video(&after, &reference, Time::ZERO, 64, 36)
                .unwrap();
            assert!(
                fold_render::render(graph)
                    .unwrap()
                    .pixels()
                    .iter()
                    .all(|p| p[0] > 0.9 && p[1] < 0.01)
            );
        }
        if kind == fold_compositor::COMPOSITE {
            let comp = fold_compositor::Composite::from_document(&after.state().documents[&target])
                .unwrap();
            assert!(comp.nodes.iter().any(|n| matches!(
                n.configuration(),
                fold_compositor::OperationConfig::Read { .. }
            ) && n.position == Some([123., 456.])));
        }
        session.command(DesktopCommand::Undo);
        let undone = session.snapshot().unwrap();
        assert!(undone.state().assets.is_empty());
        assert_eq!(undone.state().documents, before.state().documents);
        session.command(DesktopCommand::Redo);
        assert_eq!(session.snapshot().unwrap().state().assets.len(), 1);
        // Reuse preserves existing media when undoing the second placement.
        request.at = Time::new(5, 1).unwrap();
        session.command(DesktopCommand::Browser(Command::ImportAndPlace {
            paths: vec![path.clone()],
            placement: request,
        }));
        wait_for_ingest(&mut session);
        assert!(
            session.state().status.is_empty(),
            "{kind}: {}",
            session.state().status
        );
        assert_eq!(session.snapshot().unwrap().state().assets.len(), 1);
        session.command(DesktopCommand::Undo);
        assert_eq!(
            session.snapshot().unwrap().state().documents,
            after.state().documents
        );
        assert_eq!(session.snapshot().unwrap().state().assets.len(), 1);
    }
}
#[test]
fn rejected_native_drop_leaves_no_import_or_partial_placement() {
    use fold_platform::desktop::{DesktopClient, DesktopCommand};
    let dir = tempfile::tempdir().unwrap();
    let path = still(dir.path(), "source.ppm");
    let mut project = Project::new(16);
    let target = create(&mut project, fold_timeline::SEQUENCE);
    let mut sequence =
        fold_timeline::Sequence::from_document(&project.snapshot().state().documents[&target])
            .unwrap();
    let track = sequence
        .tracks
        .iter_mut()
        .find(|t| t.kind == fold_timeline::TrackKind::Video)
        .unwrap();
    track.locked = true;
    let track = track.id;
    project
        .commit(fold_project::EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![fold_project::Mutation::PutDocument(
                sequence.document(target).unwrap(),
            )],
        })
        .unwrap();
    let before = project.snapshot();
    let mut session = fold_app::session::Session::new(project);
    let mut request = placement(PlacementSource::Asset(Default::default()), target);
    request.track = Some(track);
    session.command(DesktopCommand::Browser(Command::ImportAndPlace {
        paths: vec![path],
        placement: request,
    }));
    wait_for_ingest(&mut session);
    assert!(
        session.state().status.to_lowercase().contains("lock"),
        "{}",
        session.state().status
    );
    assert_eq!(session.snapshot().unwrap().state(), before.state());
}
#[test]
fn multiple_native_images_are_sequential_on_one_new_timeline_track() {
    use fold_platform::desktop::{DesktopClient, DesktopCommand};
    let dir = tempfile::tempdir().unwrap();
    let paths = vec![still(dir.path(), "a.ppm"), still(dir.path(), "b.ppm")];
    let mut project = Project::new(16);
    let target = create(&mut project, fold_timeline::SEQUENCE);
    let mut session = fold_app::session::Session::new(project);
    let mut request = placement(PlacementSource::Asset(Default::default()), target);
    request.new_track = true;
    session.command(DesktopCommand::Browser(Command::ImportAndPlace {
        paths,
        placement: request,
    }));
    wait_for_ingest(&mut session);
    assert!(
        session.state().status.is_empty(),
        "{}",
        session.state().status
    );
    let sequence = fold_timeline::Sequence::from_document(
        &session.snapshot().unwrap().state().documents[&target],
    )
    .unwrap();
    assert_eq!(sequence.clips.len(), 2);
    assert_eq!(sequence.clips[0].track, sequence.clips[1].track);
    assert_eq!(sequence.clips[1].start, sequence.clips[0].end().unwrap());
}
