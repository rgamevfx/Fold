//! Compact artist controls over the existing cancellable worker-command path.
use fold_platform::desktop::{DesktopClient, DesktopCommand};
use fold_ui::sdk::{EditResponse, imgui::Ui};
pub fn draw(
    ui: &Ui,
    host: &mut dyn DesktopClient,
    node: &mut crate::graph::Node,
    document: Option<fold_foundation::DocumentId>,
    group: Option<fold_foundation::ObjectId>,
    response: &mut EditResponse,
) -> bool {
    if node.kind != "fold.motion.image_bake" {
        return false;
    }
    use crate::nodes::image::bake::{Args, Settings};
    let Ok(mut settings) = node.settings::<Settings>() else {
        ui.text_wrapped("Invalid bake settings");
        return true;
    };
    let mut changed = false;
    let mut resolution = settings.resolution as i32;
    ui.set_next_item_width((ui.content_region_avail()[0] - 110.).max(60.));
    if ui.input_int("Resolution", &mut resolution) {
        settings.resolution = (resolution.clamp(16, 4096) as u32).div_ceil(4) * 4;
        changed = true;
    }
    if ui.is_item_hovered() {
        ui.tooltip_text(
            "Longest texture edge in pixels; mipmaps are generated for smaller scales.",
        );
    }
    for (label, value) in [
        ("Start frame", &mut settings.start),
        ("End frame", &mut settings.end),
    ] {
        let mut v = *value as i32;
        ui.set_next_item_width((ui.content_region_avail()[0] - 110.).max(60.));
        if ui.input_int(label, &mut v) {
            *value = v.max(0) as u32;
            changed = true;
        }
        if label == "End frame" && ui.is_item_hovered() {
            ui.tooltip_text("Exclusive end. Zero uses the document end.");
        }
    }
    use fold_render::baked::Format;
    let label = match settings.format {
        Format::Auto => "Auto",
        Format::Bc7 => "Compact (SDR)",
        Format::Float16 => "Float (HDR)",
    };
    if let Some(_combo) = ui.begin_combo("Quality", label) {
        for (label, format) in [
            ("Auto", Format::Auto),
            ("Compact (SDR)", Format::Bc7),
            ("Float (HDR)", Format::Float16),
        ] {
            if ui.selectable(label) {
                settings.format = format;
                changed = true;
            }
        }
    }
    if settings.format == Format::Bc7 {
        if ui.checkbox("Clamp to SDR", &mut settings.clamp_sdr) {
            changed = true;
        }
        if settings.clamp_sdr {
            ui.text_wrapped("Clips negative and above-white RGB values.");
        }
    }
    if changed {
        node.settings["resolution"] = settings.resolution.into();
        node.settings["start"] = settings.start.into();
        node.settings["end"] = settings.end.into();
        node.settings["format"] = serde_json::to_value(settings.format).unwrap();
        node.settings["clamp_sdr"] = settings.clamp_sdr.into();
        response.changed = true;
        response.finished = true;
    }
    let Some(document) = document else {
        return true;
    };
    let args = Args {
        document,
        node: node.id,
        group,
    };
    let progress = fold_platform::image_bake::progress(&args.progress_key());
    if progress.as_ref().is_some_and(|p| p.running) {
        let p = progress.as_ref().unwrap();
        ui.text(format!("Baking textures… {} / {}", p.completed, p.total));
        if ui.button("Cancel bake") {
            fold_platform::image_bake::cancel(&args.progress_key());
        }
    } else {
        if ui.button(if settings.bake.is_some() {
            "Rebuild"
        } else {
            "Bake"
        }) && !changed
        {
            if let Some(snapshot) = host.snapshot() {
                host.command(DesktopCommand::Extension(
                    fold_platform::packages::CommandRequest {
                        id: fold_platform::image_bake::COMMAND.into(),
                        base: snapshot.revision(),
                        arguments: serde_json::to_vec(&args).unwrap(),
                    },
                ));
            }
        }
        if settings.bake.is_some() {
            ui.same_line();
            if ui.button("Clear") {
                node.settings["bake"] = serde_json::Value::Null;
                settings.bake = None;
                response.changed = true;
                response.finished = true;
            }
        }
        if let Some(error) = progress.and_then(|p| p.error) {
            ui.text_wrapped(error);
        }
    }
    if let Some(bake) = &settings.bake {
        if bake.options != settings.options() {
            ui.text_wrapped("Settings changed — rebuild the bake.");
        } else {
            ui.text_disabled("Baked textures enabled");
        }
    } else {
        ui.text_disabled("Using original input");
    }
    if settings.format != Format::Float16 {
        ui.text_wrapped(
            "Compact textures use lossy compression. Auto preserves HDR using float textures.",
        );
    }
    true
}
