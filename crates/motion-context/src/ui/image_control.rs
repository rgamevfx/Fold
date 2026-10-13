//! Project image selection shared by native images and exposed script controls.
use fold_foundation::AssetId;
use fold_platform::{browser, desktop::DesktopClient};
use fold_project::ItemId;
use fold_ui::sdk::{EditResponse, imgui::Ui};

pub(super) fn draw(
    ui: &Ui,
    host: &dyn DesktopClient,
    label: &str,
    value: &mut AssetId,
    response: &mut EditResponse,
) {
    let Some(snapshot) = host.snapshot() else {
        return;
    };
    let name = snapshot
        .state()
        .organization
        .items
        .iter()
        .find(|item| item.id == ItemId::Asset(*value))
        .map_or("Missing image", |item| item.name.as_str());
    if let Some(_combo) = ui.begin_combo(label, name) {
        for item in &snapshot.state().organization.items {
            let ItemId::Asset(id) = item.id else {
                continue;
            };
            let valid = browser::asset_metadata(&snapshot, id)
                .and_then(|metadata| crate::image_source::dimensions(&metadata.profile))
                .is_ok()
                && snapshot
                    .state()
                    .assets
                    .get(&id)
                    .is_some_and(|asset| crate::image_source::interpretation(asset).is_ok());
            if !valid {
                continue;
            }
            let _id = ui.push_id(&id.to_string());
            if ui
                .selectable_config(&item.name)
                .selected(id == *value)
                .build()
                && id != *value
            {
                *value = id;
                response.changed = true;
                response.finished = true;
            }
        }
    }
}

pub(super) fn first(host: &dyn DesktopClient) -> Option<AssetId> {
    let snapshot = host.snapshot()?;
    snapshot.state().organization.items.iter().find_map(|item| {
        let ItemId::Asset(id) = item.id else {
            return None;
        };
        browser::asset_metadata(&snapshot, id)
            .ok()
            .filter(|m| crate::image_source::dimensions(&m.profile).is_ok())?;
        crate::image_source::interpretation(snapshot.state().assets.get(&id)?).ok()?;
        Some(id)
    })
}
