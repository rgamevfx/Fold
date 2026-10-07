use fold_foundation::{DocumentId, Time};
use fold_motion::{Motion, authoring as a, graph::Link, nodes::image};
use fold_platform::image_source::{Request, Source};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};

#[test]
fn bake_worker_applies_once_undoes_and_rejects_stale_missing_and_cancelled() {
    let mut project = Project::new(16);
    let source_id = DocumentId::new();
    let target_id = DocumentId::new();
    let mut source = Motion::empty();
    source.info.width = 16;
    source.info.height = 16;
    source.info.frames = 2;
    a::add_content(&mut source, "fold.motion.rectangle").unwrap();
    let mut target = Motion::empty();
    target.info = source.info.clone();
    let input = a::add_node(&mut target, "fold.motion.image").unwrap();
    let request = Request {
        bake: None,
        source: Source::Output(DocumentRef {
            document: source_id,
            output: "video".into(),
            extensions: Default::default(),
        }),
        info: source.info.clone(),
        time: Time::ZERO,
        outside: Default::default(),
        color_space: None,
    };
    a::node(&mut target, input).unwrap().settings = serde_json::to_value(image::Settings {
        sources: vec![image::Entry {
            source: request.source.clone(),
            info: request.info.clone(),
            offset: Time::ZERO,
            extensions: Default::default(),
        }],
        ..Default::default()
    })
    .unwrap();
    let bake = a::add_node(&mut target, "fold.motion.image_bake").unwrap();
    let node = a::node(&mut target, bake).unwrap();
    node.connect("image", input, "image");
    node.settings["resolution"] = 16.into();
    node.settings["format"] = "Float16".into();
    let card = a::add_node(&mut target, "fold.motion.image_card").unwrap();
    a::node(&mut target, card)
        .unwrap()
        .connect("image", bake, "image");
    target.graph.output = Link {
        node: card,
        socket: "content".into(),
    };
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutDocument(source.document(source_id).unwrap()),
                Mutation::PutDocument(target.document(target_id).unwrap()),
            ],
        })
        .unwrap();
    let original = project.snapshot();
    let packages = fold_app::packages::builtins();
    let handler = packages
        .command(fold_platform::image_bake::COMMAND)
        .unwrap()
        .handler;
    let args = serde_json::to_vec(&image::bake::Args {
        document: target_id,
        node: bake,
        group: None,
    })
    .unwrap();
    let cancelled = fold_media::Cancel::default();
    cancelled.cancel();
    assert!(handler(&original, &args, &cancelled).is_err());
    let edit = handler(&original, &args, &Default::default()).unwrap();
    assert_eq!(project.snapshot().revision(), original.revision());
    project.commit(edit).unwrap();
    let baked = project.snapshot();
    let motion = Motion::from_document(&baked.state().documents[&target_id]).unwrap();
    let settings = motion
        .graph
        .node(bake)
        .unwrap()
        .settings::<image::bake::Settings>()
        .unwrap();
    let reference = settings.bake.unwrap();
    let texture =
        fold_platform::image_bake::resolve(&reference, &baked, &request, &Default::default())
            .unwrap();
    assert_eq!(texture.dimensions(), [16, 16]);
    project.undo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        original.state().documents
    );
    project.redo().unwrap();
    assert_eq!(
        project.snapshot().state().documents,
        baked.state().documents
    );
    source.info.frames = 3;
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(source.document(source_id).unwrap())],
        })
        .unwrap();
    assert!(
        fold_platform::image_bake::resolve(
            &reference,
            &project.snapshot(),
            &request,
            &Default::default()
        )
        .unwrap_err()
        .contains("stale")
    );
    std::fs::write(reference.directory.join("manifest.json"), b"{}").unwrap();
    assert!(
        fold_platform::image_bake::resolve(&reference, &baked, &request, &Default::default())
            .unwrap_err()
            .contains("manifest changed")
    );
    std::fs::remove_dir_all(&reference.directory).unwrap();
    assert!(
        fold_platform::image_bake::resolve(&reference, &baked, &request, &Default::default())
            .unwrap_err()
            .contains("missing")
    );
}

#[test]
fn static_bake_keys_distinguish_transparent_time_and_external_source_changes() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("source.ppm");
    std::fs::write(&path, b"P6\n1 1\n255\n\x01\x02\x03").unwrap();
    let mut project = Project::new(8);
    let before = project.snapshot();
    let proposal = fold_app::ingest::import(
        &before,
        &fold_app::ingest::ImportRequest {
            base: before.revision(),
            paths: vec![path.clone()],
            destination: fold_project::Parent::Root,
        },
        &Default::default(),
    )
    .unwrap();
    let fold_project::ItemId::Asset(asset) = proposal.items[0] else {
        panic!()
    };
    proposal.commit(&mut project).unwrap();
    let snapshot = project.snapshot();
    let mut request = Request {
        bake: None,
        source: Source::Asset(asset),
        time: Time::ZERO,
        outside: fold_platform::image_source::Outside::Black,
        color_space: None,
        info: fold_media::VideoInfo {
            width: 1,
            height: 1,
            rate: [30, 1],
            frames: 30,
        },
    };
    let key = |r: &Request| fold_platform::image_bake::key(&snapshot, r).unwrap();
    let original = key(&request);
    request.time = Time::new(1, 30).unwrap();
    assert_eq!(key(&request), original);
    request.time = Time::new(1, 1).unwrap();
    assert_ne!(key(&request), original);
    request.time = Time::ZERO;
    std::fs::write(&path, b"P6\n1 1\n255\n\x04\x05\x06").unwrap();
    assert_ne!(key(&request), original);
}
