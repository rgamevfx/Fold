use super::*;
use fold_platform::desktop::{DesktopCommand, DesktopState, PreviewResult};
#[derive(Default)]
struct Client {
    state: DesktopState,
    batches: Vec<Vec<PreviewDemand>>,
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
    fn request_preview(&mut self, _: PreviewKey) {
        panic!("production host must submit consumer demand");
    }
    fn preview_demands(&mut self, demands: Vec<PreviewDemand>) {
        self.batches.push(demands);
    }
    fn cancel_preview(&mut self) {
        panic!("one viewer must not globally cancel another");
    }
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
fn key() -> PreviewKey {
    PreviewKey {
        target: Some((
            fold_foundation::DocumentId::new(),
            fold_foundation::Time::ZERO,
        )),
        output: "video".into(),
        content: "scene".into(),
        frame: 0,
        dimensions: [16, 16],
        view: 1,
    }
}
#[test]
fn per_viewer_demand_is_bounded_and_unchanged_redraws_do_not_resubmit() {
    let mut host = PreviewHost::new();
    let mut client = Client::default();
    let a = PanelInstanceId(1);
    let b = PanelInstanceId(2);
    let first = key();
    for _ in 0..100 {
        host.select_viewers(
            &[(a, Some(first.clone())), (b, Some(first.clone()))],
            &mut client,
        );
    }
    assert_eq!(client.batches.len(), 1);
    assert_eq!(client.batches[0].len(), 2);
    assert_eq!(
        host.consumers.len(),
        1,
        "shared presentation protects one content key"
    );
    let next = PreviewKey {
        frame: 12,
        ..first.clone()
    };
    host.select_viewers(
        &[(a, Some(next.clone())), (b, Some(first.clone()))],
        &mut client,
    );
    assert_eq!(client.batches.last().unwrap()[1].key, first);
    host.select_viewers(&[(a, Some(next))], &mut client);
    assert_eq!(client.batches.last().unwrap().len(), 1);
    host.select_viewers(&[], &mut client);
    assert!(client.batches.last().unwrap().is_empty());
}
#[test]
fn held_images_survive_waiting_but_retired_completions_and_other_sources_do_not() {
    let id = PanelInstanceId(1);
    let first = key();
    let next = PreviewKey {
        frame: 1,
        ..first.clone()
    };
    let mut host = PreviewHost::new();
    let mut client = Client::default();
    host.select_viewers(&[(id, Some(first.clone()))], &mut client);
    host.held.insert(id, first.clone());
    host.admitted.insert(id, 0); // Worker captured this generation when it admitted shared work.
    host.select_viewers(&[(id, Some(next.clone()))], &mut client);
    host.completed_frame(&first);
    assert_eq!(host.completed.get(&id), Some(&first));
    client.state.frame = 1;
    host.select_viewers(&[(id, Some(next.clone()))], &mut client);
    host.completed_frame(&first);
    assert!(!host.completed.contains_key(&id));
    assert_eq!(host.presented_key(id), Some(&first));
    host.failures.push((next.clone(), "Decode failed".into()));
    assert_eq!(host.viewer_error(id), Some("Decode failed"));
    let other = PreviewKey {
        target: Some((
            fold_foundation::DocumentId::new(),
            fold_foundation::Time::ZERO,
        )),
        ..next
    };
    host.select_viewers(&[(id, Some(other))], &mut client);
    assert!(host.presented_key(id).is_none());
    host.select_viewers(&[], &mut client);
    assert!(host.held.is_empty());
    assert!(host.completed.is_empty());
}
