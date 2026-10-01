//! Application-owned Dear ImGui presentation. No project mutations or evaluation.
//! Native backend types stay private; headless applications do not load this crate.

mod desktop;
mod shell;

/// Run the single-window desktop shell on the calling (main) thread.
pub fn run_desktop() -> Result<(), Box<dyn std::error::Error>> {
    desktop::run()
}
