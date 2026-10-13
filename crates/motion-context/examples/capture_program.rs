//! Qualify real source compilation, retained capture transport and guarded edits.
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::process::Cancel;
use fold_motion_canvas::{model::*, tools};
use fold_motion_context::{
    conversion::Service,
    runtime::{Config, Profile},
};
use std::{
    path::PathBuf,
    time::{Duration, Instant},
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let directory = PathBuf::from(
        std::env::args()
            .nth(1)
            .ok_or("Expected evidence directory")?,
    );
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let cancel = Cancel::default();
    let config = Config {
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
    };
    let id = ObjectId::new();
    let control = ObjectId::new();
    let mut script = tools::new_object("script")?;
    script.transform.x.value = 100.;
    script.transform.y.value = 20.;
    if let Content::Script {
        source, controls, ..
    } = &mut script.content
    {
        *source = r#"pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {
    let x = input.number("X")?;
    let y = input.seconds() * 10.;
    Ok(fframes::svgr!(<circle cx={x} cy={y} r="10"/>))
}"#
        .into();
        controls.insert(
            control,
            Control {
                name: "X".into(),
                value: ControlValue::Number(Scalar::new(42.)),
                protected: false,
                extensions: Default::default(),
            },
        );
    }
    let mut canvas = Canvas {
        width: 640,
        height: 360,
        roots: vec![id],
        objects: [(id, script)].into(),
        ..Default::default()
    };
    let fonts = vec![std::fs::read(
        root.join("resources/fonts/NotoSans-Regular.ttf"),
    )?];
    let service = Service::new(config, fonts);
    let document_id = DocumentId::new();
    let time = Time::new(1, 2)?;
    let mut durations = Vec::new();
    for x in [42., 84.] {
        if let Content::Script { controls, .. } = &mut canvas.objects.get_mut(&id).unwrap().content
        {
            if let ControlValue::Number(value) = &mut controls.get_mut(&control).unwrap().value {
                value.value = x;
            }
        }
        let mut document = canvas.document(document_id)?;
        document
            .extensions
            .insert("vendor.document".into(), serde_json::json!({"keep":true}));
        let original = document.clone();
        let start = Instant::now();
        let prepared = service.prepare(
            &document,
            id,
            time,
            &fold_project::Project::new(1).snapshot(),
            &cancel,
        )?;
        durations.push(start.elapsed().as_secs_f64() * 1000.);
        assert_eq!(document, original);
        assert_eq!(prepared.id, document.id);
        assert_eq!(prepared.revision, document.revision);
        assert_eq!(prepared.extensions, document.extensions);
        let converted = Canvas::from_document(&prepared)?;
        let ellipse = *converted
            .objects
            .iter()
            .find(|(_, object)| matches!(object.content, Content::Ellipse { .. }))
            .ok_or("Missing captured ellipse")?
            .0;
        assert_eq!(
            tools::transforms(&converted, time)?[&ellipse].point([0., 0.]),
            [100. + x, 25.]
        );
        std::fs::create_dir_all(&directory)?;
        std::fs::write(
            directory.join("converted-canvas.json"),
            serde_json::to_vec_pretty(&converted)?,
        )?;
    }
    let report = serde_json::json!({"source_compilation":true,"runtime_control_update":true,"guarded_conversion":true,"immutable_input":true,"document_identity_and_extensions":true,"exact_time":[1,2],"capture_ms":durations,"pixel_readback":false,"scope":"Configured conversion preparation service; public candidate orchestration and artist acceptance remain open"});
    std::fs::write(
        directory.join("capture-service-evidence.json"),
        serde_json::to_vec_pretty(&report)?,
    )?;
    println!("{report}");
    Ok(())
}
