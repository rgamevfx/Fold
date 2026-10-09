//! Bounded local transport coordination. Dispatch receives the owner's project
//! only for a synchronous call; workers never retain a writer.
use fold_project::Project;
use std::time::Instant;
#[derive(Default)]
pub(super) struct Agent {
    dispatcher: crate::agent::dispatch::Dispatcher,
    #[cfg(target_os = "linux")]
    endpoint: Option<crate::agent::local::Endpoint>,
}
pub(super) struct Outcome {
    pub response: Vec<u8>,
    pub changed: bool,
}
#[cfg(target_os = "linux")]
pub(super) enum Incoming {
    Idle,
    Deferred,
    Request(crate::agent::local::Request),
}
impl Agent {
    pub fn replace_project(&mut self) {
        self.dispatcher = Default::default();
    }
    #[cfg(target_os = "linux")]
    pub fn enable(&mut self, parent: &std::path::Path) -> std::io::Result<std::path::PathBuf> {
        if self.endpoint.is_none() {
            self.endpoint = Some(crate::agent::local::Endpoint::bind(parent)?);
        }
        Ok(self
            .endpoint
            .as_ref()
            .expect("enabled endpoint")
            .path()
            .to_owned())
    }
    pub fn dispatch(
        &mut self,
        project: &mut Project,
        bytes: &[u8],
        editing: bool,
        #[cfg(feature = "gpu")] host: Option<fold_render::gpu::Host>,
    ) -> Outcome {
        #[cfg(feature = "gpu")]
        self.dispatcher.set_render_host(host);
        let before = project.snapshot().revision();
        let response =
            self.dispatcher
                .dispatch_with_edit_state(project, bytes, Instant::now(), editing);
        Outcome {
            response,
            changed: project.snapshot().revision() != before,
        }
    }
    pub fn poll(&mut self) {
        self.dispatcher.poll(Instant::now());
        #[cfg(target_os = "linux")]
        if let Some(endpoint) = &mut self.endpoint {
            endpoint.poll_waiters(&mut self.dispatcher, Instant::now());
        }
    }
    #[cfg(target_os = "linux")]
    pub fn next(&mut self) -> Incoming {
        let Some(endpoint) = self.endpoint.as_mut() else {
            return Incoming::Idle;
        };
        let Some(request) = endpoint.next() else {
            return Incoming::Idle;
        };
        match endpoint.defer(request, &mut self.dispatcher, Instant::now()) {
            Some(request) => Incoming::Request(request),
            None => Incoming::Deferred,
        }
    }
}
