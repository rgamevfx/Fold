//! Application-owned Dear ImGui presentation. No project mutations or evaluation.
//! Native backend types stay private; headless applications do not load this crate.

mod desktop;
mod preview;
mod shell;

/// Run the single-window desktop shell on the calling (main) thread.
/// Consume one asynchronous display result; the host never evaluates the source.
pub fn run_desktop(
    preview: std::sync::mpsc::Receiver<Result<fold_platform::DisplayFrame, String>>,
) -> Result<(), Box<dyn std::error::Error>> {
    desktop::run(preview)
}
