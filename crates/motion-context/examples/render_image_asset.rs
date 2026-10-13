//! Project asset -> retained native worker -> shared GPU image qualification.
use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_motion_canvas::{model::Canvas, tools};
use fold_motion_context::runtime::{
    Config, Profile,
    gpu::{GpuRuntime, Snapshot},
};
use fold_project::{Asset, EditBatch, Mutation, Project};
use fold_render::gpu::Host;
use std::{path::PathBuf, time::Duration};
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Expected evidence directory")?,
    );
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("source.ppm");
    let bytes = b"P6\n2 1\n255\n\xff\x00\x00\x00\x00\xff";
    std::fs::write(&path, bytes)?;
    let asset = AssetId::new();
    let object = ObjectId::new();
    let document = DocumentId::new();
    let canvas = Canvas {
        width: 32,
        height: 16,
        roots: vec![object],
        objects: [(object, tools::image(asset, [32, 16])?)].into(),
        ..Default::default()
    };
    let mut project = Project::new(8);
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![
            Mutation::PutAsset(Asset {
                id: asset,
                location: path
                    .canonicalize()?
                    .to_str()
                    .ok_or("Invalid source path")?
                    .into(),
                fingerprint: format!("sha256:{}", fold_media::content_hash(bytes)),
                extensions: Default::default(),
            }),
            Mutation::PutDocument(canvas.document(document)?),
        ],
    })?;
    let cancel = Cancel::default();
    let (host, adapter) = pollster::block_on(Host::headless(64 * 1024 * 1024))?;
    let mut runtime = GpuRuntime::new(
        Config {
            bridge: PathBuf::from(std::env::args().nth(2).ok_or("Expected SDK directory")?)
                .join("crates/motion-fframes"),
            cache: directory.join("cache"),
            rustc: PathBuf::from(
                std::env::args()
                    .nth(3)
                    .ok_or("Expected Rust compiler executable")?,
            ),
            profile: Profile::Release,
            compile_timeout: Duration::from_secs(600),
            frame_timeout: Duration::from_secs(30),
        },
        host.clone(),
        &cancel,
    )?;
    let project_snapshot = project.snapshot();
    let snapshot = Snapshot::from_document(
        ObjectId::new(),
        1,
        &project_snapshot.state().documents[&document],
        Time::ZERO,
        [32, 16],
    )?
    .bind_assets(&project_snapshot)?;
    let mut previous = None;
    for _ in 0..2 {
        let mut frame = runtime
            .render(&snapshot, &[], &cancel)?
            .into_current(snapshot.intent())?;
        if frame.statistics.upload_bytes != 0 || frame.statistics.readback_bytes != 0 {
            return Err("Evaluation used CPU pixel transport".into());
        }
        let readback = frame.readback(&cancel)?;
        let pixels = readback.pixels().to_vec();
        if pixels[4 * 32 + 4][0] < 0.99 || pixels[4 * 32 + 28][2] < 0.99 {
            return Err("Project image color samples failed".into());
        }
        if previous
            .as_ref()
            .is_some_and(|previous| previous != &pixels)
        {
            return Err("Warm image pixels changed".into());
        }
        previous = Some(pixels);
        drop(frame);
        host.poll()?;
    }
    if runtime.statistics().image_snapshots_sent != 1 {
        return Err("Unchanged resources were resent".into());
    }
    std::fs::write(&path, b"P6\n1 1\n255\n\x00\x00\x00")?;
    if runtime.render(&snapshot, &[], &cancel).is_ok() {
        return Err("Changed source reused stale image".into());
    }
    let mut replacement = (*project.snapshot().state().assets[&asset]).clone();
    replacement.fingerprint = format!(
        "sha256:{}",
        fold_media::content_hash(&std::fs::read(&path)?)
    );
    project.commit(EditBatch {
        base: project.snapshot().revision(),
        mutations: vec![Mutation::PutAsset(replacement)],
    })?;
    let replacement_project = project.snapshot();
    let replacement_snapshot = Snapshot::from_document(
        snapshot.intent().epoch,
        2,
        &replacement_project.state().documents[&document],
        Time::ZERO,
        [32, 16],
    )?
    .bind_assets(&replacement_project)?;
    let mut frame = runtime
        .render(&replacement_snapshot, &[], &cancel)?
        .into_current(replacement_snapshot.intent())?;
    let pixels = frame.readback(&cancel)?;
    let center = pixels.pixels()[8 * 32 + 16];
    if center[0] != 0. || center[2] != 0. || center[3] < 0.99 {
        return Err("Relink used stale image pixels".into());
    }
    drop(pixels);
    drop(frame);
    host.poll()?;
    project.commit(fold_motion_canvas::editing::stage(
        &project.snapshot(),
        document,
        &[fold_motion_canvas::editing::Edit::Delete {
            id: object,
            expected: Box::new(canvas.objects[&object].clone()),
        }],
    )?)?;
    let empty_project = project.snapshot();
    let empty_snapshot = Snapshot::from_document(
        snapshot.intent().epoch,
        3,
        &empty_project.state().documents[&document],
        Time::ZERO,
        [32, 16],
    )?
    .bind_assets(&empty_project)?;
    let mut frame = runtime
        .render(&empty_snapshot, &[], &cancel)?
        .into_current(empty_snapshot.intent())?;
    if frame
        .readback(&cancel)?
        .pixels()
        .iter()
        .any(|pixel| pixel.iter().any(|value| *value != 0.))
    {
        return Err("Removed image still rendered".into());
    }
    drop(frame);
    host.poll()?;
    if runtime.statistics().image_snapshots_sent != 3 {
        return Err("Resource update count differs".into());
    }
    let evidence = serde_json::json!({"device":adapter.name,"dimensions":[32,16],"native_project_image":true,
        "warm_pixels_identical":true,"evaluation_cpu_pixel_transport":false,"image_snapshots_sent":runtime.statistics().image_snapshots_sent,
        "changed_source_rejected":true,"relink_changes_pixels":true,"image_removal_clears_pixels":true,"warm_snapshot_count":1});
    std::fs::write(
        directory.join("project-image-evidence.json"),
        serde_json::to_vec_pretty(&evidence)?,
    )?;
    println!("{evidence}");
    Ok(())
}
