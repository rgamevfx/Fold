use fold_app::{packages, settings};
use fold_foundation::{DocumentId, Time};
use fold_platform::{browser::BrowserCommand, settings::*};
use fold_project::{EditBatch, Mutation, Project};
use serde_json::json;

fn defaults(project: &mut Project, width: u32) {
    let mut defaults = ProjectDefaults::default();
    defaults.format.width = width;
    defaults.format.height = 540;
    defaults.format.rate = [30000, 1001];
    defaults.format.frames = 150;
    defaults.cache_folder = "renders/cache".into();
    defaults
        .extensions
        .insert("future".into(), json!({"value":42}));
    let batch = settings::project(
        &project.snapshot(),
        &serde_json::to_vec(&ProjectEdit {
            defaults,
            color_config: None,
        })
        .unwrap(),
        &Default::default(),
    )
    .unwrap();
    project.commit(batch).unwrap();
}
fn create(project: &mut Project, kind: &str) -> DocumentId {
    let before = project.snapshot();
    let registry = packages::builtins();
    let batch = if registry.document_kinds().iter().any(|k| k.type_id == kind) {
        fold_app::browser::edit(
            &before,
            BrowserCommand::Create {
                kind: kind.into(),
                parent: Default::default(),
            },
        )
        .unwrap()
    } else {
        EditBatch {
            base: before.revision(),
            mutations: vec![Mutation::PutDocument(
                registry
                    .create_with_defaults(&before, kind, DocumentId::new())
                    .unwrap(),
            )],
        }
    };
    let id = batch
        .mutations
        .iter()
        .find_map(|m| {
            if let Mutation::PutDocument(d) = m {
                Some(d.id)
            } else {
                None
            }
        })
        .unwrap();
    project.commit(batch).unwrap();
    id
}
fn edit(project: &mut Project, document: DocumentId, settings: DocumentSettings) {
    let batch = fold_app::settings::document(
        &project.snapshot(),
        &serde_json::to_vec(&DocumentEdit { document, settings }).unwrap(),
        &Default::default(),
    )
    .unwrap();
    project.commit(batch).unwrap();
}
#[test]
fn defaults_initialize_all_providers_and_do_not_retroactively_change_items() {
    let mut project = fold_app::color::new_project(32);
    defaults(&mut project, 960);
    let registry = packages::builtins();
    let ids: Vec<_> = [
        fold_composition::COMPOSITION,
        fold_compositor::COMPOSITE,
        fold_timeline::SEQUENCE,
    ]
    .into_iter()
    .map(|kind| create(&mut project, kind))
    .collect();
    for id in &ids {
        let snapshot = project.snapshot();
        let value = registry
            .document_settings(&snapshot.state().documents[id])
            .unwrap()
            .unwrap();
        assert_eq!(value.format.width, 960);
        assert_eq!(value.format.rate, [30000, 1001]);
        if value.timeline {
            assert!(value.output_duration.is_none());
        } else {
            assert_eq!(value.format.frames, 150);
        }
    }
    defaults(&mut project, 800);
    for id in &ids {
        assert_eq!(
            registry
                .document_settings(&project.snapshot().state().documents[id])
                .unwrap()
                .unwrap()
                .format
                .width,
            960
        );
    }
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("settings.fold");
    fold_project::save(&project.snapshot(), &path).unwrap();
    let reopened = fold_project::load(&path, 32).unwrap();
    let value = ProjectDefaults::read(&reopened.snapshot()).unwrap();
    assert_eq!(value.cache_folder, "renders/cache");
    assert_eq!(value.extensions["future"], json!({"value":42}));
    assert_eq!(value.format.width, 800);
    project.undo().unwrap();
    assert_eq!(
        ProjectDefaults::read(&project.snapshot())
            .unwrap()
            .format
            .width,
        960
    );
    project.redo().unwrap();
    assert_eq!(
        ProjectDefaults::read(&project.snapshot())
            .unwrap()
            .format
            .width,
        800
    );
}
#[test]
fn format_edits_preserve_payload_extensions_envelopes_and_undo() {
    for kind in [
        fold_composition::COMPOSITION,
        fold_compositor::COMPOSITE,
        fold_timeline::SEQUENCE,
    ] {
        let registry = packages::builtins();
        let mut project = Project::new(32);
        let id = create(&mut project, kind);
        let mut document = (*project.snapshot().state().documents[&id]).clone();
        document
            .extensions
            .insert("future.envelope".into(), json!(42));
        let mut payload: serde_json::Value = serde_json::from_slice(&document.payload).unwrap();
        if payload.get("extensions").is_some() {
            payload["extensions"]["future.payload"] = json!({"preserved":true});
        } else {
            payload["future.payload"] = json!({"preserved":true});
        }
        document.payload = serde_json::to_vec(&payload).unwrap();
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(document)],
            })
            .unwrap();
        let before = project.snapshot();
        let mut value = registry
            .document_settings(&before.state().documents[&id])
            .unwrap()
            .unwrap();
        value.format.width = 640;
        value.format.height = 360;
        value.format.rate = [24000, 1001];
        edit(&mut project, id, value);
        let after = project.snapshot();
        let document = &after.state().documents[&id];
        assert_eq!(document.extensions["future.envelope"], json!(42));
        let payload: serde_json::Value = serde_json::from_slice(&document.payload).unwrap();
        assert_eq!(
            payload.get("extensions").unwrap_or(&payload)["future.payload"],
            json!({"preserved":true})
        );
        assert_eq!(
            registry
                .document_settings(document)
                .unwrap()
                .unwrap()
                .format
                .width,
            640
        );
        project.undo().unwrap();
        assert!(project.snapshot().same_content(&before));
        project.redo().unwrap();
        assert!(project.snapshot().same_content(&after));
    }
}
#[test]
fn timeline_output_range_keeps_clips_and_exact_times() {
    let mut project = Project::new(32);
    let id = create(&mut project, fold_timeline::SEQUENCE);
    let registry = packages::builtins();
    // A nested composition supplies real content without media I/O.
    let child = create(&mut project, fold_composition::COMPOSITION);
    let mut sequence =
        fold_timeline::Sequence::from_document(&project.snapshot().state().documents[&id]).unwrap();
    let info = registry.output(&project.snapshot(), child).unwrap();
    sequence.clips.push(fold_timeline::Clip {
        effects: vec![],
        id: Default::default(),
        track: sequence.tracks[0].id,
        link: None,
        asset: Default::default(),
        info: fold_timeline::SourceMedia::Document {
            source: fold_project::DocumentRef {
                document: child,
                output: "video".into(),
                extensions: Default::default(),
            },
            info,
        },
        start: Time::ZERO,
        source_start: Time::ZERO,
        duration: Time::new(4, 1).unwrap(),
        level: 1.,
        extensions: Default::default(),
    });
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(sequence.document(id).unwrap())],
        })
        .unwrap();
    let clips = serde_json::to_value(&sequence.clips).unwrap();
    let mut value = registry
        .document_settings(&project.snapshot().state().documents[&id])
        .unwrap()
        .unwrap();
    value.format.rate = [30000, 1001];
    value.output_duration = Some(Time::new(2, 1).unwrap());
    edit(&mut project, id, value.clone());
    let snapshot = project.snapshot();
    let result = fold_timeline::Sequence::from_document(&snapshot.state().documents[&id]).unwrap();
    assert_eq!(serde_json::to_value(&result.clips).unwrap(), clips);
    assert_eq!(result.end().unwrap(), Time::new(4, 1).unwrap());
    assert_eq!(
        result.output_duration().unwrap(),
        Some(Time::new(2, 1).unwrap())
    );
    assert_eq!(registry.output(&snapshot, id).unwrap().frames, 60);
    value.output_duration = None;
    edit(&mut project, id, value);
    assert_eq!(
        registry.output(&project.snapshot(), id).unwrap().frames,
        120
    );
}
#[test]
fn invalid_and_stale_settings_are_atomic_and_legacy_color_is_retained() {
    let mut project = Project::new(32);
    let id = create(&mut project, fold_composition::COMPOSITION);
    let before = project.snapshot();
    let registry = packages::builtins();
    let mut value = registry
        .document_settings(&before.state().documents[&id])
        .unwrap()
        .unwrap();
    value.format.width = 0;
    assert!(
        settings::document(
            &before,
            &serde_json::to_vec(&DocumentEdit {
                document: id,
                settings: value.clone()
            })
            .unwrap(),
            &Default::default()
        )
        .is_err()
    );
    assert!(project.snapshot().same_content(&before));
    value.format.width = 640;
    let batch = settings::document(
        &before,
        &serde_json::to_vec(&DocumentEdit {
            document: id,
            settings: value,
        })
        .unwrap(),
        &Default::default(),
    )
    .unwrap();
    defaults(&mut project, 900);
    assert!(project.commit(batch).is_err());
    assert!(
        fold_platform::color::project(&project.snapshot())
            .unwrap()
            .is_none()
    );
    let bad_color = ProjectEdit {
        defaults: ProjectDefaults::default(),
        color_config: Some(String::new()),
    };
    assert!(
        settings::project(
            &project.snapshot(),
            &serde_json::to_vec(&bad_color).unwrap(),
            &Default::default()
        )
        .unwrap_err()
        .contains("Legacy")
    );
}

#[test]
fn shorter_timeline_output_clips_audio_plan_without_trimming_authored_clips() {
    let mut project = Project::new(32);
    let id = create(&mut project, fold_timeline::SEQUENCE);
    let asset = fold_foundation::AssetId::new();
    let mut sequence =
        fold_timeline::Sequence::from_document(&project.snapshot().state().documents[&id]).unwrap();
    let clip = fold_timeline::Clip {
        effects: vec![],
        id: Default::default(),
        track: sequence.tracks[1].id,
        link: None,
        asset,
        info: fold_timeline::SourceMedia::Audio(fold_media::WaveInfo {
            rate: 48000,
            channels: 2,
            frames: 480000,
        }),
        start: Time::ZERO,
        source_start: Time::new(1, 1).unwrap(),
        duration: Time::new(4, 1).unwrap(),
        level: 1.,
        extensions: Default::default(),
    };
    sequence.clips.push(clip.clone());
    let mut later = clip;
    later.id = Default::default();
    later.start = Time::new(5, 1).unwrap();
    sequence.clips.push(later);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![
                Mutation::PutAsset(fold_project::Asset {
                    id: asset,
                    location: "fixture.wav".into(),
                    fingerprint: "fixture".into(),
                    extensions: Default::default(),
                }),
                Mutation::PutDocument(sequence.document(id).unwrap()),
            ],
        })
        .unwrap();
    let mut value = packages::builtins()
        .document_settings(&project.snapshot().state().documents[&id])
        .unwrap()
        .unwrap();
    value.output_duration = Some(Time::new(2, 1).unwrap());
    edit(&mut project, id, value);
    let snapshot = project.snapshot();
    let after = fold_timeline::Sequence::from_document(&snapshot.state().documents[&id]).unwrap();
    assert_eq!(
        serde_json::to_value(&after.clips).unwrap(),
        serde_json::to_value(&sequence.clips).unwrap()
    );
    let plan = after.audio_plan(&snapshot).unwrap();
    assert_eq!(plan.end, 96000);
    assert_eq!(plan.regions.len(), 1);
    assert_eq!(plan.regions[0].end, 96000);
    assert_eq!(plan.regions[0].source_offset, 48000);
}

#[test]
#[ignore = "requires packaged OCIO runtime; set FOLD_COLOR_ROOT"]
fn project_color_configuration_validates_on_worker_and_is_undoable() {
    let mut project = fold_app::color::new_project(32);
    let before = project.snapshot();
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("config.ocio");
    std::fs::write(
        &path,
        include_bytes!("../../../resources/color/config.ocio"),
    )
    .unwrap();
    let mut args = ProjectEdit {
        defaults: ProjectDefaults::default(),
        color_config: Some(path.to_string_lossy().into_owned()),
    };
    args.defaults.cache_folder = "bakes".into();
    let batch = settings::project(
        &before,
        &serde_json::to_vec(&args).unwrap(),
        &Default::default(),
    )
    .unwrap();
    project.commit(batch).unwrap();
    assert!(matches!(
        fold_platform::color::project(&project.snapshot())
            .unwrap()
            .unwrap()
            .config,
        fold_platform::color::ConfigSource::External { .. }
    ));
    project.undo().unwrap();
    assert!(project.snapshot().same_content(&before));
    project.redo().unwrap();
    args.color_config = Some(String::new());
    let batch = settings::project(
        &project.snapshot(),
        &serde_json::to_vec(&args).unwrap(),
        &Default::default(),
    )
    .unwrap();
    project.commit(batch).unwrap();
    assert!(matches!(
        fold_platform::color::project(&project.snapshot())
            .unwrap()
            .unwrap()
            .config,
        fold_platform::color::ConfigSource::Bundled { .. }
    ));
    let valid = project.snapshot();
    args.color_config = Some(
        dir.path()
            .join("missing.ocio")
            .to_string_lossy()
            .into_owned(),
    );
    assert!(
        settings::project(
            &valid,
            &serde_json::to_vec(&args).unwrap(),
            &Default::default()
        )
        .is_err()
    );
    assert!(project.snapshot().same_content(&valid));
}
