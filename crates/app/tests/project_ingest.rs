use fold_app::ingest::*;
use fold_foundation::{BinId, DocumentId};
use fold_media::{
    Cancel,
    ingest::{INTERPRETATION_KEY, METADATA_KEY, SourceMetadata},
};
use fold_project::*;
use std::{
    path::{Path, PathBuf},
    time::{Duration, Instant},
};

fn commit(p: &mut Project, mutations: Vec<Mutation>) {
    p.commit(EditBatch {
        base: p.snapshot().revision(),
        mutations,
    })
    .unwrap();
}
fn request(p: &Project, paths: Vec<PathBuf>, destination: Parent) -> ImportRequest {
    ImportRequest {
        base: p.snapshot().revision(),
        paths,
        destination,
    }
}
fn ppm(path: &Path, color: &[u8; 3]) {
    let mut bytes = b"P6\n1 1\n255\n".to_vec();
    bytes.extend(color);
    std::fs::write(path, bytes).unwrap();
}
fn asset_id(id: ItemId) -> fold_foundation::AssetId {
    let ItemId::Asset(id) = id else { panic!() };
    id
}
fn import_one(p: &mut Project, path: &Path) -> fold_foundation::AssetId {
    let result = import(
        &p.snapshot(),
        &request(p, vec![path.into()], Parent::Root),
        &Cancel::default(),
    )
    .unwrap();
    let id = asset_id(result.items[0]);
    result.commit(p).unwrap();
    id
}
fn wait(worker: &mut IngestWorker) -> Result<IngestProposal, String> {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        if let Some(result) = worker.poll() {
            return result;
        }
        assert!(Instant::now() < deadline, "ingest worker timed out");
        std::thread::sleep(Duration::from_millis(2));
    }
}

#[test]
fn asset_only_worker_import_duplicates_history_and_persistence() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("A.ppm");
    ppm(&path, &[10, 20, 30]);
    let mut p = Project::new(20);
    let doc = fold_timeline::import_sequence(
        DocumentId::new(),
        [24, 1],
        &[std::fs::read(&path).unwrap()],
    )
    .unwrap();
    // Authoritative documents/settings are compared byte-for-byte across import.
    let bin = Bin {
        id: BinId::new(),
        name: "Media".into(),
        parent: Parent::Root,
        order: 0,
        extensions: Metadata::new(),
    };
    commit(
        &mut p,
        vec![
            Mutation::PutDocument(doc),
            Mutation::PutBin(bin.clone()),
            Mutation::SetSettings(Metadata::from([(
                "workspace-test".into(),
                serde_json::json!({"delivery": "untouched"}),
            )])),
        ],
    );
    let before = p.snapshot();
    let mut worker = IngestWorker::default();
    let req = request(&p, vec![path.clone(), path.clone()], Parent::Bin(bin.id));
    worker
        .import(p.snapshot().evaluation(), req.clone())
        .unwrap();
    assert!(worker.import(p.snapshot().evaluation(), req).is_err());
    let proposal = wait(&mut worker).unwrap();
    assert_eq!(proposal.items[0], proposal.items[1]);
    let id = asset_id(proposal.items[0]);
    proposal.commit(&mut p).unwrap();
    assert_eq!(p.snapshot().state().documents, before.state().documents);
    assert_eq!(p.snapshot().state().settings, before.state().settings);
    assert_eq!(p.snapshot().state().assets.len(), 1);
    assert_eq!(
        p.snapshot()
            .state()
            .organization
            .items
            .iter()
            .find(|i| i.id == ItemId::Asset(id))
            .unwrap()
            .parent,
        Parent::Bin(bin.id)
    );
    let metadata: SourceMetadata =
        serde_json::from_value(p.snapshot().state().assets[&id].extensions[METADATA_KEY].clone())
            .unwrap();
    assert_eq!(metadata.streams[0].bit_depth, Some(8));
    assert!(
        p.snapshot().state().assets[&id].extensions[INTERPRETATION_KEY]["color_space"].is_null()
    );
    let revision = p.snapshot().revision();
    import(
        &p.snapshot(),
        &request(&p, vec![path.clone()], Parent::Root),
        &Cancel::default(),
    )
    .unwrap()
    .commit(&mut p)
    .unwrap();
    assert_eq!(p.snapshot().revision(), revision); // duplicate is not an undo entry or a move
    let copy = dir.path().join("copy.ppm");
    std::fs::copy(&path, &copy).unwrap();
    let second = import_one(&mut p, &copy);
    assert_ne!(second, id);
    p.undo().unwrap();
    assert_eq!(p.snapshot().state().assets.len(), 1);
    p.redo().unwrap();
    assert_eq!(p.snapshot().state().assets.len(), 2);
    let project_path = dir.path().join("project.fold");
    save(&p.snapshot(), &project_path).unwrap();
    assert_eq!(
        load(project_path, 20).unwrap().snapshot().state(),
        p.snapshot().state()
    );
    assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 3); // no source copies
    ppm(&path, &[30, 20, 10]);
    let req = request(&p, vec![path], Parent::Root);
    assert!(
        import(&p.snapshot(), &req, &Cancel::default())
            .err()
            .unwrap()
            .contains("explicit relink")
    );
}

#[test]
fn cancellation_stale_errors_and_delete_publish_no_partial_state() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("A.ppm");
    ppm(&path, &[1, 2, 3]);
    let mut p = Project::new(20);
    let before = p.snapshot();
    let cancel = Cancel::default();
    let req = request(&p, vec![path.clone()], Parent::Root);
    let proposal = import(&p.snapshot(), &req, &cancel).unwrap();
    cancel.cancel();
    assert!(proposal.commit(&mut p).is_err());
    assert_eq!(p.snapshot().state(), before.state());
    assert!(import(&p.snapshot(), &req, &cancel).is_err());
    let proposal = import(&p.snapshot(), &req, &Cancel::default()).unwrap();
    commit(&mut p, vec![Mutation::SetEnvironment(Metadata::new())]);
    assert!(proposal.commit(&mut p).is_err());
    assert!(import(&p.snapshot(), &req, &Cancel::default()).is_err());
    for missing in [dir.path().join("missing.mp4"), dir.path().join("bad.txt")] {
        std::fs::write(dir.path().join("bad.txt"), b"unsupported").unwrap();
        let req = request(&p, vec![path.clone(), missing], Parent::Root);
        assert!(import(&p.snapshot(), &req, &Cancel::default()).is_err());
        assert!(p.snapshot().state().assets.is_empty());
    }
    let mut worker = IngestWorker::default();
    worker
        .import(
            p.snapshot().evaluation(),
            request(&p, vec![path.clone()], Parent::Root),
        )
        .unwrap();
    worker.cancel();
    match wait(&mut worker) {
        Ok(proposal) => assert!(proposal.commit(&mut p).is_err()),
        Err(e) => assert!(e.contains("cancel")),
    }
    let id = import_one(&mut p, &path);
    let batch = delete_item(&p.snapshot(), ItemId::Asset(id)).unwrap();
    p.commit(batch).unwrap();
    assert!(path.exists());
    assert!(p.snapshot().state().assets.is_empty());
    p.undo().unwrap();
    assert!(p.snapshot().state().assets.contains_key(&id));
}

fn video(path: &Path, color: &str, frames: &str) {
    assert!(
        std::process::Command::new("ffmpeg")
            .args([
                "-v",
                "error",
                "-nostdin",
                "-y",
                "-f",
                "lavfi",
                "-i",
                &format!("color={color}:size=16x16:rate=24"),
                "-frames:v",
                frames,
                "-c:v",
                "libx264",
                "-threads",
                "1",
                "-pix_fmt",
                "yuv420p",
                "-color_range",
                "tv",
                "-colorspace",
                "bt709",
                "-color_trc",
                "bt709",
                "-color_primaries",
                "bt709"
            ])
            .arg(path)
            .status()
            .unwrap()
            .success()
    );
}

#[test]
fn referenced_relink_validates_providers_invalidates_content_and_preserves_pinned_source() {
    let dir = tempfile::tempdir().unwrap();
    let old_path = dir.path().join("old.mp4");
    let new_path = dir.path().join("new.mp4");
    let short_path = dir.path().join("short.mp4");
    video(&old_path, "red", "4");
    video(&new_path, "blue", "4");
    video(&short_path, "blue", "2");
    let mut p = Project::new(20);
    let id = import_one(&mut p, &old_path);
    let original_asset = p.snapshot().state().assets[&id].clone();
    let metadata: SourceMetadata =
        serde_json::from_value(original_asset.extensions[METADATA_KEY].clone()).unwrap();
    assert_eq!(metadata.streams[0].facts["color_primaries"], "bt709");
    assert_eq!(metadata.streams[0].planes, Some(3));
    let fold_media::ingest::SourceProfile::Video(info) = metadata.profile.clone() else {
        panic!()
    };
    let document = fold_timeline::VideoLayers {
        assets: vec![id],
        info: info.clone(),
        foreground_opacity: 1.0,
        extensions: Metadata::new(),
    }
    .document(DocumentId::new())
    .unwrap();
    let doc_id = document.id;
    let read = fold_compositor::Node::new(
        fold_compositor::Parameters::Read {
            source: fold_compositor::Source::Asset {
                asset: id,
                info: info.clone(),
            },
            start: fold_foundation::Time::ZERO,
            source_start: fold_foundation::Time::ZERO,
            duration: info.time(info.frames).unwrap(),
        },
        vec![],
    );
    let output = fold_compositor::Node::new(fold_compositor::Parameters::Output, vec![read.id]);
    let composite = fold_compositor::Composite {
        info: info.clone(),
        output: output.id,
        nodes: vec![read, output],
        extensions: Metadata::new(),
    }
    .document(DocumentId::new())
    .unwrap();
    let composite_id = composite.id;
    commit(
        &mut p,
        vec![
            Mutation::PutDocument(document),
            Mutation::PutDocument(composite),
        ],
    );
    let selection = fold_app::media_workflow::select_output(&p.snapshot(), doc_id).unwrap();
    p.commit(selection).unwrap();
    let before = p.snapshot();
    let content = fold_app::media_workflow::content(&before).unwrap();
    let composite_content = fold_app::media_workflow::content_for(&before, composite_id).unwrap();
    let extra = dir.path().join("unplaced.ppm");
    ppm(&extra, &[2, 3, 4]);
    import_one(&mut p, &extra);
    assert_eq!(p.snapshot().state().documents, before.state().documents);
    assert_eq!(p.snapshot().state().settings, before.state().settings);
    assert_eq!(
        fold_app::media_workflow::content(&p.snapshot()).unwrap(),
        content
    );
    // Organization changes cannot alter evaluation identity.
    let mut item = p
        .snapshot()
        .state()
        .organization
        .items
        .iter()
        .find(|i| i.id == ItemId::Asset(id))
        .unwrap()
        .clone();
    item.name = "Renamed".into();
    commit(&mut p, vec![Mutation::PutItem(item)]);
    assert_eq!(
        fold_app::media_workflow::content(&p.snapshot()).unwrap(),
        content
    );
    let req = RelinkRequest {
        base: p.snapshot().revision(),
        asset: id,
        path: short_path,
    };
    assert!(relink(&p.snapshot(), &req, &Cancel::default()).is_err());
    let source = fold_media::VideoSource {
        path: old_path.clone(),
        fingerprint: original_asset.fingerprint.clone(),
        info,
    };
    let mut decoder = fold_media::Decoder::default();
    let original = decoder
        .decode(
            &source,
            source.info.time(0).unwrap(),
            [16, 16],
            &Cancel::default(),
        )
        .unwrap();
    let request = RelinkRequest {
        base: p.snapshot().revision(),
        asset: id,
        path: new_path,
    };
    relink(&p.snapshot(), &request, &Cancel::default())
        .unwrap()
        .commit(&mut p)
        .unwrap();
    assert_ne!(
        fold_app::media_workflow::content(&p.snapshot()).unwrap(),
        content
    );
    assert_ne!(
        fold_app::media_workflow::content_for(&p.snapshot(), composite_id).unwrap(),
        composite_content
    );
    assert_eq!(p.snapshot().state().documents, before.state().documents);
    assert_eq!(p.snapshot().state().settings, before.state().settings);
    assert_eq!(before.state().assets[&id], original_asset);
    std::fs::remove_file(&old_path).unwrap();
    let pinned = decoder
        .decode(
            &source,
            source.info.time(1).unwrap(),
            [8, 8], // force a cache miss: must use the retained source handle
            &Cancel::default(),
        )
        .unwrap();
    assert_eq!(
        original.linear_pixels().next(),
        pinned.linear_pixels().next()
    );
    assert!(
        delete_item(&p.snapshot(), ItemId::Asset(id))
            .unwrap_err()
            .iter()
            .any(|s| s.contains("authored use"))
    );
    assert!(
        delete_item(&p.snapshot(), ItemId::Document(doc_id))
            .unwrap_err()
            .iter()
            .any(|s| s.contains("delivery"))
    );
    p.undo().unwrap();
    assert_eq!(p.snapshot().state().assets[&id], original_asset);
    p.redo().unwrap();
    assert_ne!(p.snapshot().state().assets[&id], original_asset);
    let archive = dir.path().join("relinked.fold");
    save(&p.snapshot(), &archive).unwrap();
    assert_eq!(
        load(archive, 10).unwrap().snapshot().state(),
        p.snapshot().state()
    );
}

#[test]
fn wave_stream_metadata_and_timeline_relink_are_verified() {
    use fold_foundation::{ObjectId, Time};
    use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
    let dir = tempfile::tempdir().unwrap();
    let paths: Vec<_> = ["a.wav", "b.wav", "short.wav"]
        .iter()
        .map(|n| dir.path().join(n))
        .collect();
    for (index, path) in paths.iter().enumerate() {
        assert!(
            std::process::Command::new("ffmpeg")
                .args([
                    "-v",
                    "error",
                    "-nostdin",
                    "-y",
                    "-f",
                    "lavfi",
                    "-i",
                    &format!("sine=frequency={}:sample_rate=48000", 440 + index * 100),
                    "-t",
                    if index == 2 { "0.1" } else { "0.2" },
                    "-c:a",
                    "pcm_s16le"
                ])
                .arg(path)
                .status()
                .unwrap()
                .success()
        );
    }
    let mut p = Project::new(10);
    let id = import_one(&mut p, &paths[0]);
    let metadata: SourceMetadata =
        serde_json::from_value(p.snapshot().state().assets[&id].extensions[METADATA_KEY].clone())
            .unwrap();
    assert_eq!(metadata.streams[0].kind, "audio");
    assert_eq!(metadata.streams[0].bit_depth, Some(16));
    let fold_media::ingest::SourceProfile::Wave(info) = metadata.profile else {
        panic!()
    };
    let track = Track::new(TrackKind::Audio, "Audio");
    let clip = Clip {
        id: ObjectId::new(),
        track: track.id,
        link: None,
        asset: id,
        duration: info.duration().unwrap(),
        info: SourceMedia::Audio(info),
        start: Time::ZERO,
        source_start: Time::ZERO,
        level: 1.0,
        extensions: Metadata::new(),
    };
    let document = Sequence {
        dimensions: [16, 16],
        rate: [24, 1],
        tracks: vec![track],
        clips: vec![clip],
        extensions: Metadata::new(),
    }
    .document(DocumentId::new())
    .unwrap();
    commit(&mut p, vec![Mutation::PutDocument(document)]);
    let before = p.snapshot();
    let request = RelinkRequest {
        base: p.snapshot().revision(),
        asset: id,
        path: paths[2].clone(),
    };
    assert!(relink(&p.snapshot(), &request, &Cancel::default()).is_err());
    let request = RelinkRequest {
        base: p.snapshot().revision(),
        asset: id,
        path: paths[1].clone(),
    };
    let mut worker = IngestWorker::default();
    worker.relink(p.snapshot().evaluation(), request).unwrap();
    wait(&mut worker).unwrap().commit(&mut p).unwrap();
    assert_eq!(p.snapshot().state().documents, before.state().documents);
    assert_ne!(
        p.snapshot().state().assets[&id].fingerprint,
        before.state().assets[&id].fingerprint
    );
}

#[test]
fn unavailable_provider_prevents_destructive_changes_and_override_survives_relink() {
    let dir = tempfile::tempdir().unwrap();
    let a = dir.path().join("a.ppm");
    let b = dir.path().join("b.ppm");
    ppm(&a, &[0, 0, 0]);
    ppm(&b, &[1, 1, 1]);
    let mut p = Project::new(10);
    let id = import_one(&mut p, &a);
    let mut asset = (*p.snapshot().state().assets[&id]).clone();
    asset.extensions.insert(
        INTERPRETATION_KEY.into(),
        serde_json::json!({"color_space":"custom", "alpha":"opaque", "provenance":"user"}),
    );
    asset
        .extensions
        .insert("vendor".into(), serde_json::json!([1, 2, 3]));
    commit(&mut p, vec![Mutation::PutAsset(asset.clone())]);
    let req = RelinkRequest {
        base: p.snapshot().revision(),
        asset: id,
        path: b.clone(),
    };
    relink(&p.snapshot(), &req, &Cancel::default())
        .unwrap()
        .commit(&mut p)
        .unwrap();
    assert_eq!(
        p.snapshot().state().assets[&id].extensions[INTERPRETATION_KEY],
        asset.extensions[INTERPRETATION_KEY]
    );
    assert_eq!(
        p.snapshot().state().assets[&id].extensions["vendor"],
        asset.extensions["vendor"]
    );
    let unknown = Document {
        id: DocumentId::new(),
        type_id: "future".into(),
        package_id: "unavailable".into(),
        schema_version: 100,
        revision: Revision(0),
        dependencies: vec![],
        assets: vec![],
        payload: vec![1, 255],
        extensions: Metadata::new(),
    };
    commit(&mut p, vec![Mutation::PutDocument(unknown)]);
    assert!(delete_item(&p.snapshot(), ItemId::Asset(id)).is_err());
    let req = RelinkRequest {
        base: p.snapshot().revision(),
        asset: id,
        path: a,
    };
    assert!(
        relink(&p.snapshot(), &req, &Cancel::default())
            .err()
            .unwrap()
            .contains("unavailable provider")
    );
}
