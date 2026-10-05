use super::*;
use crate::automation::{self as api, error, host::Outcome, required, success};
use serde_json::{Value, json};
impl Session {
    fn automation_state(&self, params: &Value) -> Value {
        let offset = params["offset"]
            .as_u64()
            .unwrap_or(0)
            .min(usize::MAX as u64) as usize;
        let limit = params["limit"].as_u64().unwrap_or(100).clamp(1, 200) as usize;
        let snapshot = self.project.snapshot();
        json!({"revision":snapshot.revision(),"project_epoch":self.workspace_epoch,"path":self.state.project_path,"dirty":self.state.dirty,"editing":self.overlay.is_some(),"selection":self.state.selection,
            "document_count":snapshot.state().documents.len(),"asset_count":snapshot.state().assets.len(),"offset":offset,"limit":limit,
            "documents":snapshot.state().documents.values().skip(offset).take(limit).map(|d|json!({"id":d.id,"type":d.type_id,"revision":d.revision,"name":snapshot.state().organization.items.iter().find(|i|i.id==fold_project::ItemId::Document(d.id)).map(|i|&i.name)})).collect::<Vec<_>>(),
            "assets":snapshot.state().assets.values().skip(offset).take(limit).map(|a|json!({"id":a.id,"location":a.location})).collect::<Vec<_>>(),"undo_label":self.project.undo_label(),"redo_label":self.project.redo_label(),"can_undo":self.project.can_undo(),"can_redo":self.project.can_redo()})
    }
    pub(super) fn poll_automation(&mut self) {
        self.automation.poll_start();
        let completed: Vec<_> = self
            .automation
            .jobs
            .iter_mut()
            .filter_map(|(id, job)| {
                let result = match job.receiver.as_ref()?.try_recv() {
                    Ok(r) => r,
                    Err(std::sync::mpsc::TryRecvError::Empty) => return None,
                    Err(_) => Err("worker disconnected".into()),
                };
                job.receiver = None;
                Some((*id, job.epoch, job.cancel.check().is_err(), result))
            })
            .collect();
        for (id, epoch, cancelled, result) in completed {
            let result = if cancelled && !matches!(&result, Ok(Outcome::Saved { .. })) {
                Err("job cancelled".into())
            } else {
                result
            };
            let result=result.and_then(|outcome|match outcome{
                Outcome::Value(value)=>Ok(value),
                Outcome::Edit{batch,mut result,validate_only}=>{
                    if epoch!=self.workspace_epoch || batch.base!=self.project.snapshot().revision(){return Err("revision/project conflict; inspect and rebuild the batch".into());}
                    if self.overlay.is_some() || self.state.file_busy{return Err("user edit or project file operation is active; retry after it completes".into());}
                    if !validate_only{
                        self.project.commit_named(batch,result["label"].as_str().unwrap_or("Agent edit").to_owned()).map_err(|e|e.to_string())?;self.refresh();
                    }
                    result["revision"]=json!(self.project.snapshot().revision());result["committed"]=json!(!validate_only);Ok(result)
                }
                Outcome::Saved{snapshot,path}=>{
                    if epoch!=self.workspace_epoch{return Err("saved pinned snapshot, but live project changed; live association retained".into());}
                    self.saved_project=snapshot.clone();self.state.project_path=Some(path.clone());self.workspace_project=Some(path.to_string_lossy().into_owned());self.state.save_serial+=1;self.workspace_epoch+=1;self.workspace_restore=false;self.refresh();
                    Ok(json!({"saved_revision":snapshot.revision(),"path":path,"dirty":self.state.dirty}))
                }
            });
            if let Some(job) = self.automation.jobs.get_mut(&id) {
                job.result = Some(match result {
                    Ok(v) => success(v),
                    Err(e) => error(e),
                });
            }
        }
        self.state.busy = !self.background.is_empty() || self.automation.file_busy();
        self.state.file_busy =
            self.background.iter().any(|j| !j.export) || self.automation.file_busy();
        for _ in 0..8 {
            let incoming = self
                .automation
                .server
                .as_ref()
                .and_then(|s| s.receiver.try_recv().ok());
            let Some(incoming) = incoming else { break };
            let request = incoming.request;
            let mutation = matches!(request.method.as_str(), "batch" | "undo" | "redo" | "save");
            let replay = if mutation {
                self.automation.replay(&request)
            } else {
                Ok(None)
            };
            let reply = match replay {
                Ok(Some(reply)) => reply,
                Err(e) => error(e),
                Ok(None) => {
                    let reply = match self.automation_call(&request) {
                        Ok(v) => success(v),
                        Err(e) => error(e),
                    };
                    // Cache accepted results and rejected requests alike. A revised proposal gets a new ID.
                    if mutation {
                        self.automation.remember(&request, reply.clone());
                    }
                    reply
                }
            };
            let _ = incoming.reply.try_send(reply);
        }
    }
    fn automation_call(&mut self, request: &api::Request) -> Result<Value, String> {
        let params = &request.params;
        let epoch = self.workspace_epoch;
        match request.method.as_str() {
            "state" => Ok(self.automation_state(params)),
            "catalog" => api::catalog(params),
            "inspect" => {
                let snapshot = self.project.snapshot();
                let params = params.clone();
                self.automation.spawn(epoch, move |_| {
                    api::inspect(&snapshot, &params).map(Outcome::Value)
                })
            }
            "batch" => {
                if self.overlay.is_some() || self.state.file_busy {
                    return Err("finish active edit/file operation before agent mutations".into());
                }
                let batch: api::Batch = fold_platform::authoring::decode(params)?;
                let snapshot = self.project.snapshot();
                if epoch != batch.project_epoch || snapshot.revision() != batch.revision {
                    return Err("revision conflict; inspect current state".into());
                }
                self.automation.spawn(epoch, move |cancel| {
                    let (proposal, result) = api::stage(&snapshot, &batch, &cancel)?;
                    Ok(Outcome::Edit {
                        batch: proposal,
                        result,
                        validate_only: batch.validate_only,
                    })
                })
            }
            "undo" | "redo" => {
                let revision: fold_project::Revision = required(params, "revision")?;
                if required::<u64>(params, "project_epoch")? != epoch
                    || revision != self.project.snapshot().revision()
                    || self.overlay.is_some()
                    || self.state.file_busy
                {
                    return Err("revision conflict or active user/file edit".into());
                }
                if request.method == "undo" {
                    self.project.undo()
                } else {
                    self.project.redo()
                }
                .map_err(|e| e.to_string())?;
                self.refresh();
                Ok(json!({"revision":self.project.snapshot().revision()}))
            }
            "jobs" => {
                if let Some(id) = params.get("job").and_then(Value::as_u64) {
                    self.automation.result(id)
                } else {
                    Ok(json!(self.automation.jobs.iter().map(|(id,j)|json!({"job":id,"status":if j.receiver.is_some(){"pending"}else{"complete"},"ok":j.result.as_ref().and_then(|v|v.get("ok"))})).collect::<Vec<_>>()))
                }
            }
            "cancel" => {
                let id = required(params, "job")?;
                let job = self.automation.jobs.get(&id).ok_or("unknown job")?;
                if job.receiver.is_some() {
                    job.cancel.cancel();
                }
                Ok(json!({"job":id,"cancellation_requested":job.receiver.is_some()}))
            }
            "viewers" => Ok(self.automation.viewers.clone()),
            "capture" => {
                let viewer = required(params, "viewer")?;
                let (tx, rx) = std::sync::mpsc::sync_channel(1);
                let cancel = Cancel::default();
                let token = cancel.clone();
                let accepted = self.automation.spawn(epoch, move |job_cancel| {
                    let start = std::time::Instant::now();
                    loop {
                        if job_cancel.check().is_err() {
                            token.cancel();
                            return Err("capture cancelled".into());
                        }
                        match rx.recv_timeout(std::time::Duration::from_millis(50)) {
                            Ok(result) => {
                                let (frame, metadata): (fold_platform::DisplayFrame, Value) =
                                    result?;
                                let [w, h] = frame.dimensions();
                                return api::images::rgba(w, h, frame.rgba(), metadata)
                                    .map(Outcome::Value);
                            }
                            Err(std::sync::mpsc::RecvTimeoutError::Timeout)
                                if start.elapsed().as_secs() < 15 => {}
                            _ => {
                                token.cancel();
                                return Err("viewer capture unavailable or timed out".into());
                            }
                        }
                    }
                })?;
                self.automation
                    .captures
                    .push(fold_platform::desktop::AgentCapture {
                        viewer,
                        cancel,
                        reply: tx,
                    });
                Ok(accepted)
            }
            "render" => {
                let source: fold_project::DocumentRef = required(params, "source")?;
                let time = required(params, "time")?;
                let dimensions: [u32; 2] = required(params, "dimensions")?;
                if dimensions.contains(&0) || dimensions.iter().any(|n| *n > 4096) {
                    return Err("render dimensions must be 1..4096".into());
                }
                let snapshot = self.project.snapshot();
                let info = crate::packages::builtins().output_ref(&snapshot, &source)?;
                if time < fold_foundation::Time::ZERO || time >= info.time(info.frames)? {
                    return Err("render time outside output range".into());
                }
                #[cfg(feature = "gpu")]
                let host = self.preview.shared.0.lock().unwrap().render_host.clone();
                self.automation.spawn(epoch,move|cancel|{
                    #[cfg(feature="gpu")]let mut renderer=crate::output::OutputRenderer::with_shared_host(host)?;
                    #[cfg(not(feature="gpu"))]let mut renderer=crate::output::OutputRenderer::from_environment()?;
                    let frame=renderer.evaluate(&snapshot,&crate::media_workflow::SceneRequest{source:source.clone(),time,dimensions,region:None,preview:None},&cancel)?;
                    let[w,h]=frame.dimensions();api::images::rgba(w,h,frame.rgba(),json!({"kind":"render_output","revision":snapshot.revision(),"source":source,"time":time,"color":fold_platform::color::output(&snapshot,source.document)?,"alpha":"over black","overlays":false})).map(Outcome::Value)
                })
            }
            "navigate" => {
                let viewer = required(params, "viewer")?;
                let time = required(params, "time")?;
                let mut transport = self.viewer_transport(viewer).ok_or("viewer unavailable")?;
                if let Some(source) = params.get("source") {
                    transport.output = fold_platform::authoring::decode(source)?;
                }
                let info = crate::packages::builtins()
                    .output_ref(&self.project.snapshot(), &transport.output)?;
                if time < fold_foundation::Time::ZERO || time >= info.time(info.frames)? {
                    return Err("time outside viewer source".into());
                }
                transport.time = time;
                transport.playing = false;
                self.configure_viewer(viewer, transport);
                Ok(json!({"viewer":viewer,"requested_time":time}))
            }
            "save" => {
                let revision: fold_project::Revision = required(params, "revision")?;
                if required::<u64>(params, "project_epoch")? != epoch
                    || revision != self.project.snapshot().revision()
                    || self.overlay.is_some()
                    || self.state.file_busy
                {
                    return Err("revision conflict or active edit/file operation".into());
                }
                let path: std::path::PathBuf = required(params, "path")?;
                if !path.is_absolute() {
                    return Err("save requires an absolute path".into());
                }
                let snapshot = self.project.snapshot();
                let accepted = self.automation.spawn(epoch, move |cancel| {
                    cancel.check()?;
                    fold_project::save(&snapshot, &path).map_err(|e| e.to_string())?;
                    Ok(Outcome::Saved { snapshot, path })
                })?;
                self.automation.saving = accepted["job"].as_u64();
                self.state.busy = true;
                self.state.file_busy = true;
                Ok(accepted)
            }
            "screenshot" => self.automation.spawn(epoch, move |cancel| {
                api::images::screenshot(&cancel).map(Outcome::Value)
            }),
            _ => Err("unknown automation method; call catalog".into()),
        }
    }
}
