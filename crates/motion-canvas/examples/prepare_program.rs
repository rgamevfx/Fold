//! Prepare one real fframes worker program and requests for native/script qualification.
use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::{compilation, model::*};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = PathBuf::from(std::env::args().nth(1).ok_or("Expected output directory")?);
    if destination.exists() {
        return Err("Use a new program directory".into());
    }
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let mut canvas = Canvas {
        width: 640,
        height: 360,
        ..Default::default()
    };
    let mut title = Object::new(
        "Title",
        Content::Text {
            text: "MOTION / 10,000 DOTS".into(),
            font_family: "Noto Sans".into(),
            size: Scalar::new(28.),
            extensions: Default::default(),
        },
    );
    title.transform.x.value = 24.;
    title.transform.y.value = 52.;
    title.paint.fill = Some([0.85, 0.85, 0.92, 1.].into());
    let title_id = ObjectId::new();
    canvas.objects.insert(title_id, title);
    canvas.roots.push(title_id);
    let source = r##"pub fn render<'a>(input: Context<'a>) -> Result<fframes::Svgr<'a>, String> {
    let count = input.number("Count")? as usize;
    let time = input.seconds();
    let dots = (0..count).map(|index| {
        let x = 24. + (index % 200) as f64 * 2.96;
        let y = 112. + (index / 200) as f64 * 2.96;
        let phase = x * 0.018 + y * 0.025 - time * 2.;
        let opacity = 0.2 + 0.8 * (0.5 + 0.5 * phase.sin());
        fframes::svgr!(<circle cx={x} cy={y} r="1.05" fill="#a28bff" opacity={opacity}/> )
    }).collect::<fframes::Svgr<'a>>();
    Ok(fframes::svgr!(<g>{dots}</g>))
}"##;
    let dots = Object::new(
        "Dot Display",
        Content::Script {
            source: source.into(),
            controls: [(
                ObjectId::new(),
                Control {
                    name: "Count".into(),
                    value: ControlValue::Number(Scalar::new(10_000.)),
                    protected: false,
                    extensions: Default::default(),
                },
            )]
            .into(),
            extensions: Default::default(),
        },
    );
    let dots_id = ObjectId::new();
    canvas.objects.insert(dots_id, dots);
    canvas.roots.push(dots_id);
    for (index, width) in [0.5, 1., 1.5, 2.25].into_iter().enumerate() {
        let y = 288. + index as f64 * 17.;
        let mut line = Object::new(
            format!("Bezier stroke {width}px"),
            Content::Path {
                contours: vec![Contour {
                    id: ObjectId::new(),
                    closed: false,
                    extensions: Default::default(),
                    points: vec![
                        Point {
                            position_track: None,
                            incoming_track: None,
                            outgoing_track: None,
                            id: ObjectId::new(),
                            position: [24., y],
                            incoming: [0.; 2],
                            outgoing: [180., -16.],
                            extensions: Default::default(),
                        },
                        Point {
                            position_track: None,
                            incoming_track: None,
                            outgoing_track: None,
                            id: ObjectId::new(),
                            position: [616., y],
                            incoming: [-180., 16.],
                            outgoing: [0.; 2],
                            extensions: Default::default(),
                        },
                    ],
                }],
                extensions: Default::default(),
            },
        );
        line.paint.fill = None;
        line.paint.stroke = Some([0.95, 0.95, 1., 1.].into());
        line.paint.stroke_width.value = width;
        line.paint.cap = Cap::Round;
        let id = ObjectId::new();
        canvas.roots.push(id);
        canvas.objects.insert(id, line);
    }
    let program = compilation::program(
        &canvas,
        &root.join("crates/motion-fframes"),
        "qualification-1",
    )?;
    std::fs::create_dir_all(destination.join("src"))?;
    std::fs::write(destination.join("Cargo.toml"), program.manifest)?;
    std::fs::write(destination.join("src/main.rs"), program.source)?;
    let fonts = vec![std::fs::read(
        root.join("resources/fonts/NotoSans-Regular.ttf"),
    )?];
    let mut requests = String::new();
    for frame in 0..3 {
        for object in canvas.objects.values_mut() {
            if matches!(object.content, Content::Path { .. }) {
                object.transform.x.value = frame as f64 * 0.25;
            }
        }
        let request = serde_json::json!({"id": frame+1, "operation":{"kind":"render"}, "canvas": canvas, "time": Time::new(frame, 30)?, "dimensions": [640, 360], "allocation_limit": 256 * 1024 * 1024, "cache_limit": 64 * 1024 * 1024, "fonts": if frame==0 { Some(&fonts) } else { None }});
        requests.push_str(&serde_json::to_string(&request)?);
        requests.push('\n');
    }
    std::fs::write(destination.join("requests.jsonl"), requests)?;
    std::fs::write(destination.join("binary-name"), program.executable)?;
    println!(
        "{}",
        serde_json::json!({"identity":program.identity,"directory":destination,"objects":canvas.objects.len(),"dots":10_000})
    );
    Ok(())
}
