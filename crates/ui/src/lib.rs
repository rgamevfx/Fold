//! Shared Dear ImGui presentation. Project mutations and jobs belong to the host.
mod cache;
mod desktop;
#[cfg(feature = "native-probe")]
mod native_probe;
mod preview;
pub mod sdk;
mod shell;
mod transport;

// Dear ImGui permits only one owning thread at a time, including headless tests.
#[cfg(test)]
static IMGUI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn run_desktop(
    client: Box<dyn fold_platform::desktop::DesktopClient>,
    panels: sdk::PanelRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    desktop::run(client, panels.finish()?)
}
