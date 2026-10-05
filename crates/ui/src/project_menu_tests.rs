use super::*;
use fold_platform::desktop::{DesktopState, PreviewKey, PreviewResult};
#[derive(Default)]
struct Host {
    state: DesktopState,
    commands: Vec<DesktopCommand>,
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        self.commands.push(command);
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
#[test]
fn quit_waits_for_successful_save_of_all_current_changes() {
    let mut host = Host::default();
    let mut menu = ProjectMenu::default();
    host.state.dirty = true;
    menu.request(Action::Quit, &mut host);
    assert!(menu.pending.is_some());
    assert!(!menu.quit);
    // Failed/cancelled saves cannot advance the pending action.
    menu.waiting_save = Some(0);
    menu.poll_save(&mut host);
    assert!(!menu.quit && menu.pending.is_some());
    // A successful older snapshot still leaves unsaved changes.
    menu.waiting_save = Some(0);
    host.state.save_serial = 1;
    menu.poll_save(&mut host);
    assert!(!menu.quit && menu.pending.is_some());
    menu.waiting_save = Some(1);
    host.state.file_busy = true;
    host.state.dirty = false;
    menu.poll_save(&mut host);
    assert!(!menu.quit);
    host.state.file_busy = false;
    host.state.save_serial = 2;
    menu.poll_save(&mut host);
    assert!(menu.quit && menu.pending.is_none());
}
#[test]
fn busy_jobs_and_transient_edits_cannot_be_silently_discarded() {
    let mut host = Host::default();
    let mut menu = ProjectMenu::default();
    host.state.busy = true;
    menu.request(Action::New, &mut host);
    assert!(matches!(
        host.commands.as_slice(),
        [DesktopCommand::Notify(_)]
    ));
    host.commands.clear();
    host.state.busy = false;
    host.state.transient = true;
    menu.request(Action::New, &mut host);
    assert!(host.commands.is_empty() && menu.pending.is_some());
}
