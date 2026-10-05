use super::*;
use serde_json::{Value, json};
fn describe(key: &PreviewKey) -> Value {
    json!({"target":key.target,"output":key.output,"content":key.content,"frame":key.frame,"dimensions":key.dimensions,"region":key.region.map(|r|json!({"x":r.x,"y":r.y,"width":r.width,"height":r.height})),"view_policy":key.view,"channels":key.channels})
}
impl PreviewHost {
    pub(crate) fn agent_capture(
        &self,
        client: &mut dyn DesktopClient,
        active: Option<PanelInstanceId>,
    ) {
        if client.agent_access()["enabled"] != true {
            return;
        }
        let views:Vec<_>=self.demands.iter().map(|(id,requested)|{
            let displayed=self.held.get(id);
            json!({"viewer":id,"active":Some(*id)==active,"requested":describe(requested),"displayed":displayed.map(describe),"stale":displayed!=Some(requested),"requested_generation":self.generations.get(id),"error":self.viewer_error(*id)})
        }).collect();
        client.agent_viewers(json!(views));
        for request in client.take_agent_captures() {
            let work = (|| {
                let host = self.capture_host.clone().ok_or("render host unavailable")?;
                let displayed = self
                    .held
                    .get(&request.viewer)
                    .ok_or("viewer has no displayed image")?;
                let texture = self
                    .cache
                    .peek(displayed)
                    .ok_or("displayed image has been evicted")?;
                let metadata = json!({"kind":"viewer_capture","observed_project_revision":client.snapshot().map(|s|s.revision()),"transient_user_edit":client.state().transient,"viewer":request.viewer,"displayed":describe(displayed),"requested":self.demands.get(&request.viewer).map(describe),"stale":self.demands.get(&request.viewer)!=Some(displayed),"requested_generation":self.generations.get(&request.viewer),"color":"display-transformed RGBA8; channel/data view is recorded in displayed.channels","alpha":"opaque","overlays":false,"chrome":false,"zoom_pan":"capture is the retained image/crop, not window pixels","source_dimensions":texture.dimensions});
                Ok::<_, String>((
                    host,
                    texture._texture.clone(),
                    texture.gpu_lease.clone(),
                    texture.dimensions,
                    metadata,
                ))
            })();
            match work {
                Ok((host, texture, lease, dimensions, mut metadata)) => {
                    std::thread::spawn(move || {
                        let identity = lease
                            .as_ref()
                            .map(|l| l.transform_identity().to_owned())
                            .unwrap_or_else(|| "viewer CPU upload".into());
                        metadata["transform_identity"] = json!(identity);
                        let result = host.capture_presentation(
                            &texture,
                            dimensions,
                            identity,
                            &request.cancel,
                        );
                        let _ = request
                            .reply
                            .try_send(result.map(|frame| (frame, metadata)));
                        drop(lease);
                    });
                }
                Err(error) => {
                    let _ = request.reply.try_send(Err(error));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use fold_platform::desktop::{DesktopCommand, DesktopState};
    struct Client {
        state: DesktopState,
        viewers: Value,
        captures: Vec<fold_platform::desktop::AgentCapture>,
    }
    impl DesktopClient for Client {
        fn state(&self) -> &DesktopState {
            &self.state
        }
        fn poll(&mut self) {}
        fn command(&mut self, _: DesktopCommand) {}
        fn request_preview(&mut self, _: PreviewKey) {}
        fn cancel_preview(&mut self) {}
        fn take_preview(&mut self) -> Option<fold_platform::desktop::PreviewResult> {
            None
        }
        fn agent_access(&self) -> Value {
            json!({"enabled":true})
        }
        fn agent_viewers(&mut self, v: Value) {
            self.viewers = v;
        }
        fn take_agent_captures(&mut self) -> Vec<fold_platform::desktop::AgentCapture> {
            std::mem::take(&mut self.captures)
        }
    }
    #[test]
    fn capture_reports_held_frame_separately_from_pending_demand() {
        let mut host = PreviewHost::new();
        let id = PanelInstanceId(4);
        let displayed = PreviewKey {
            region: None,
            target: Some((
                fold_foundation::DocumentId::new(),
                fold_foundation::Time::ZERO,
            )),
            output: "video".into(),
            content: "old-content".into(),
            frame: 0,
            dimensions: [16, 16],
            view: 1,
            channels: Default::default(),
        };
        let requested = PreviewKey {
            frame: 24,
            content: "new-content".into(),
            ..displayed.clone()
        };
        host.held.insert(id, displayed);
        host.demands.insert(id, requested);
        host.generations.insert(id, 2);
        let (tx, rx) = std::sync::mpsc::sync_channel(1);
        let mut client = Client {
            state: Default::default(),
            viewers: Value::Null,
            captures: vec![fold_platform::desktop::AgentCapture {
                viewer: id,
                cancel: Default::default(),
                reply: tx,
            }],
        };
        host.agent_capture(&mut client, Some(id));
        assert_eq!(client.viewers[0]["stale"], true);
        assert_eq!(client.viewers[0]["displayed"]["frame"], 0);
        assert_eq!(client.viewers[0]["requested"]["frame"], 24);
        assert_eq!(client.viewers[0]["active"], true);
        assert!(
            rx.try_recv().unwrap().is_err(),
            "missing capture source must be an error, not a fabricated image"
        );
    }
}
