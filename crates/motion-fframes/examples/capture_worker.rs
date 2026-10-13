//! Qualify capture framing, exact time, local coordinates and runtime controls
//! through the actual persistent worker without initializing Vulkan.
use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::{
    editing::{self, Edit},
    model::{Canvas, Captured, Content, Control, ControlValue, Scalar},
    protocol::{CaptureResponse, Operation, Request},
    scripts::Context,
    tools,
};
use fold_motion_fframes::{
    Scripts,
    fframes::{Svgr, svgr},
};
use std::{
    io::{BufRead, BufReader, Read, Write},
    process::{Command, Stdio},
};
struct Program;
impl Scripts for Program {
    fn render<'a>(&mut self, _: ObjectId, input: Context<'a>) -> Result<Svgr<'a>, String> {
        let x = input.number("X")?;
        let y = input.seconds() * 10.;
        Ok(svgr!(<circle cx={x} cy={y} r="10"/>))
    }
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().nth(1).as_deref() == Some("--worker") {
        fold_motion_fframes::worker::run(Program)?;
        return Ok(());
    }
    let id = ObjectId::new();
    let control = ObjectId::new();
    let mut object = tools::new_object("script")?;
    object.transform.x.value = 100.;
    object.transform.y.value = 20.;
    if let Content::Script { controls, .. } = &mut object.content {
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
        roots: vec![id],
        objects: [(id, object)].into(),
        ..Default::default()
    };
    let mut child = Command::new(std::env::current_exe()?)
        .arg("--worker")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    let mut input = child.stdin.take().ok_or("Missing worker input")?;
    let mut output = BufReader::new(child.stdout.take().ok_or("Missing worker output")?);
    let fonts = vec![std::fs::read(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../resources/fonts/NotoSans-Regular.ttf"
    ))?];
    let time = Time::new(1, 2)?;
    for sequence in 1..=3 {
        let request = Request {
            id: sequence,
            operation: Operation::Capture { object: id },
            canvas: &canvas,
            time: if sequence == 2 { canvas.duration } else { time },
            dimensions: [canvas.width, canvas.height],
            allocation_limit: 0,
            cache_limit: 64 * 1024 * 1024,
            fonts: if sequence == 1 { Some(&fonts) } else { None },
            images: None,
        };
        serde_json::to_writer(&mut input, &request)?;
        input.write_all(b"\n")?;
        input.flush()?;
        let mut header = String::new();
        output.read_line(&mut header)?;
        let header: CaptureResponse = serde_json::from_str(&header)?;
        assert_eq!(header.id, sequence);
        assert!(header.bytes <= fold_motion_canvas::protocol::MAX_REQUEST_BYTES);
        if sequence == 2 {
            assert!(header.error.is_some());
            assert_eq!(header.bytes, 0);
            continue;
        }
        assert!(header.error.is_none(), "{:?}", header.error);
        let mut bytes = vec![0; header.bytes];
        output.read_exact(&mut bytes)?;
        let captured: Captured = serde_json::from_slice(&bytes)?;
        let isolated = Canvas {
            roots: captured.roots.clone(),
            objects: captured.objects.clone(),
            ..Default::default()
        };
        let ellipse = isolated
            .objects
            .iter()
            .find(|(_, object)| matches!(object.content, Content::Ellipse { .. }))
            .ok_or("Missing captured ellipse")?;
        let expected_x = if sequence == 1 { 42. } else { 84. };
        assert_eq!(
            tools::transforms(&isolated, time)?[ellipse.0].point([0., 0.]),
            [expected_x, 5.]
        );
        assert_ne!(ellipse.1.paint.fill, Some([0., 0., 0., 1.].into()));
        let converted = editing::apply(
            &canvas,
            &[Edit::Capture {
                id,
                expected: Box::new(canvas.objects[&id].clone()),
                value: Box::new(captured),
                time,
            }],
        )?;
        assert_eq!(
            tools::transforms(&converted, time)?[ellipse.0].point([0., 0.]),
            [100. + expected_x, 25.]
        );
        if let Content::Script { controls, .. } = &mut canvas.objects.get_mut(&id).unwrap().content
        {
            if let ControlValue::Number(value) = &mut controls.get_mut(&control).unwrap().value {
                value.value = 84.;
            }
        }
    }
    drop(input);
    assert!(child.wait()?.success());
    println!(
        "{}",
        serde_json::json!({"local_coordinates":true,"inherited_paint":true,"exact_time":[1,2],"runtime_control_update":true,"invalid_time_rejected":true,"recovery_after_error":true,"gpu_required":false})
    );
    Ok(())
}
