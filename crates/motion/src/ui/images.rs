//! Image pickers use project metadata only. Decoding stays on workers.
use crate::{
    binding::Binding,
    model::Node,
    types::{AssetReference, Literal},
};
use fold_foundation::ObjectId;
use fold_platform::{desktop::DesktopClient, image_source::Source};
use fold_ui::sdk::{EditResponse, imgui::Ui};
use std::collections::BTreeSet;

pub(super) fn sources(
    host: &dyn DesktopClient,
    current: Option<fold_foundation::DocumentId>,
) -> Vec<(Source, String)> {
    let Some(snapshot) = host.snapshot() else {
        return vec![];
    };
    let mut result = vec![];
    for asset in snapshot.state().assets.values() {
        if fold_platform::browser::asset_metadata(&snapshot, asset.id).is_ok_and(|m| {
            matches!(
                m.profile,
                fold_media::ingest::SourceProfile::Ppm { .. }
                    | fold_media::ingest::SourceProfile::Video(_)
                    | fold_media::ingest::SourceProfile::Exr(_)
            )
        }) {
            let fallback = std::path::Path::new(&asset.location)
                .file_name()
                .and_then(|v| v.to_str())
                .unwrap_or("Image");
            let label = fold_platform::browser::item_name(
                &snapshot,
                fold_project::ItemId::Asset(asset.id),
                fallback,
            );
            result.push((Source::Asset(asset.id), label));
        }
    }
    for document in snapshot.state().documents.values() {
        if Some(document.id) == current {
            continue;
        }
        let name = fold_platform::browser::item_name(
            &snapshot,
            fold_project::ItemId::Document(document.id),
            "Document",
        );
        for output in host.outputs(document.id) {
            if output.info.is_ok() {
                result.push((
                    Source::Output(output.reference),
                    format!("{name} / {}", output.label),
                ));
            }
        }
    }
    result
}
pub(super) fn bindings(source: &Source) -> std::collections::BTreeMap<String, Binding> {
    let lit = |value| Binding::Literal { value };
    match source {
        Source::Asset(id) => [(
            "asset".into(),
            lit(Literal::AssetRef {
                value: AssetReference::Resource(*id),
            }),
        )]
        .into(),
        Source::Output(r) => [
            (
                "document".into(),
                lit(Literal::Text {
                    value: r.document.to_string(),
                }),
            ),
            (
                "output".into(),
                lit(Literal::Text {
                    value: r.output.clone(),
                }),
            ),
        ]
        .into(),
    }
}
pub(super) fn source_picker(
    ui: &Ui,
    node: &mut Node,
    host: &dyn DesktopClient,
    document: fold_foundation::DocumentId,
    locked: &BTreeSet<ObjectId>,
    response: &mut EditResponse,
) {
    if node.operation != "fold.motion.image_texture" {
        return;
    }
    let _id = ui.push_id(&node.id.to_string());
    let options = sources(host, Some(document));
    let current = crate::images::source(node).ok();
    let label = options
        .iter()
        .find(|(s, _)| Some(s) == current.as_ref())
        .map_or("Missing image", |(_, l)| l.as_str());
    ui.text("Source");
    let disabled = node.parameters.iter().any(|(name, p)| {
        matches!(name.as_str(), "asset" | "document" | "output") && locked.contains(&p.id)
    });
    let _disabled = ui.begin_disabled_with_cond(disabled);
    let _width = ui.push_item_width(ui.content_region_avail()[0].max(1.));
    if let Some(_combo) = ui.begin_combo("##image-source", label) {
        for (source, label) in options {
            // Source kind belongs to the versioned mode; replacing within a kind
            // preserves all parameter IDs, links, keys and protection.
            if matches!(&source, Source::Asset(_)) != (node.mode == "asset") {
                continue;
            }
            let _id = ui.push_id(&serde_json::to_string(&source).unwrap());
            if ui
                .selectable_config(&label)
                .selected(Some(&source) == current.as_ref())
                .build()
                && Some(&source) != current.as_ref()
            {
                for (name, binding) in bindings(&source) {
                    node.parameters.get_mut(&name).unwrap().value =
                        serde_json::to_value(binding).unwrap();
                }
                response.began = true;
                response.changed = true;
                response.finished = true;
            }
        }
    }
}
pub(super) fn choice(
    ui: &Ui,
    operation: &str,
    name: &str,
    label: &str,
    value: &mut Literal,
    response: &mut EditResponse,
) -> bool {
    if !matches!(
        operation,
        "fold.motion.image_texture" | "fold.motion.sample_image"
    ) {
        return false;
    }
    if name == "color_space" {
        let Literal::Text { value } = value else {
            return false;
        };
        ui.text(label);
        let _width = ui.push_item_width(ui.content_region_avail()[0].max(1.));
        let changed = ui
            .input_text("##color-space", value)
            .hint("From source")
            .build();
        response.item(ui, changed);
        return true;
    }
    let choices: &[(&str, &str)] = match name {
        "outside" => &[
            ("hold", "Hold"),
            ("loop", "Loop"),
            ("transparent", "Transparent"),
            ("error", "Error"),
        ],
        "filter" => &[("nearest", "Nearest"), ("bilinear", "Bilinear")],
        "wrap" => &[
            ("clamp", "Clamp"),
            ("repeat", "Repeat"),
            ("mirror", "Mirror"),
            ("transparent", "Transparent"),
        ],
        "channel" => &[
            ("luminance", "Luminance"),
            ("red", "Red"),
            ("green", "Green"),
            ("blue", "Blue"),
            ("alpha", "Alpha"),
        ],
        _ => return false,
    };
    let Literal::Text { value } = value else {
        return false;
    };
    ui.text(label);
    let _width = ui.push_item_width(ui.content_region_avail()[0].max(1.));
    let title = choices
        .iter()
        .find(|(v, _)| *v == value)
        .map_or(value.as_str(), |(_, label)| label);
    if let Some(_combo) = ui.begin_combo("##choice", title) {
        for (key, label) in choices {
            let clicked = ui.selectable_config(label).selected(value == key).build();
            response.item(ui, clicked && value != key);
            if clicked {
                *value = (*key).into();
            }
        }
    }
    true
}
