//! Focused native stroke cost experiment; readback explicitly synchronizes each frame.
use fold_foundation::{ObjectId, Time};
use fold_motion_canvas::{model::*, tools};
use fold_motion_fframes::{Context, Renderer, Scripts, document, fframes::Svgr};
use std::time::Instant;
struct NoScripts;
impl Scripts for NoScripts {
    fn render<'a>(&mut self, _: ObjectId, _: Context<'a>) -> Result<Svgr<'a>, String> {
        Err("Stroke fixture must not run scripts".into())
    }
}
fn canvas(count: usize) -> Result<Canvas, String> {
    let mut canvas = Canvas {
        width: 640,
        height: 360,
        ..Default::default()
    };
    let columns = if count == 128 { 16 } else { 32 };
    let rows = count / columns;
    let cell = [640. / columns as f64, 360. / rows as f64];
    for index in 0..count {
        let point = |position, incoming, outgoing| Point {
            id: ObjectId::new(),
            position,
            incoming,
            outgoing,
            position_track: None,
            incoming_track: None,
            outgoing_track: None,
            extensions: Default::default(),
        };
        let mut object = Object::new(
            "Stroke",
            Content::Path {
                contours: vec![Contour {
                    id: ObjectId::new(),
                    closed: false,
                    points: vec![
                        point([0., 0.], [0.; 2], [cell[0] * 0.4, -cell[1] * 0.2]),
                        point(
                            [cell[0] * 0.7, cell[1] * 0.5],
                            [-cell[0] * 0.3, cell[1] * 0.2],
                            [0.; 2],
                        ),
                    ],
                    extensions: Default::default(),
                }],
                extensions: Default::default(),
            },
        );
        object.paint.fill = None;
        object.paint.stroke = Some([0.2, 0.9, 0.5, 0.7].into());
        object.paint.stroke_width.value = 2.;
        object.transform.shear.value = 0.25;
        let x = (index % columns) as f64 * cell[0] + 2.;
        object.transform.y.value = (index / columns) as f64 * cell[1] + cell[1] * 0.25;
        tools::set_scalar(&mut object.transform.x, Time::ZERO, x, true);
        tools::set_scalar(
            &mut object.transform.x,
            Time::new(1, 1).unwrap(),
            x + 3.,
            true,
        );
        let id = ObjectId::new();
        canvas.roots.push(id);
        canvas.objects.insert(id, object);
    }
    canvas.validate()?;
    Ok(canvas)
}
fn main() -> Result<(), Box<dyn std::error::Error>> {
    let destination = std::env::args()
        .nth(1)
        .ok_or("Provide evidence JSON path")?;
    let uuid = fold_gpu_sharing::Device::first_uuid()?;
    let mut scenes = vec![];
    for count in [128, 1024] {
        let canvas = canvas(count)?;
        let mut renderer = Renderer::new(uuid, [640, 360], [], 64 * 1024 * 1024, 64 * 1024 * 1024)?;
        let mut samples = vec![];
        let mut stages = vec![];
        for frame in 0..50 {
            let time = Time::new(frame % 30, 30)?;
            let start = Instant::now();
            let timing = renderer.draw(document(&canvas, time, &mut NoScripts)?)?;
            let pixels = renderer.readback()?;
            assert!(pixels.iter().any(|p| p[3] > 0.));
            if frame >= 10 {
                samples.push(start.elapsed().as_secs_f64() * 1000.);
                stages.push(timing);
            }
        }
        let ordered = {
            let mut sorted = samples.clone();
            sorted.sort_by(f64::total_cmp);
            sorted
        };
        scenes.push(serde_json::json!({"curved_strokes":count,"dimensions":[640,360],
            "warm_samples":samples,"median_ms":ordered[20],"p95_ms":ordered[37],"max_ms":ordered[39],
            "cpu_stages":stages}));
    }
    std::fs::write(
        destination,
        serde_json::to_vec_pretty(&serde_json::json!({
        "device_uuid":uuid,"scope":"direct native fframes draw plus explicit Float readback; retained target; excludes application/worker/interop/export",
        "discarded_frames":10,"measured_frames":40,"scenes":scenes}))?,
    )?;
    Ok(())
}
