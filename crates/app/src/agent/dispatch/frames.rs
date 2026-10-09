//! Bounded frame worker. The application's Host owns GPU resources; this worker
//! retains one renderer and immutable observation inputs, never artist state.
use super::{Failure, encode, observe, workers};
use crate::{
    agent::jobs,
    media_workflow::{self, SceneRequest},
};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{DocumentRef, Revision, Snapshot};
use fold_render::gpu::{Host, Renderer};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::sync::{
    Arc, Mutex,
    atomic::{AtomicUsize, Ordering},
    mpsc,
};

pub(super) const MAX_PIXELS: u64 = 1920 * 1080;
pub(super) struct Request {
    pub snapshot: Snapshot,
    pub document: DocumentId,
    pub revision: Revision,
    pub candidate: Option<fold_project::proposals::Handle>,
    pub output: String,
    pub time: Time,
    pub dimensions: [u32; 2],
    pub region: Option<fold_render::region::Region>,
    pub host: Host,
}
pub(super) struct Permit(Arc<AtomicUsize>);
impl Drop for Permit {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::AcqRel);
    }
}
struct Task {
    id: ObjectId,
    request: Request,
    ticket: jobs::Ticket,
    _permit: Permit,
}
pub(super) struct Worker {
    sender: Option<mpsc::Sender<Task>>,
    results: Option<mpsc::Receiver<workers::Completed>>,
    cancellations: Arc<Mutex<std::collections::BTreeMap<ObjectId, fold_media::Cancel>>>,
    thread: Option<std::thread::JoinHandle<()>>,
    admitted: Arc<AtomicUsize>,
}
impl Worker {
    pub fn new(epoch: ObjectId) -> Self {
        let (sender, tasks) = mpsc::channel::<Task>();
        let (completed, results) = mpsc::sync_channel(1);
        let cancellations = Arc::new(Mutex::new(std::collections::BTreeMap::<
            ObjectId,
            fold_media::Cancel,
        >::new()));
        let active = cancellations.clone();
        let thread = std::thread::spawn(move || {
            let mut gpu: Option<(u64, Result<Renderer, String>)> = None;
            let mut decoder = fold_media::Decoder::default();
            for Task {
                id,
                request,
                ticket,
                _permit,
            } in tasks
            {
                let cancel = ticket.cancellation();
                let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
                    if gpu.as_ref().is_none_or(|(id, _)| *id != request.host.id()) {
                        gpu = Some((request.host.id(), Renderer::new(request.host.clone())));
                    }
                    let renderer = gpu
                        .as_mut()
                        .unwrap()
                        .1
                        .as_mut()
                        .map_err(|e| Failure::new("render", e.clone()))?;
                    run(epoch, request, renderer, &mut decoder, &cancel)
                }))
                .unwrap_or_else(|_| {
                    gpu = None;
                    Err(Failure::new(
                        "render",
                        "Frame worker panicked; renderer retired",
                    ))
                });
                let reply = if cancel.check().is_err() {
                    jobs::Completion::Cancelled
                } else {
                    jobs::Completion::Response {
                        bytes: encode(epoch, result).into(),
                        committed: None,
                    }
                };
                active.lock().unwrap().remove(&id);
                if completed
                    .send(workers::Completed {
                        ticket,
                        result: workers::Product::Reply(reply),
                    })
                    .is_err()
                {
                    break;
                }
            }
        });
        Self {
            sender: Some(sender),
            results: Some(results),
            cancellations,
            thread: Some(thread),
            admitted: Arc::new(AtomicUsize::new(0)),
        }
    }
    pub fn reserve(&self) -> Result<Permit, Failure> {
        self.admitted
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |n| {
                (n < 2).then_some(n + 1)
            })
            .map(|_| Permit(self.admitted.clone()))
            .map_err(|_| Failure::new("capacity", "Frame worker admission is full"))
    }
    pub fn submit(&self, request: Request, ticket: jobs::Ticket, permit: Permit) {
        let id = ObjectId::new();
        self.cancellations
            .lock()
            .unwrap()
            .insert(id, ticket.cancellation());
        assert!(
            self.sender
                .as_ref()
                .unwrap()
                .send(Task {
                    id,
                    request,
                    ticket,
                    _permit: permit
                })
                .is_ok()
        );
    }
    pub fn drain(&self) -> impl Iterator<Item = workers::Completed> + '_ {
        self.results.iter().flat_map(|results| results.try_iter())
    }
}
impl Drop for Worker {
    fn drop(&mut self) {
        for cancel in self.cancellations.lock().unwrap().values() {
            cancel.cancel();
        }
        self.sender.take();
        self.results.take();
        // GPU teardown must finish before process/driver shutdown, as in the
        // preview owner. Cancellation prevents queued media work during teardown.
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}
fn run(
    epoch: ObjectId,
    request: Request,
    renderer: &mut Renderer,
    decoder: &mut fold_media::Decoder,
    cancel: &fold_media::Cancel,
) -> Result<Value, Failure> {
    let Request {
        snapshot,
        document,
        revision,
        candidate,
        output,
        time,
        dimensions,
        region,
        host,
    } = request;
    let document_record = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or_else(|| Failure::new("missing_resource", "Missing frame document"))?;
    let source = DocumentRef {
        document,
        output: output.clone(),
        extensions: Default::default(),
    };
    let registry = crate::packages::builtins();
    let info = registry
        .output_ref(&snapshot, &source)
        .map_err(Failure::invalid)?;
    let duration = Time::new(
        i64::from(info.frames) * i64::from(info.rate[1]),
        info.rate[0],
    )
    .map_err(|e| Failure::invalid(e.to_string()))?;
    if time < Time::ZERO || time >= duration {
        return Err(Failure::new(
            "type",
            "Frame time outside output half-open range",
        ));
    }
    let mut provenance = if document_record.type_id == fold_motion::CONSTRUCTION {
        let construction = fold_motion::model::Construction::from_document(document_record)
            .map_err(Failure::invalid)?;
        let scene_time = construction
            .canvas
            .start
            .checked_add(time)
            .map_err(|e| Failure::invalid(e.to_string()))?;
        let measurements = observe::run(
            epoch,
            document_record,
            revision,
            candidate,
            serde_json::from_value(json!(output))
                .map_err(|_| Failure::new("type", "Invalid construction output"))?,
            &[scene_time],
            cancel,
        )?;
        let mut provenance = measurements["provenance"].clone();
        provenance["camera"] = measurements["samples"][0]["value"]["camera"].clone();
        provenance["cameras"] = measurements["samples"][0]["value"]["cameras"].clone();
        provenance["scene_time"] = json!(scene_time);
        provenance["asset_time"] = measurements["samples"][0]["asset_time"].clone();
        provenance
    } else {
        // Mixed outputs may contain several cameras and clocks. Fingerprint the
        // immutable project rather than inventing one scene's provenance.
        json!({"epoch":epoch,"document":document,"revision":revision,
            "candidate":candidate,"output":output,
            "fingerprints":{"project":format!("{:x}", Sha256::digest(
                serde_json::to_vec(snapshot.state()).map_err(|e| Failure::invalid(e.to_string()))?))},
            "resource_inputs":{"scope":"immutable_project","external_content_verified":false}})
    };
    provenance["time"] = json!(time);
    provenance["time_basis"] = json!("output_relative");
    let output_region = region.unwrap_or_else(|| fold_render::region::Region::full(dimensions));
    let output_dimensions = output_region.dimensions();
    provenance["output_size"] = json!(output_dimensions);
    provenance["canvas_size"] = json!(dimensions);
    provenance["region"] = json!(output_region);
    provenance["quality"] = json!("gpu_display_no_history");
    provenance["frames_available"] = json!(true);
    let scene = media_workflow::evaluate_scene_gpu_admitted(
        &snapshot,
        &SceneRequest {
            source,
            time,
            dimensions,
            region,
            preview: None,
        },
        renderer,
        decoder,
        cancel,
        Some(fold_render::scheduling::Class::Prepare),
    )
    .map_err(|e| Failure::new("render", e))?;
    let transfer = json!({"cpu_adapter_nodes":scene.statistics.cpu_adapter_nodes,"evaluation_readback_bytes":scene.statistics.readback_bytes});
    let _permit = fold_render::scheduling::Scheduler::shared()
        .enter(fold_render::scheduling::Class::Prepare, cancel)
        .map_err(|e| Failure::new("cancelled", e))?;
    let mut display = crate::color::with_config(&snapshot, |config| {
        let processor = config
            .map(|c| c.display(fold_color::WORKING_SPACE, &Default::default()))
            .transpose()?;
        renderer.output(&scene, processor.as_ref(), cancel)
    })
    .map_err(|e| Failure::new("render", e))?;
    provenance["display_transform"] = json!(display.transform_identity());
    let pixels = display
        .readback(cancel)
        .map_err(|e| Failure::new("render", e))?;
    drop(_permit);
    let configuration = json!({"profile":"gpu_display_no_history","build":env!("FOLD_SOURCE_SHA256"),"target":env!("FOLD_BUILD_TARGET"),"project_settings":snapshot.state().settings,"display_transform":display.transform_identity(),"adapter":format!("{:?}", host.adapter().get_info()),"spatial":cfg!(feature="spatial"),"spatial_msaa":1,"dimensions":dimensions,"region":output_region});
    provenance["fingerprints"]["configuration"] = json!(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&configuration).unwrap())
    ));
    provenance["configuration"] = configuration;
    // Hex is explicit, dependency-free and bounded below the 16 MiB wire budget.
    let mut data = String::with_capacity(pixels.rgba().len() * 2);
    const HEX: &[u8; 16] = b"0123456789abcdef";
    for row in pixels.rgba().chunks(output_dimensions[0] as usize * 4) {
        cancel.check().map_err(|e| Failure::new("cancelled", e))?;
        for &byte in row {
            data.push(HEX[(byte >> 4) as usize] as char);
            data.push(HEX[(byte & 15) as usize] as char);
        }
    }
    Ok(
        json!({"provenance":provenance,"transfers":transfer,"image":{"encoding":"rgba8_hex","dimensions":output_dimensions,"row_order":"top_to_bottom","presentation":"shared_viewer_display","alpha":"opaque","data":data}}),
    )
}
