//! Shared Dear ImGui presentation. Project mutations and jobs belong to the host.
mod cache;
mod desktop;
mod preview;
mod shell;

pub fn run_desktop(
    client: Box<dyn fold_platform::desktop::DesktopClient>,
) -> Result<(), Box<dyn std::error::Error>> {
    desktop::run(client)
}
