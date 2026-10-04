use fold_app::{media_workflow, packages};
use fold_compositor::{Composite, Node, Parameters as P, Source};
use fold_foundation::{DocumentId, Time};
use fold_project::{DocumentRef, EditBatch, Mutation, Project};
use fold_render::view::{Range, View};
fn fixture(path: &std::path::Path) {
    use exr::prelude::*;
    let mut channels: Vec<_> = ["R", "G", "B", "A"]
        .into_iter()
        .map(|name| {
            AnyChannel::new(
                name,
                FlatSamples::F32(vec![if name == "A" { 1. } else { 0.25 }; 6]),
            )
        })
        .collect();
    channels.extend((0..20).map(|i| {
        AnyChannel::new(
            format!("aov{i}.Z").as_str(),
            FlatSamples::F32(vec![1000. + i as f32; 6]),
        )
    }));
    Image::from_layer(Layer::new(
        (3, 2),
        LayerAttributes::default(),
        Encoding::SMALL_LOSSLESS,
        AnyChannels::sort(channels.into_iter().collect()),
    ))
    .write()
    .to_file(path)
    .unwrap();
}
fn project(path: &std::path::Path) -> (Project, DocumentRef) {
    let info = fold_media::VideoInfo {
        width: 3,
        height: 2,
        rate: [24, 1],
        frames: 24,
    };
    let source = Node::new(
        P::Read {
            source: Source::Unassigned { info: info.clone() },
            start: Time::ZERO,
            source_start: Time::ZERO,
            duration: Time::new(1, 1).unwrap(),
        },
        vec![],
    );
    let source_id = source.id;
    let output = Node::new(P::Output, vec![source.id]);
    let id = DocumentId::new();
    let composite = Composite {
        info,
        nodes: vec![source, output.clone()],
        output: output.id,
        extensions: Default::default(),
    };
    let mut project = Project::new(8);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(composite.document(id).unwrap())],
        })
        .unwrap();
    let request = fold_platform::packages::CommandRequest {
        id: "fold.app.read-source".into(),
        base: project.snapshot().revision(),
        arguments: serde_json::to_vec(
            &serde_json::json!({"document":id,"node":source_id,"path":path}),
        )
        .unwrap(),
    };
    let batch = packages::builtins()
        .stage(&project.snapshot(), &request, &Default::default())
        .unwrap();
    project.commit(batch).unwrap();
    (
        project,
        DocumentRef {
            document: id,
            output: "video".into(),
            extensions: Default::default(),
        },
    )
}
fn request(source: DocumentRef, view: View) -> media_workflow::SceneRequest {
    media_workflow::SceneRequest {
        preview: Some(view),
        source,
        time: Time::ZERO,
        dimensions: [3, 2],
    }
}
#[test]
fn read_picker_transaction_preserves_named_channels_and_undoes_import_atomically() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("aovs.exr");
    fixture(&path);
    let (mut project, source) = project(&path);
    assert_eq!(project.snapshot().state().assets.len(), 1);
    let channels = packages::builtins()
        .channels(&project.snapshot(), &source)
        .unwrap();
    assert_eq!(channels.len(), 24);
    let range = Range::new(1000., 1020.).unwrap();
    let scene = media_workflow::evaluate_scene(
        &project.snapshot(),
        &request(
            source.clone(),
            View::Channel {
                name: "aov19.Z".to_owned().try_into().unwrap(),
                range,
            },
        ),
        &mut Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(scene.pixels(), &[[1019., 1019., 1019., 1.]; 6]);
    assert_eq!(
        &scene.to_data_display(range).unwrap().rgba()[..4],
        &[242, 242, 242, 255]
    );
    let beauty = media_workflow::evaluate_scene(
        &project.snapshot(),
        &request(source.clone(), View::default()),
        &mut Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(beauty.pixels(), &[[0.25, 0.25, 0.25, 1.]; 6]);
    project.undo().unwrap();
    assert!(project.snapshot().state().assets.is_empty());
    project.redo().unwrap();
    assert_eq!(project.snapshot().state().assets.len(), 1);
    let snapshot = project.snapshot();
    let document = &snapshot.state().documents[&source.document];
    let decoded = Composite::from_document(document).unwrap();
    assert_eq!(decoded.channel_names(decoded.output).unwrap(), channels);
}
#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native GPU"]
fn twenty_aovs_decode_only_the_selected_source_tuple_and_cache_one_display_image() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("aovs.exr");
    fixture(&path);
    let (project, source) = project(&path);
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(2 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    for view in [
        View::default(),
        View::Channel {
            name: "rgba.alpha".to_owned().try_into().unwrap(),
            range: Range::default(),
        },
        View::Channel {
            name: "aov19.Z".to_owned().try_into().unwrap(),
            range: Range::new(1000., 1020.).unwrap(),
        },
    ] {
        let scene = media_workflow::evaluate_scene_gpu(
            &project.snapshot(),
            &request(source.clone(), view.clone()),
            &mut renderer,
            &mut decoder,
            &cancel,
        )
        .unwrap();
        assert_eq!(scene.statistics.cpu_adapter_nodes, 1, "{view:?}");
        let mut display = match view {
            View::Channel { range, .. } => renderer.output_data(&scene, range, &cancel).unwrap(),
            _ => renderer.output(&scene, None, &cancel).unwrap(),
        };
        assert_eq!(display.storage_bytes(), 24);
        assert_eq!(display.readback(&cancel).unwrap().rgba().len(), 24);
    }
}

#[test]
fn nested_composites_preserve_channel_catalogs_values_and_inactive_frames() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("aovs.exr");
    fixture(&path);
    let (mut project, child) = project(&path);
    let info = Composite::from_document(&project.snapshot().state().documents[&child.document])
        .unwrap()
        .info;
    let read = Node::new(
        P::Read {
            source: Source::Document {
                source: child.clone(),
                info: info.clone(),
            },
            start: Time::new(1, 24).unwrap(),
            source_start: Time::ZERO,
            duration: Time::new(23, 24).unwrap(),
        },
        vec![],
    );
    let output = Node::new(P::Output, vec![read.id]);
    let parent = DocumentRef {
        document: DocumentId::new(),
        output: "video".into(),
        extensions: Default::default(),
    };
    let graph = Composite {
        info,
        nodes: vec![read, output.clone()],
        output: output.id,
        extensions: Default::default(),
    };
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                graph.document(parent.document).unwrap(),
            )],
        })
        .unwrap();
    let registry = packages::builtins();
    let names = registry.channels(&project.snapshot(), &parent).unwrap();
    assert_eq!(
        names,
        registry.channels(&project.snapshot(), &child).unwrap()
    );
    let view = View::Channel {
        name: "aov19.Z".to_owned().try_into().unwrap(),
        range: Range::new(1000., 1020.).unwrap(),
    };
    for (time, value) in [(Time::ZERO, 0.), (Time::new(1, 24).unwrap(), 1019.)] {
        let mut request = request(parent.clone(), view.clone());
        request.time = time;
        let scene = media_workflow::evaluate_scene(
            &project.snapshot(),
            &request,
            &mut Default::default(),
            &Default::default(),
        )
        .unwrap();
        assert_eq!(scene.pixels(), &[[value, value, value, 1.]; 6]);
    }
    // An inactive Read needs the catalog, not a renderable child graph.
    let snapshot = project.snapshot();
    let mut incomplete =
        Composite::from_document(&snapshot.state().documents[&child.document]).unwrap();
    let output_id = incomplete.output;
    incomplete.node_mut(output_id).unwrap().inputs[0] = None;
    project
        .commit(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(
                incomplete.document(child.document).unwrap(),
            )],
        })
        .unwrap();
    let scene = media_workflow::evaluate_scene(
        &project.snapshot(),
        &request(parent.clone(), View::default()),
        &mut Default::default(),
        &Default::default(),
    )
    .unwrap();
    assert_eq!(scene.pixels(), &[[0., 0., 0., 1.]; 6]);
    let mut active = request(parent, View::default());
    active.time = Time::new(1, 24).unwrap();
    assert!(
        media_workflow::evaluate_scene(
            &project.snapshot(),
            &active,
            &mut Default::default(),
            &Default::default()
        )
        .unwrap_err()
        .contains("connect")
    );
}
