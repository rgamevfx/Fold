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
        let reused = matches!(&view, View::Channel { name, .. } if name.as_str() == "rgba.alpha");
        assert_eq!(
            scene.statistics.cpu_adapter_nodes,
            u32::from(!reused),
            "{view:?}"
        );
        assert_eq!(scene.statistics.resident_still_nodes, u32::from(reused));
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

#[cfg(feature = "gpu")]
#[test]
#[ignore = "requires native GPU"]
fn unchanged_exr_reuses_gpu_input_across_time_and_effect_edits() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("cached.exr");
    fixture(&path);
    let (mut project, source) = project(&path);
    let (host, _) = pollster::block_on(fold_render::gpu::Host::headless(2 * 1024 * 1024)).unwrap();
    let mut renderer = fold_render::gpu::Renderer::new(host.clone()).unwrap();
    let cancel = fold_media::Cancel::default();
    let mut decoder = fold_media::Decoder::default();
    let mut demand = request(source.clone(), View::default());
    let mut first = media_workflow::evaluate_scene_gpu(
        &project.snapshot(),
        &demand,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    let original = first.readback(&cancel).unwrap();
    assert_eq!(first.statistics.cpu_adapter_nodes, 1);
    let mut composite =
        Composite::from_document(&project.snapshot().state().documents[&source.document]).unwrap();
    let input = composite.node(composite.output).unwrap().inputs[0].unwrap();
    let grade = Node::new(
        P::ColorGrade {
            settings: fold_render::operations::Grade {
                multiply: [2.; 3],
                ..Default::default()
            },
        },
        vec![input],
    );
    composite.node_mut(composite.output).unwrap().inputs[0] = Some(grade.id);
    composite.nodes.push(grade);
    project
        .commit(EditBatch {
            base: project.snapshot().revision(),
            mutations: vec![Mutation::PutDocument(
                composite.document(source.document).unwrap(),
            )],
        })
        .unwrap();
    demand.time = Time::new(1, 24).unwrap();
    let mut second = media_workflow::evaluate_scene_gpu(
        &project.snapshot(),
        &demand,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    assert_eq!(
        second.statistics.cpu_adapter_nodes, 0,
        "unchanged EXR must not decode again after an effect edit"
    );
    assert_eq!(second.statistics.upload_bytes, 0);
    assert_eq!(
        second.readback(&cancel).unwrap().pixels(),
        &[[0.5, 0.5, 0.5, 1.]; 6]
    );
    assert_eq!(
        first.readback(&cancel).unwrap().pixels(),
        original.pixels(),
        "cached source cannot become writable scratch"
    );
    demand.dimensions = [2, 1];
    let mut resized = media_workflow::evaluate_scene_gpu(
        &project.snapshot(),
        &demand,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    assert_eq!(resized.statistics.cpu_adapter_nodes, 1);
    resized.readback(&cancel).unwrap();
    // The byte cap also evicts before the entry cap: each tuple almost fills
    // the 256 KiB retention allowance on this 2 MiB host.
    for dimensions in [[128, 128], [127, 128], [128, 128]] {
        demand.dimensions = dimensions;
        let mut frame = media_workflow::evaluate_scene_gpu(
            &project.snapshot(),
            &demand,
            &mut renderer,
            &mut decoder,
            &cancel,
        )
        .unwrap();
        assert_eq!(frame.statistics.cpu_adapter_nodes, 1);
        frame.readback(&cancel).unwrap();
        assert!(host.memory().allocated <= host.memory().budget);
    }
    // Selecting more tuples than the retention cap evicts inputs, never held frames.
    for layer in 0..10 {
        let view = View::Channel {
            name: format!("aov{layer}.Z").try_into().unwrap(),
            range: Range::new(1000., 1020.).unwrap(),
        };
        let mut frame = media_workflow::evaluate_scene_gpu(
            &project.snapshot(),
            &request(source.clone(), view),
            &mut renderer,
            &mut decoder,
            &cancel,
        )
        .unwrap();
        frame.readback(&cancel).unwrap();
        assert!(host.memory().allocated <= host.memory().budget);
    }
    demand.dimensions = [3, 2];
    let mut evicted = media_workflow::evaluate_scene_gpu(
        &project.snapshot(),
        &demand,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    assert_eq!(evicted.statistics.cpu_adapter_nodes, 1);
    evicted.readback(&cancel).unwrap();
    let mut warm = media_workflow::evaluate_scene_gpu(
        &project.snapshot(),
        &demand,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .unwrap();
    assert_eq!(warm.statistics.cpu_adapter_nodes, 0);
    warm.readback(&cancel).unwrap();
    assert_eq!(first.readback(&cancel).unwrap().pixels(), original.pixels());
    // Same length and restored mtime must still invalidate the cached input.
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let mut changed = std::fs::read(&path).unwrap();
    *changed.last_mut().unwrap() ^= 1;
    std::fs::write(&path, changed).unwrap();
    std::fs::File::options()
        .write(true)
        .open(&path)
        .unwrap()
        .set_times(std::fs::FileTimes::new().set_modified(modified))
        .unwrap();
    demand.dimensions = [3, 2];
    let error = media_workflow::evaluate_scene_gpu(
        &project.snapshot(),
        &demand,
        &mut renderer,
        &mut decoder,
        &cancel,
    )
    .err()
    .unwrap();
    assert!(error.contains("fingerprint"), "{error}");
    assert!(host.memory().allocated <= host.memory().budget);
}
