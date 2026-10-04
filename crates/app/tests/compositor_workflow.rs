use fold_app::{composition, media_workflow, packages};
use fold_compositor::{Composite, Node, Parameters as P, Source};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::{Cancel, VideoInfo};
use fold_platform::{desktop::PreviewKey, packages::CommandRequest};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use fold_timeline::{Clip, Sequence, SourceMedia, Track, TrackKind};
fn t(n: i64) -> Time {
    Time::new(n, 1).unwrap()
}
fn info() -> VideoInfo {
    VideoInfo {
        width: 16,
        height: 16,
        rate: [24, 1],
        frames: 96,
    }
}
fn reference(document: DocumentId) -> DocumentRef {
    DocumentRef {
        document,
        output: "video".into(),
        extensions: Default::default(),
    }
}
fn solid(rgba: [f32; 4]) -> Composite {
    let node = Node::new(P::Solid { rgba }, vec![]);
    let output = Node::new(P::Output, vec![node.id]);
    Composite {
        info: info(),
        output: output.id,
        nodes: vec![node, output],
        extensions: Default::default(),
    }
}
fn commit(project: &mut Project, documents: Vec<fold_project::Document>) {
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: documents.into_iter().map(Mutation::PutDocument).collect(),
        })
        .unwrap();
}
fn pixels(project: &Project, source: DocumentId, time: Time) -> Vec<[f32; 4]> {
    fold_render::render(
        packages::builtins()
            .video(&project.snapshot(), &reference(source), time, 16, 16)
            .unwrap(),
    )
    .unwrap()
    .pixels()
    .to_vec()
}
fn request<T: serde::Serialize>(project: &Project, id: &str, arguments: &T) -> CommandRequest {
    CommandRequest {
        id: id.into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(arguments).unwrap(),
    }
}
fn apply(project: &mut Project, request: CommandRequest) {
    let batch = packages::builtins()
        .stage(&project.snapshot(), &request, &Cancel::default())
        .unwrap();
    project.commit(batch).unwrap();
}
#[test]
fn registry_rejects_bad_types_cycles_ports_and_dependency_declarations() {
    let mut c = solid([1.0; 4]);
    let mask = Node::new(P::Mask { rect: [0, 0, 8, 8] }, vec![]);
    c.nodes[1].inputs[0] = Some(mask.id);
    c.nodes.insert(1, mask);
    assert!(c.validate().unwrap_err().contains("incompatible"));
    c.nodes[2].inputs[0] = Some(c.nodes[2].id);
    assert!(c.validate().is_err());
    let source = DocumentId::new();
    let read = Node::new(
        P::Read {
            source: Source::Document {
                source: reference(source),
                info: info(),
            },
            start: t(0),
            source_start: t(0),
            duration: t(4),
        },
        vec![],
    );
    c.nodes = vec![
        read.clone(),
        Node {
            id: c.output,
            parameters: P::Output,
            animation: Default::default(),
            inputs: vec![Some(read.id)],
            effect: Default::default(),
            position: None,
            extensions: Default::default(),
        },
    ];
    let mut document = c.document(DocumentId::new()).unwrap();
    document.dependencies.clear();
    assert!(Composite::from_document(&document).is_err());
    let mut p = Project::new(8);
    commit(&mut p, vec![solid([1.0; 4]).document(source).unwrap()]);
    let mut invalid = reference(source);
    invalid.output = "audio".into();
    assert!(
        packages::builtins()
            .video(&p.snapshot(), &invalid, t(0), 16, 16)
            .unwrap_err()
            .contains("port")
    );
    // Project-level cycle rejection is atomic, even when local node graphs are valid.
    let a = DocumentId::new();
    let b = DocumentId::new();
    let mut ca = c.clone();
    let mut cb = c;
    if let P::Read {
        source: Source::Document { source, .. },
        ..
    } = &mut ca.nodes[0].parameters
    {
        *source = reference(b);
    }
    if let P::Read {
        source: Source::Document { source, .. },
        ..
    } = &mut cb.nodes[0].parameters
    {
        *source = reference(a);
    }
    let before = p.snapshot();
    assert!(
        p.commit(EditBatch {
            base: before.revision(),
            mutations: vec![
                Mutation::PutDocument(ca.document(a).unwrap()),
                Mutation::PutDocument(cb.document(b).unwrap())
            ]
        })
        .is_err()
    );
    assert_eq!(p.snapshot().revision(), before.revision());
}
#[test]
fn nested_timing_alpha_cache_persistence_and_atomic_cross_document_undo() {
    let mut project = Project::new(32);
    let source = DocumentId::new();
    let mut inner = solid([0.5, 0.0, 0.0, 0.5]);
    inner.extensions.insert(
        "future.controls".into(),
        serde_json::json!({"opaque": [1, 2, 3]}),
    );
    let sequence_id = DocumentId::new();
    let track = Track::new(TrackKind::Video, "Video");
    let clip = Clip {
        id: ObjectId::new(),
        track: track.id,
        link: None,
        asset: Default::default(),
        info: SourceMedia::Document {
            source: reference(source),
            info: info(),
        },
        start: t(1),
        source_start: t(1),
        duration: t(2),
        level: 0.5,
        extensions: Default::default(),
    };
    let sequence = Sequence {
        dimensions: [16, 16],
        rate: [24, 1],
        tracks: vec![track],
        clips: vec![clip.clone()],
        extensions: Default::default(),
    };
    commit(
        &mut project,
        vec![
            inner.document(source).unwrap(),
            sequence.document(sequence_id).unwrap(),
        ],
    );
    let original = pixels(&project, sequence_id, t(1));
    assert_eq!(original, vec![[0.25, 0.0, 0.0, 1.0]; 256]);
    let req = request(
        &project,
        composition::CREATE,
        &composition::CreateArgs {
            document: sequence_id,
            clips: vec![clip.id],
        },
    );
    apply(&mut project, req);
    assert_eq!(project.snapshot().state().documents.len(), 3);
    assert_eq!(pixels(&project, sequence_id, t(1)), original);
    assert_eq!(
        pixels(&project, sequence_id, t(0)),
        vec![[0.0, 0.0, 0.0, 1.0]; 256]
    );
    assert_eq!(
        pixels(&project, sequence_id, t(3)),
        vec![[0.0, 0.0, 0.0, 1.0]; 256]
    );
    let nested =
        Sequence::from_document(&project.snapshot().state().documents[&sequence_id]).unwrap();
    assert_eq!(nested.clips[0].id, clip.id);
    let hash = media_workflow::content(&project.snapshot()).unwrap();
    project.undo().unwrap();
    assert_eq!(project.snapshot().state().documents.len(), 2);
    assert_eq!(pixels(&project, sequence_id, t(1)), original);
    project.redo().unwrap();
    assert_eq!(media_workflow::content(&project.snapshot()).unwrap(), hash);
    let saved_snapshot = project.snapshot();
    let mut edited = inner;
    edited.nodes[0].parameters = P::Solid {
        rgba: [0.0, 0.5, 0.0, 0.5],
    };
    let req = request(
        &project,
        fold_compositor::package::EDIT,
        &fold_compositor::package::EditArgs {
            document: source,
            composite: edited,
        },
    );
    apply(&mut project, req);
    assert_ne!(media_workflow::content(&project.snapshot()).unwrap(), hash);
    assert_eq!(
        pixels(&project, sequence_id, t(1)),
        vec![[0.0, 0.25, 0.0, 1.0]; 256]
    );
    assert_eq!(
        fold_render::render(
            packages::builtins()
                .video(&saved_snapshot, &reference(sequence_id), t(1), 16, 16)
                .unwrap()
        )
        .unwrap()
        .pixels(),
        original
    );
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("nested.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 32).unwrap();
    assert_eq!(
        media_workflow::content(&reopened.snapshot()).unwrap(),
        media_workflow::content(&project.snapshot()).unwrap()
    );
    let key = PreviewKey {
        output: "video".into(),
        target: None,
        content: media_workflow::content(&reopened.snapshot()).unwrap(),
        frame: 24,
        dimensions: [16, 16],
        view: 1,
        channels: Default::default(),
    };
    assert_eq!(
        media_workflow::evaluate(
            &reopened.snapshot(),
            &key,
            &mut Default::default(),
            &Cancel::default()
        )
        .unwrap()
        .pixels(),
        pixels(&project, sequence_id, t(1))
    );
    // Unrelated documents do not invalidate nested preview content.
    let hash = media_workflow::content(&project.snapshot()).unwrap();
    commit(
        &mut project,
        vec![solid([1.0; 4]).document(DocumentId::new()).unwrap()],
    );
    assert_eq!(media_workflow::content(&project.snapshot()).unwrap(), hash);
}
#[test]
fn unavailable_nested_provider_fails_even_in_an_inactive_read() {
    let mut project = Project::new(4);
    let missing = DocumentId::new();
    let mut unknown = solid([1.0; 4]).document(missing).unwrap();
    unknown.package_id = "missing.package".into();
    let mut c = solid([1.0; 4]);
    let read = Node::new(
        P::Read {
            source: Source::Document {
                source: reference(missing),
                info: info(),
            },
            start: t(2),
            source_start: t(0),
            duration: t(1),
        },
        vec![],
    );
    c.nodes[1].inputs[0] = Some(read.id);
    c.nodes.insert(1, read);
    let id = DocumentId::new();
    commit(&mut project, vec![unknown, c.document(id).unwrap()]);
    assert!(
        packages::builtins()
            .video(&project.snapshot(), &reference(id), t(0), 16, 16)
            .unwrap_err()
            .contains("provider")
    );
}

#[test]
fn legacy_schema_is_readable_and_composition_rejects_locked_audio_without_mutation() {
    let mut project = Project::new(4);
    let video = Track::new(TrackKind::Video, "Video");
    let mut audio = Track::new(TrackKind::Audio, "Audio");
    audio.locked = true;
    let asset = fold_foundation::AssetId::new();
    let clip = Clip {
        id: ObjectId::new(),
        track: video.id,
        link: Some(ObjectId::new()),
        asset,
        info: SourceMedia::Video(info()),
        start: t(0),
        source_start: t(0),
        duration: t(2),
        level: 1.0,
        extensions: Default::default(),
    };
    let mut audio_clip = clip.clone();
    audio_clip.id = ObjectId::new();
    audio_clip.track = audio.id;
    let sequence = Sequence {
        dimensions: [16, 16],
        rate: [24, 1],
        tracks: vec![video, audio],
        clips: vec![clip.clone(), audio_clip],
        extensions: Default::default(),
    };
    let id = DocumentId::new();
    let mut legacy = sequence.document(id).unwrap();
    legacy.schema_version = 2;
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutAsset(fold_project::Asset {
                    id: asset,
                    location: "not-decoded.mp4".into(),
                    fingerprint: "fixture".into(),
                    extensions: Default::default(),
                }),
                Mutation::PutDocument(legacy),
            ],
        })
        .unwrap();
    assert!(fold_timeline::has_sequence(&project.snapshot()));
    assert!(packages::builtins().supports(&project.snapshot().state().documents[&id]));
    assert!(fold_timeline::active(&project.snapshot()).is_ok());
    let req = request(
        &project,
        composition::CREATE,
        &composition::CreateArgs {
            document: id,
            clips: vec![clip.id],
        },
    );
    let revision = project.snapshot().revision();
    assert!(
        packages::builtins()
            .stage(&project.snapshot(), &req, &Cancel::default())
            .unwrap_err()
            .contains("locked")
    );
    assert_eq!(project.snapshot().revision(), revision);
}

#[test]
fn nested_read_offsets_compose_exactly_with_half_open_fractional_boundaries() {
    let mut project = Project::new(4);
    let leaf = DocumentId::new();
    let inner = DocumentId::new();
    let outer = DocumentId::new();
    let tick = Time::new(1001, 24000).unwrap();
    let twice = tick.checked_add(tick).unwrap();
    let make = |source, start, source_start| {
        let read = Node::new(
            P::Read {
                source: Source::Document {
                    source: reference(source),
                    info: info(),
                },
                start,
                source_start,
                duration: tick,
            },
            vec![],
        );
        let output = Node::new(P::Output, vec![read.id]);
        Composite {
            info: info(),
            nodes: vec![read, output.clone()],
            output: output.id,
            extensions: Default::default(),
        }
    };
    commit(
        &mut project,
        vec![
            solid([1.0, 0.0, 0.0, 1.0]).document(leaf).unwrap(),
            make(leaf, tick, t(0)).document(inner).unwrap(),
            make(inner, twice, tick).document(outer).unwrap(),
        ],
    );
    assert_eq!(pixels(&project, outer, tick), vec![[0.0; 4]; 256]);
    assert_eq!(
        pixels(&project, outer, twice),
        vec![[1.0, 0.0, 0.0, 1.0]; 256]
    );
    assert_eq!(
        pixels(&project, outer, twice.checked_add(tick).unwrap()),
        vec![[0.0; 4]; 256]
    );
}
