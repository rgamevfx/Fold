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
    fn viewer_request(&self, _: PanelInstanceId) -> Option<(u64, fold_foundation::Time)> {
        Some((u64::from(self.state.frame), fold_foundation::Time::ZERO))
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
fn held_images_survive_waiting_but_retired_completions_and_other_sources_do_not() {
    use fold_foundation::{DocumentId, Time};
    let id = PanelInstanceId(1);
    let doc = DocumentId::new();
    let first = PreviewKey {
        target: Some((doc, Time::ZERO)),
        output: "video".into(),
        content: "a".into(),
        frame: 0,
        dimensions: [16, 16],
        view: 1,
    };
    let next = PreviewKey {
        target: Some((doc, Time::new(1, 24).unwrap())),
        frame: 1,
        ..first.clone()
    };
    let mut host = PreviewHost::new();
    let mut client = Client {
        state: Default::default(),
        requests: vec![],
        cancellations: 0,
    };
    host.select_viewers(&[(id, Some(first.clone()))], &mut client);
    host.held.insert(id, first.clone()); // Simulate an already presented GPU image.
    host.select_viewers(&[(id, Some(next.clone()))], &mut client);
    assert_eq!(host.presented_key(id), Some(&first));
    host.completed_frame(&first);
    assert_eq!(
        host.completed.get(&id),
        Some(&first),
        "late completion is offered to the playback policy"
    );
    client.state.frame = 1; // Explicit seek/mode/edit generation, not clock progression.
    host.select_viewers(&[(id, Some(next.clone()))], &mut client);
    host.completed_frame(&first);
    assert!(!host.completed.contains_key(&id));
    assert_eq!(
        host.presented_key(id),
        Some(&first),
        "seek holds the picture but cannot publish the retired request"
    );
    host.failures.push((next.clone(), "Decode failed".into()));
    assert_eq!(host.viewer_error(id), Some("Decode failed"));
    assert_eq!(host.presented_key(id), Some(&first));
    let other = PreviewKey {
        target: Some((DocumentId::new(), Time::ZERO)),
        ..next
    };
    host.select_viewers(&[(id, Some(other))], &mut client);
    assert!(
        host.presented_key(id).is_none(),
        "never show the previous source as the new one"
    );
    host.select_viewers(&[], &mut client);
    assert!(host.held.is_empty());
    assert!(host.completed.is_empty());
}

#[test]
fn seeking_and_closing_consumers_do_not_cancel_other_demands_or_publish_obsolete_results() {
    use fold_platform::workspace::PanelInstanceId;
    let a = PanelInstanceId(1);
    let b = PanelInstanceId(2);
    let key = PreviewKey {
        target: None,
        output: "video".into(),
        content: "same".into(),
        frame: 0,
        dimensions: [64, 64],
        view: 1,
    };
    let next = PreviewKey {
        frame: 12,
        ..key.clone()
    };
    let mut host = PreviewHost::new();
    let mut client = Client {
        state: Default::default(),
        requests: vec![],
        cancellations: 0,
    };
    host.select_viewers(
        &[(a, Some(key.clone())), (b, Some(key.clone()))],
        &mut client,
    );
    host.select_viewers(
        &[(a, Some(next.clone())), (b, Some(key.clone()))],
        &mut client,
    );
    assert_eq!(client.requests.len(), 1);
    assert_eq!(client.cancellations, 1);
    assert_eq!(host.demands[&b], key);
    host.select_viewers(&[(a, Some(next.clone()))], &mut client);
    assert_eq!(
        client.cancellations, 1,
        "finish shared in-flight content rather than canceling on consumer churn"
    );
    host.requested = false;
    host.failures.push((key, "Obsolete result".into()));
    host.select_viewers(&[(a, Some(next.clone()))], &mut client);
    assert!(matches!(host.state_for_viewer(a), Preview::Pending));
    assert_eq!(client.requests.last(), Some(&next));
    assert!(!host.demands.contains_key(&b));
    host.select_viewers(&[], &mut client);
    assert!(host.wanted.is_none());
    assert!(host.pending.is_none());
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
