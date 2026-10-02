use super::*;
use fold_platform::desktop::{DesktopCommand, DesktopState, PreviewResult};
struct Client {
    state: DesktopState,
    requests: Vec<PreviewKey>,
    cancellations: usize,
}
impl DesktopClient for Client {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn poll(&mut self) {}
    fn command(&mut self, _: DesktopCommand) {}
    fn request_preview(&mut self, key: PreviewKey) {
        self.requests.push(key);
    }
    fn cancel_preview(&mut self) {
        self.cancellations += 1;
    }
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
#[test]
fn bounded_serial_adapter_does_not_replace_one_viewer_demand_every_redraw() {
    let key = PreviewKey {
        target: None,
        output: "video".into(),
        content: "a".into(),
        frame: 0,
        dimensions: [64, 64],
        view: 1,
    };
    let other = PreviewKey {
        content: "b".into(),
        ..key.clone()
    };
    let mut host = PreviewHost::new();
    let mut client = Client {
        state: Default::default(),
        requests: vec![],
        cancellations: 0,
    };
    for _ in 0..10 {
        host.select_many(vec![key.clone(), other.clone()], &mut client);
    }
    assert_eq!(client.requests, vec![key.clone()]);
    assert_eq!(client.cancellations, 1);
    // A failed source is disclosed and must not starve the other source.
    host.requested = false;
    host.failures.push((key.clone(), "Missing source".into()));
    host.select_many(vec![key.clone(), other.clone()], &mut client);
    assert_eq!(client.requests, vec![key.clone(), other.clone()]);
    assert!(matches!(host.state_for(Some(&key)), Preview::Failed(_)));
    host.select_many(vec![other.clone()], &mut client);
    assert_eq!(client.requests.len(), 2);
    host.select_many(vec![], &mut client);
    assert!(host.wanted.is_none());
    host.select_many(vec![other], &mut client);
    assert_eq!(client.requests.len(), 3);
    assert_eq!(host.cache.budget, 256 * 1024 * 1024);
}

#[test]
fn serial_adapter_rotates_even_when_the_first_viewer_keeps_advancing() {
    let a = PreviewKey {
        target: None,
        output: "video".into(),
        content: "a".into(),
        frame: 0,
        dimensions: [64, 64],
        view: 1,
    };
    let b = PreviewKey {
        content: "b".into(),
        ..a.clone()
    };
    let mut host = PreviewHost::new();
    let mut client = Client {
        state: Default::default(),
        requests: vec![],
        cancellations: 0,
    };
    host.select_many(vec![a.clone(), a.clone(), b.clone()], &mut client);
    assert_eq!(
        host.consumers.len(),
        2,
        "equivalent demands share one cache/request key"
    );
    host.requested = false; // First request completed; next UI frame has a newer time.
    let next = PreviewKey { frame: 1, ..a };
    host.select_many(vec![next.clone(), b.clone()], &mut client);
    assert_eq!(
        client.requests[1], b,
        "an advancing first viewer must not starve the second"
    );
    host.requested = false;
    host.select_many(vec![next.clone(), b], &mut client);
    assert_eq!(client.requests[2], next);
}
