//! Native authoring dispatch with supervised preparation and observation workers.
//! The runtime owner must replace this session on project replacement.
use super::{
    jobs,
    retry::{self, Admission, Tracker},
    wire::{self, Command, Scope},
};
use fold_foundation::ObjectId;
use fold_motion::{model::Construction, source};
use fold_project::proposals::{self as candidates, Proposal};
pub mod native;
use fold_project::{Project, Revision};
use serde_json::{Value, json};
use std::time::Instant;
mod admission;
use admission::construction as operation_budget;
mod errors;
#[cfg(feature = "gpu")]
mod frames;
mod observe;
mod persistence;
mod prepare;
mod workers;
use errors::Failure;

pub struct Dispatcher {
    #[cfg(feature = "gpu")]
    render_host: Option<fold_render::gpu::Host>,
    #[cfg(feature = "gpu")]
    frames: Option<frames::Worker>,
    candidates: candidates::Store<native::Candidate>,
    retries: Tracker,
    jobs: jobs::Jobs,
    workers: Option<workers::Workers>,
    job_scopes: std::collections::BTreeMap<ObjectId, Option<Scope>>,
}
impl Default for Dispatcher {
    fn default() -> Self {
        let candidates = candidates::Store::default();
        let retries = Tracker::new(candidates.epoch());
        let jobs = jobs::Jobs::new(candidates.epoch());
        Self {
            #[cfg(feature = "gpu")]
            render_host: None,
            #[cfg(feature = "gpu")]
            frames: None,
            candidates,
            retries,
            jobs,
            workers: None,
            job_scopes: Default::default(),
        }
    }
}
impl Dispatcher {
    #[cfg(feature = "gpu")]
    pub fn set_render_host(&mut self, host: Option<fold_render::gpu::Host>) {
        self.render_host = host;
    }
    fn frames_available(&self) -> bool {
        #[cfg(feature = "gpu")]
        {
            self.render_host.is_some()
        }
        #[cfg(not(feature = "gpu"))]
        {
            false
        }
    }
    pub fn epoch(&self) -> ObjectId {
        self.candidates.epoch()
    }
    pub fn dispatch(&mut self, project: &mut Project, bytes: &[u8], now: Instant) -> Vec<u8> {
        self.dispatch_with_edit_state(project, bytes, now, false)
    }
    pub(crate) fn dispatch_with_edit_state(
        &mut self,
        project: &mut Project,
        bytes: &[u8],
        now: Instant,
        artist_edit_active: bool,
    ) -> Vec<u8> {
        self.poll(now);
        if bytes.len() > retry::MAX_REQUEST_BYTES {
            return encode(
                self.epoch(),
                Err(Failure::new("budget", "Request exceeds 1 MiB")),
            );
        }
        let header: Value = match serde_json::from_slice(bytes) {
            Ok(value) => value,
            Err(error) => {
                return encode(
                    self.epoch(),
                    Err(Failure::new("unsupported", error.to_string())),
                );
            }
        };
        if header.get("version").and_then(Value::as_u64) != Some(2) {
            return encode(
                self.epoch(),
                Err(Failure::new("version", "Wire version 2 is required")),
            );
        }
        // Recognizable request identities are checked before command validation:
        // malformed amendments cannot evade byte-for-byte retry conflict checks.
        let identity = header
            .get("command")
            .and_then(|c| c.get("guard"))
            .and_then(|guard| {
                Some((
                    serde_json::from_value::<ObjectId>(guard.get("epoch")?.clone()).ok()?,
                    serde_json::from_value::<ObjectId>(guard.get("request_id")?.clone()).ok()?,
                ))
            });
        let ticket = if let Some((epoch, request_id)) = identity {
            match self.retries.begin(epoch, request_id, bytes, now) {
                Ok(Admission::Execute(ticket)) => Some(ticket),
                Ok(Admission::Replay(response)) => return response.to_vec(),
                Ok(Admission::Pending) => {
                    return encode(self.epoch(), Ok(json!({"state":"pending"})));
                }
                Err(error) => return encode(self.epoch(), Err(error.into())),
            }
        } else {
            None
        };
        // Parse original bytes so duplicate keys cannot disappear through Value.
        let result = serde_json::from_slice::<wire::Request>(bytes)
            .map_err(|error| Failure::new("unsupported", error.to_string()));
        if let Ok(wire::Request {
            command: Command::JobStatus { handle, wait_ms },
            ..
        }) = &result
        {
            if u128::from(*wait_ms) > jobs::MAX_LONG_POLL.as_millis() {
                return encode(
                    self.epoch(),
                    Err(Failure::new("budget", "Job wait exceeds 25 seconds")),
                );
            }
            return self
                .job_response(*handle, now, false)
                .expect("immediate status");
        }

        let result =
            result.and_then(|request| self.run(project, request.command, now, artist_edit_active));
        let response = encode(self.epoch(), result);
        if let Some(ticket) = ticket {
            // encode always returns a bounded body, including failures.
            self.retries
                .complete(&ticket, response.clone(), now)
                .expect("owned request completion");
        }
        response
    }
    /// Transport waiters ask for no response while running; direct callers use
    /// immediate mode. Retained result bytes never need host-side reserialization.
    pub(crate) fn job_response(
        &mut self,
        handle: jobs::Handle,
        now: Instant,
        wait: bool,
    ) -> Option<Vec<u8>> {
        Some(match self.jobs.status(handle, now) {
            Ok(jobs::Status::Running { .. }) if wait => return None,
            Ok(jobs::Status::Finished(jobs::Completion::Response { bytes, .. })) => bytes.to_vec(),
            Ok(jobs::Status::Finished(jobs::Completion::Cancelled)) => encode(
                self.epoch(),
                Err(Failure::new("cancelled", "Job cancelled")),
            ),
            Ok(jobs::Status::Running {
                cancellation_requested,
            }) => encode(
                self.epoch(),
                Ok(
                    json!({"state":"running","job":handle,"cancellation_requested":cancellation_requested}),
                ),
            ),
            Err(error) => encode(self.epoch(), Err(error.into())),
        })
    }
    /// Host tick: nonblocking, including when no client is polling a job.
    pub fn poll(&mut self, now: Instant) {
        self.jobs.maintain(now);
        let completed: Vec<_> = self
            .workers
            .as_ref()
            .map(|workers| workers.drain().collect())
            .unwrap_or_default();
        #[cfg(feature = "gpu")]
        let completed: Vec<_> = completed
            .into_iter()
            .chain(self.frames.iter().flat_map(|worker| worker.drain()))
            .collect();
        for item in completed {
            let published = matches!(
                &item.result,
                workers::Product::Reply(jobs::Completion::Response {
                    committed: Some(_),
                    ..
                })
            );
            let outcome = if !published && item.ticket.cancellation().check().is_err() {
                jobs::Completion::Cancelled
            } else {
                match item.result {
                    workers::Product::Reply(reply) => reply,
                    workers::Product::Inspection { proof, bytes } => {
                        let bytes = match self.candidates.publish_inspection(proof, now) {
                            Ok(()) => bytes,
                            Err(error) => encode(self.epoch(), Err(error.into())).into(),
                        };
                        jobs::Completion::Response {
                            bytes,
                            committed: None,
                        }
                    }
                    workers::Product::Candidate(candidate) => {
                        let base = candidate.base_revision();
                        let result = self.candidates.insert(*candidate, now)
                            .map(|handle| json!({"handle":handle,"base":base,"inspection_required":true}))
                            .map_err(Failure::from);
                        jobs::Completion::Response {
                            bytes: encode(self.epoch(), result).into(),
                            committed: None,
                        }
                    }
                }
            };
            self.jobs
                .complete(&item.ticket, outcome, now)
                .expect("owned job completion");
        }
        let epoch = self.epoch();
        self.job_scopes.retain(|id, _| {
            self.jobs
                .status(jobs::Handle { epoch, job: *id }, now)
                .is_ok()
        });
    }
    fn submit(
        &mut self,
        scope: impl Into<Option<Scope>>,
        now: Instant,
        work: impl FnOnce(&fold_media::Cancel) -> Result<workers::Output, Failure> + Send + 'static,
    ) -> Result<Value, Failure> {
        let scope = scope.into();
        let epoch = self.epoch();
        let workers = self
            .workers
            .get_or_insert_with(|| workers::Workers::new(epoch));
        let permit = workers.reserve()?;
        let (job, ticket) = self.jobs.start(jobs::MAX_LIFETIME, now)?;
        self.job_scopes.insert(job.job, scope);
        workers.submit(permit, ticket, work);
        Ok(json!({"state":"running","job":job,"scope":scope}))
    }
    fn run(
        &mut self,
        project: &mut Project,
        command: Command,
        now: Instant,
        artist_edit_active: bool,
    ) -> Result<Value, Failure> {
        let snapshot = project.snapshot();
        if let Some((epoch, base)) = command.guard_base() {
            self.check_epoch(epoch)?;
            if base != snapshot.revision().0 {
                return Err(Failure::conflict(
                    json!({"expected":base,"actual":snapshot.revision().0}),
                ));
            }
        }
        match command {
            Command::Discover {} => Ok(json!({
                "wire_version":2,"revision":snapshot.revision(),"request_schema":wire::schema(),
                "documents":snapshot.state().documents.values().map(|document| json!({
                    "id":document.id,"package":document.package_id,"type":document.type_id,
                    "schema":document.schema_version,
                    "native_inspection":(document.package_id == fold_motion::PACKAGE && document.type_id == fold_motion::CONSTRUCTION && document.schema_version == fold_motion::SCHEMA_VERSION) || (native::supported(&document.type_id) && crate::packages::builtins().supports(document))
                })).collect::<Vec<_>>(),
                "catalog":fold_motion::registry::catalog(),
                "project_authoring":{"schema":schemars::schema_for!(native::Edit),"providers":crate::packages::builtins().authoring_catalog().into_iter().filter(|p| p["type"].as_str().is_some_and(native::supported)).collect::<Vec<_>>(),"inspection":"Native provider records are under native; scenes include read-only object_sources. Project candidates inspect all changed documents before atomic Apply."},
                "transport_support":{"linux_local_socket":cfg!(target_os = "linux"),"cli_protocol":cfg!(target_os = "linux"),"mcp_stdio":cfg!(target_os = "linux")},
                "implemented":({ let mut methods = vec!["job_status","cancel_job","save_project_copy","observe","inspect_document","inspect_candidate","propose","rebase","apply","cancel_candidate"]; if self.frames_available() { methods.push("observe_frame"); } methods }),
                "frame_observation":{"available":self.frames_available(),"method":"observe_frame","encoding":"rgba8_hex","max_pixels":2073600,"max_dimension":4096,"in_flight":2,"time_range":"registered output half-open duration","presentation":"shared_viewer_display","regions":true},
                "unavailable":if self.frames_available() { json!({}) } else { json!({"frames":"No shared GPU host attached"}) },
                "limits":{"job_wait_ms":jobs::MAX_LONG_POLL.as_millis(),"cpu_workers":2,"cpu_in_flight":workers::MAX_IN_FLIGHT,"jobs":jobs::MAX_JOBS,"job_lifetime_seconds":jobs::MAX_LIFETIME.as_secs(),"job_retention_seconds":jobs::RETENTION.as_secs(),"request_bytes":retry::MAX_REQUEST_BYTES,"response_bytes":retry::MAX_RESPONSE_BYTES,"observation_samples":64,"evaluation_steps":1000000,"evaluated_elements":1000000,"source_bytes":65536,"patches":128,"operations":10000,"asset_nesting":64,"live_candidates":candidates::MAX_LIVE,"candidate_idle_seconds":candidates::IDLE_LIFETIME.as_secs(),"request_ids":retry::MAX_REQUEST_IDS,"pending_requests":retry::MAX_PENDING,"cached_responses":retry::MAX_CACHED_RESPONSES,"cached_response_bytes":retry::MAX_CACHED_BYTES,"response_retention_seconds":retry::RESPONSE_LIFETIME.as_secs()}
            })),
            Command::JobStatus { .. } => {
                unreachable!("job reads return retained bytes before dispatch")
            }
            Command::CancelJob { guard, handle } => {
                self.jobs.status(handle, now)?;
                if self.job_scopes.get(&handle.job) != Some(&guard.scope()) {
                    return Err(Failure::new("conflict", "Job scope differs"));
                }
                let state = match self.jobs.cancel(handle, now)? {
                    jobs::Cancellation::Requested => "cancellation_requested",
                    jobs::Cancellation::Cancelled => "cancelled",
                    jobs::Cancellation::Finished => "finished",
                    jobs::Cancellation::AlreadyCommitted(_) => "already_committed",
                };
                Ok(json!({"state":state,"job":handle}))
            }
            Command::SaveProjectCopy { guard: _, path } => {
                persistence::validate_path(&path)?;
                self.submit(None, now, move |cancel| {
                    persistence::save(&snapshot, &path, cancel)
                })
            }
            Command::ObserveFrame {
                target,
                output,
                time,
                dimensions,
                region,
            } => {
                #[cfg(not(feature = "gpu"))]
                {
                    let _ = (target, output, time, dimensions, region);
                    Err(Failure::new(
                        "unsupported",
                        "Frame observation requires a GPU build and shared host",
                    ))
                }
                #[cfg(feature = "gpu")]
                {
                    let host = self.render_host.clone().ok_or_else(|| {
                        Failure::new("unsupported", "No shared GPU host attached")
                    })?;
                    if dimensions.contains(&0)
                        || dimensions.iter().any(|&v| v > 4096)
                        || u64::from(dimensions[0]) * u64::from(dimensions[1]) > frames::MAX_PIXELS
                    {
                        return Err(Failure::new(
                            "budget",
                            "Frame dimensions exceed 4096 per axis or 2073600 pixels",
                        ));
                    }
                    let region = region.map(wire::FrameRegion::native);
                    if let Some(region) = region {
                        region
                            .validate(dimensions)
                            .map_err(|e| Failure::new("type", e))?;
                    }
                    let (snapshot, document, revision, candidate) = match target {
                        wire::ObservationTarget::Committed {
                            epoch,
                            revision,
                            document,
                        } => {
                            self.check_epoch(epoch)?;
                            if revision != snapshot.revision().0 {
                                return Err(Failure::conflict(
                                    json!({"expected":revision,"actual":snapshot.revision().0}),
                                ));
                            }
                            (snapshot.evaluation(), document, Revision(revision), None)
                        }
                        wire::ObservationTarget::Candidate { handle } => {
                            let candidate = self.candidates.shared(handle, now)?;
                            (
                                candidate.snapshot().clone(),
                                candidate.document().id,
                                candidate.base_revision(),
                                Some(handle),
                            )
                        }
                    };
                    let epoch = self.epoch();
                    let worker = self
                        .frames
                        .get_or_insert_with(|| frames::Worker::new(epoch));
                    let permit = worker.reserve()?;
                    let (job, ticket) = self.jobs.start(jobs::MAX_LIFETIME, now)?;
                    let scope = Scope {
                        document,
                        definition: None,
                        object: None,
                    };
                    self.job_scopes.insert(job.job, Some(scope));
                    worker.submit(
                        frames::Request {
                            snapshot,
                            document,
                            revision,
                            candidate,
                            output,
                            time,
                            dimensions,
                            region,
                            host,
                        },
                        ticket,
                        permit,
                    );
                    Ok(json!({"state":"running","job":job,"scope":scope}))
                }
            }
            Command::Observe {
                target,
                output,
                times,
            } => {
                let (document, revision, candidate) = match target {
                    wire::ObservationTarget::Committed {
                        epoch,
                        revision,
                        document,
                    } => {
                        self.check_epoch(epoch)?;
                        if revision != snapshot.revision().0 {
                            return Err(Failure::conflict(
                                json!({"expected":revision,"actual":snapshot.revision().0}),
                            ));
                        }
                        let document =
                            snapshot.state().documents.get(&document).ok_or_else(|| {
                                Failure::new("missing_resource", "Missing document")
                            })?;
                        (document.as_ref().clone(), Revision(revision), None)
                    }
                    wire::ObservationTarget::Candidate { handle } => {
                        let revision = self.candidates.base_revision(handle, now)?;
                        (
                            self.candidates.document(handle, now)?.clone(),
                            revision,
                            Some(handle),
                        )
                    }
                };
                if times.is_empty() || times.len() > 64 {
                    return Err(Failure::new("budget", "Observe requires 1..64 exact times"));
                }
                let epoch = self.epoch();
                self.submit(
                    Scope {
                        document: document.id,
                        definition: None,
                        object: None,
                    },
                    now,
                    move |cancel| {
                        observe::run(
                            epoch, &document, revision, candidate, output, &times, cancel,
                        )
                        .map(workers::Output::Value)
                    },
                )
            }
            Command::InspectDocument {
                epoch,
                revision,
                scope,
            } => {
                self.check_epoch(epoch)?;
                if revision != snapshot.revision().0 {
                    return Err(Failure::conflict(
                        json!({"expected":revision,"actual":snapshot.revision().0}),
                    ));
                }
                self.submit(scope, now, move |cancel| {
                    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
                    let original = snapshot.state().documents.get(&scope.document)
                        .ok_or_else(|| Failure::new("missing_resource", "Missing document"))?;
                    let text = if let Some(object) = scope.object {
                        let scene = fold_motion::scene::Scene::from_document(original).map_err(Failure::invalid)?;
                        projection(&scene.object(object).map_err(Failure::invalid)?.graph, scope)?
                    } else if original.type_id == fold_motion::CONSTRUCTION {
                        let value = Construction::from_document(original).map_err(Failure::invalid)?;
                        projection(&value, scope)?
                    } else {
                        if scope.definition.is_some() { return Err(Failure::new("type", "This document uses document scope")); }
                        serde_json::to_string_pretty(&native::inspect(original)?).map_err(|e| Failure::invalid(e.to_string()))?
                    };
                    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
                    Ok(workers::Output::Value(json!({"revision":snapshot.revision(),"scope":scope,"source":text,"rendering":false})))
                })
            }
            Command::InspectCandidate { handle } => {
                let (document, native) = self.candidates.target(handle, now)?;
                let scope = Scope {
                    document,
                    definition: native.definition,
                    object: native.object,
                };
                let task = self.candidates.prepare_inspection(handle, now)?;
                let epoch = self.epoch();
                self.submit(scope, now, move |cancel| {
                    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
                    let (inspection, proof) = task.run()?;
                    let mut result =
                        serde_json::to_value(inspection).expect("candidate inspection");
                    result["scope"] = json!(scope);
                    let bytes =
                        serde_json::to_vec(&json!({"version":2,"epoch":epoch,"result":result}))
                            .expect("inspection response");
                    if bytes.len() > retry::MAX_RESPONSE_BYTES {
                        return Err(Failure::new("budget", "Inspection exceeds 16 MiB"));
                    }
                    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
                    Ok(workers::Output::Inspection {
                        proof,
                        bytes: bytes.into(),
                    })
                })
            }
            Command::Propose { guard, edit } => self.submit(guard.scope, now, move |cancel| {
                let candidate = prepare::run(&snapshot, guard, edit, cancel)?;
                candidate.validate_dependencies(cancel)?;
                Ok(workers::Output::Candidate(Box::new(candidate)))
            }),
            Command::Rebase { guard, handle } => {
                self.check_scope(handle, guard.scope, now)?;
                let candidate = self.candidates.shared(handle, now)?;
                self.submit(guard.scope, now, move |cancel| {
                    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
                    let rebased = candidate.rebase(&snapshot)?;
                    cancel.check().map_err(|e| Failure::new("cancelled", e))?;
                    rebased.validate_budget()?;
                    rebased.validate_dependencies(cancel)?;
                    Ok(workers::Output::Candidate(Box::new(rebased)))
                })
            }
            Command::Apply {
                guard,
                handle,
                receipt,
            } => {
                if artist_edit_active {
                    return Err(Failure::new(
                        "conflict",
                        "Finish the active artist edit before Apply",
                    ));
                }
                self.check_scope(handle, guard.scope, now)?;
                let revision =
                    self.candidates
                        .apply(handle, receipt, Revision(guard.base), project, now)?;
                Ok(json!({"handle":handle,"revision":revision}))
            }
            Command::CancelCandidate { guard, handle } => {
                self.check_scope(handle, guard.scope, now)?;
                let state = self.candidates.cancel(handle, now)?;
                Ok(match state {
                    candidates::Cancellation::Cancelled => {
                        json!({"handle":handle,"state":"cancelled"})
                    }
                    candidates::Cancellation::AlreadyCommitted(revision) => {
                        json!({"handle":handle,"state":"already_committed","revision":revision})
                    }
                })
            }
        }
    }
    fn check_epoch(&self, epoch: ObjectId) -> Result<(), Failure> {
        if epoch != self.epoch() {
            Err(Failure::new("conflict", "Session epoch changed"))
        } else {
            Ok(())
        }
    }
    fn check_scope(
        &mut self,
        handle: candidates::Handle,
        scope: Scope,
        now: Instant,
    ) -> Result<(), Failure> {
        let (document, native) = self.candidates.target(handle, now)?;
        if document != scope.document {
            return Err(Failure::new("conflict", "Candidate document scope differs"));
        }
        if native != scope {
            return Err(Failure::new("conflict", "Candidate asset scope differs"));
        }
        Ok(())
    }
}
fn projection(value: &Construction, scope: Scope) -> Result<String, Failure> {
    scope
        .definition
        .map_or_else(
            || source::canonical(value),
            |id| source::canonical_fragment(value, id),
        )
        .map_err(Failure::invalid)
}

fn encode(epoch: ObjectId, result: Result<Value, Failure>) -> Vec<u8> {
    let value = match result {
        Ok(result) => json!({"version":2,"epoch":epoch,"result":result}),
        Err(error) => {
            json!({"version":2,"epoch":epoch,"error":{"code":error.code,"message":error.message,"details":error.details}})
        }
    };
    let bytes = serde_json::to_vec(&value).expect("JSON response");
    if bytes.len() > retry::MAX_RESPONSE_BYTES {
        return encode(
            epoch,
            Err(Failure::new(
                "budget",
                "Response exceeds 16 MiB; inspect a smaller scope",
            )),
        );
    }
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_foundation::DocumentId;
    use fold_motion::editing;
    use fold_project::{EditBatch, Mutation};
    use std::{sync::mpsc, time::Duration};

    #[test]
    fn save_job_cancellation_distinguishes_prepublication_and_published_results() {
        for publish_first in [false, true] {
            let mut dispatcher = Dispatcher::default();
            let snapshot = Project::new(0).snapshot();
            let expected = snapshot.clone();
            let directory = tempfile::tempdir().unwrap();
            let path = directory.path().join("copy.fold");
            let destination = path.to_str().unwrap().to_owned();
            let (ready, started) = mpsc::channel();
            let (release, resume) = mpsc::channel();
            let now = Instant::now();
            let result = dispatcher
                .submit(None, now, move |cancel| {
                    let published = if publish_first {
                        Some(persistence::save(&snapshot, &destination, cancel)?)
                    } else {
                        None
                    };
                    ready.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(30)).unwrap();
                    match published {
                        Some(result) => Ok(result),
                        None => persistence::save(&snapshot, &destination, cancel),
                    }
                })
                .unwrap_or_else(|_| panic!("save admission"));
            let job: jobs::Handle = serde_json::from_value(result["job"].clone()).unwrap();
            started.recv_timeout(Duration::from_secs(30)).unwrap();
            assert_eq!(path.exists(), publish_first);
            assert_eq!(
                dispatcher.jobs.cancel(job, now),
                Ok(jobs::Cancellation::Requested)
            );
            release.send(()).unwrap();
            let deadline = Instant::now() + Duration::from_secs(30);
            loop {
                dispatcher.poll(Instant::now());
                if let jobs::Status::Finished(outcome) =
                    dispatcher.jobs.status(job, Instant::now()).unwrap()
                {
                    if publish_first {
                        let jobs::Completion::Response { bytes, committed } = outcome else {
                            panic!("published file hidden by cancellation")
                        };
                        assert_eq!(committed, Some(expected.revision()));
                        let response: Value = serde_json::from_slice(&bytes).unwrap();
                        assert_eq!(response["result"]["state"], "saved");
                        assert_eq!(
                            fold_project::load(&path, 0).unwrap().snapshot().state(),
                            expected.state()
                        );
                        assert_eq!(
                            dispatcher.jobs.cancel(job, Instant::now()),
                            Ok(jobs::Cancellation::AlreadyCommitted(expected.revision()))
                        );
                    } else {
                        assert_eq!(outcome, jobs::Completion::Cancelled);
                        assert!(!path.exists());
                    }
                    break;
                }
                assert!(Instant::now() < deadline);
                std::thread::sleep(Duration::from_millis(1));
            }
        }
    }

    #[test]
    fn cancelled_preparation_cannot_publish_a_candidate() {
        let mut dispatcher = Dispatcher::default();
        let mut project = Project::new(16);
        let document = DocumentId::new();
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(
                    fold_motion::package::create(document).unwrap(),
                )],
            })
            .unwrap();
        let snapshot = project.snapshot();
        let mut construction =
            Construction::from_document(&snapshot.state().documents[&document]).unwrap();
        construction.seed = 42;
        let candidate =
            editing::prepare(&snapshot, document, editing::Scope::Document, construction).unwrap();
        let candidate = native::Candidate::Construction(Box::new(candidate));
        let retained = candidate.clone();
        let (ready, started) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let now = Instant::now();
        let result = dispatcher
            .submit(
                Scope {
                    document,
                    definition: None,
                    object: None,
                },
                now,
                move |_| {
                    ready.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    // Simulate work that finishes after cancellation, returning a valid result.
                    Ok(workers::Output::Candidate(Box::new(candidate)))
                },
            )
            .unwrap_or_else(|_| panic!("job admission"));
        let job: jobs::Handle = serde_json::from_value(result["job"].clone()).unwrap();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        assert_eq!(
            dispatcher.jobs.cancel(job, now),
            Ok(jobs::Cancellation::Requested)
        );
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            dispatcher.poll(Instant::now());
            if let jobs::Status::Finished(outcome) =
                dispatcher.jobs.status(job, Instant::now()).unwrap()
            {
                assert!(matches!(outcome, jobs::Completion::Cancelled));
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        // All live slots remain available: the cancelled product never entered the store.
        for _ in 0..candidates::MAX_LIVE {
            dispatcher
                .candidates
                .insert(retained.clone(), Instant::now())
                .unwrap();
        }
        assert_eq!(project.snapshot().revision(), snapshot.revision());
    }
    #[test]
    fn cancelled_inspection_does_not_activate_its_receipt() {
        let mut dispatcher = Dispatcher::default();
        let mut project = Project::new(16);
        let document = DocumentId::new();
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(
                    fold_motion::package::create(document).unwrap(),
                )],
            })
            .unwrap();
        let snapshot = project.snapshot();
        let mut value =
            Construction::from_document(&snapshot.state().documents[&document]).unwrap();
        value.seed = 77;
        let candidate =
            editing::prepare(&snapshot, document, editing::Scope::Document, value).unwrap();
        let now = Instant::now();
        let handle = dispatcher
            .candidates
            .insert(native::Candidate::Construction(Box::new(candidate)), now)
            .unwrap();
        let (inspection, proof) = dispatcher
            .candidates
            .prepare_inspection(handle, now)
            .unwrap()
            .run()
            .unwrap();
        let (ready, started) = mpsc::channel();
        let (release, resume) = mpsc::channel();
        let result = dispatcher
            .submit(
                Scope {
                    document,
                    definition: None,
                    object: None,
                },
                now,
                move |_| {
                    ready.send(()).unwrap();
                    resume.recv_timeout(Duration::from_secs(5)).unwrap();
                    Ok(workers::Output::Inspection {
                        proof,
                        bytes: b"{}".as_slice().into(),
                    })
                },
            )
            .unwrap_or_else(|_| panic!("job admission"));
        let job = serde_json::from_value(result["job"].clone()).unwrap();
        started.recv_timeout(Duration::from_secs(5)).unwrap();
        dispatcher.jobs.cancel(job, now).unwrap();
        release.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            dispatcher.poll(Instant::now());
            if let jobs::Status::Finished(outcome) =
                dispatcher.jobs.status(job, Instant::now()).unwrap()
            {
                assert!(matches!(outcome, jobs::Completion::Cancelled));
                break;
            }
            assert!(Instant::now() < deadline);
            std::thread::sleep(Duration::from_millis(1));
        }
        assert_eq!(
            dispatcher.candidates.apply(
                handle,
                inspection.receipt,
                snapshot.revision(),
                &mut project,
                Instant::now()
            ),
            Err(candidates::Error::Uninspected)
        );
        assert_eq!(project.snapshot().revision(), snapshot.revision());
    }
    #[cfg(target_os = "linux")]
    #[test]
    fn local_waiters_finish_timeout_and_retire_without_blocking_the_owner() {
        use crate::agent::local::{Endpoint, Request};
        let directory = tempfile::tempdir().unwrap();
        let mut endpoint = Endpoint::bind(directory.path()).unwrap();
        let mut dispatcher = Dispatcher::default();
        let now = Instant::now();
        let (job, ticket) = dispatcher.jobs.start(Duration::from_secs(30), now).unwrap();
        let queue = |endpoint: &mut Endpoint, dispatcher: &mut Dispatcher, handle, wait_ms| {
            let (response, receive) = mpsc::sync_channel(1);
            let bytes = serde_json::to_vec(&json!({"version":2,"command":{"method":"job_status","handle":handle,"wait_ms":wait_ms}})).unwrap();
            assert!(
                endpoint
                    .defer(Request { bytes, response }, dispatcher, now)
                    .is_none()
            );
            receive
        };
        let receive = queue(&mut endpoint, &mut dispatcher, job, 100);
        endpoint.poll_waiters(&mut dispatcher, now + Duration::from_millis(1));
        assert!(matches!(receive.try_recv(), Err(mpsc::TryRecvError::Empty)));
        dispatcher
            .jobs
            .complete(
                &ticket,
                jobs::Completion::Response {
                    bytes: b"completed".as_slice().into(),
                    committed: None,
                },
                now,
            )
            .unwrap();
        endpoint.poll_waiters(&mut dispatcher, now + Duration::from_millis(2));
        assert_eq!(receive.try_recv().unwrap(), b"completed");

        let (job, ticket) = dispatcher.jobs.start(Duration::from_secs(30), now).unwrap();
        let receive = queue(&mut endpoint, &mut dispatcher, job, 10);
        endpoint.poll_waiters(&mut dispatcher, now + Duration::from_millis(11));
        let response: Value = serde_json::from_slice(&receive.try_recv().unwrap()).unwrap();
        assert_eq!(response["result"]["state"], "running");
        assert!(ticket.cancellation().check().is_ok());

        let receive = queue(&mut endpoint, &mut dispatcher, job, 100);
        dispatcher.jobs.cancel(job, now).unwrap();
        // Requested cancellation alone does not invent an acknowledged result.
        endpoint.poll_waiters(&mut dispatcher, now);
        assert!(matches!(receive.try_recv(), Err(mpsc::TryRecvError::Empty)));
        dispatcher
            .jobs
            .complete(&ticket, jobs::Completion::Cancelled, now)
            .unwrap();
        endpoint.poll_waiters(&mut dispatcher, now);
        let response: Value = serde_json::from_slice(&receive.try_recv().unwrap()).unwrap();
        assert_eq!(response["error"]["code"], "cancelled");

        let (job, _) = dispatcher.jobs.start(Duration::from_secs(30), now).unwrap();
        let receive = queue(&mut endpoint, &mut dispatcher, job, 100);
        dispatcher = Dispatcher::default();
        endpoint.poll_waiters(&mut dispatcher, now);
        let response: Value = serde_json::from_slice(&receive.try_recv().unwrap()).unwrap();
        assert_eq!(response["error"]["code"], "conflict");

        let (active, _) = dispatcher.jobs.start(Duration::from_secs(30), now).unwrap();
        let waiting: Vec<_> = (0..8)
            .map(|_| queue(&mut endpoint, &mut dispatcher, active, 10))
            .collect();
        let (response, _receive) = mpsc::sync_channel(1);
        let bytes = serde_json::to_vec(
            &json!({"version":2,"command":{"method":"job_status","handle":active,"wait_ms":10}}),
        )
        .unwrap();
        assert!(
            endpoint
                .defer(Request { bytes, response }, &mut dispatcher, now)
                .is_some()
        );
        endpoint.poll_waiters(&mut dispatcher, now + Duration::from_millis(11));
        for receiver in waiting {
            assert!(receiver.try_recv().is_ok());
        }

        let mut project = Project::new(16);
        let bytes = serde_json::to_vec(
            &json!({"version":2,"command":{"method":"job_status","handle":job,"wait_ms":25001}}),
        )
        .unwrap();
        let response: Value =
            serde_json::from_slice(&dispatcher.dispatch(&mut project, &bytes, now)).unwrap();
        assert_eq!(response["error"]["code"], "budget");
    }
}
