//! Shared Dear ImGui presentation. Project mutations and jobs belong to the host.
mod cache;
mod desktop;
mod group_selector;
#[cfg(feature = "native-probe")]
mod native_probe;
#[cfg(feature = "native-probe")]
mod native_shared_probe;
mod preview;
mod review;
pub mod sdk;
mod shell;
mod target_selector;
mod transport;
mod viewer_source;
mod workspace_store;

// Dear ImGui permits only one owning thread at a time, including headless tests.
#[cfg(test)]
static IMGUI_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn run_desktop(
    client: Box<dyn fold_platform::desktop::DesktopClient>,
    panels: sdk::PanelRegistry,
) -> Result<(), Box<dyn std::error::Error>> {
    desktop::run(client, panels.finish()?)
}
