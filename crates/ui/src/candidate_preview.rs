//! A separate candidate viewer keeps the artist's playback and selection intact.
use crate::{
    sdk::imgui::{self, Ui},
    shell::Preview,
};
use fold_platform::desktop::DesktopClient;
pub(crate) fn draw(ui: &Ui, preview: &Preview, client: &mut dyn DesktopClient) {
    let Some(p) = client.agent_preview() else {
        return;
    };
    ui.window("Agent preview")
        .size([680., 460.], imgui::Condition::FirstUseEver)
        .build(|| {
            let mut action = None;
            {
                let _disabled = ui.begin_disabled_with_cond(p.conflict);
                if ui.button("Apply") {
                    action = Some(("apply", None));
                }
            }
            ui.same_line();
            if ui.button("Discard") {
                action = Some(("discard", None));
            }
            ui.same_line();
            {
                let _disabled = ui.begin_disabled_with_cond(!p.can_compare);
                let mut original = p.original;
                if ui.checkbox("Original", &mut original) {
                    action = Some(("compare", None));
                }
            }
            if p.conflict {
                ui.text_wrapped(
                    "Project changed or an edit is active. Rebuild the candidate before applying.",
                );
            }
            let mut frame = p.key.frame as i32;
            ui.set_next_item_width(ui.content_region_avail()[0].max(40.));
            if ui.slider(
                "##preview-frame",
                0,
                p.frames.saturating_sub(1) as i32,
                &mut frame,
            ) {
                action = Some(("frame", Some(frame as u32)));
            }
            ui.text_disabled(format!(
                "{} · version {} · frame {}",
                if p.original { "Original" } else { "Candidate" },
                p.generation,
                p.key.frame
            ));
            match preview {
                Preview::Ready {
                    texture,
                    dimensions,
                    uv_max,
                } => {
                    let available = ui.content_region_avail();
                    let scale = (available[0] / dimensions[0] as f32)
                        .min(available[1] / dimensions[1] as f32)
                        .max(0.);
                    imgui::Image::new(
                        ui,
                        *texture,
                        [dimensions[0] as f32 * scale, dimensions[1] as f32 * scale],
                    )
                    .uv1(*uv_max)
                    .build();
                }
                Preview::Pending => ui.text_disabled("Rendering preview…"),
                Preview::Failed(error) => ui.text_wrapped(error),
            }
            if let Some((action, frame)) = action
                && let Err(error) = client.agent_preview_action(action, frame)
            {
                ui.text_wrapped(error);
            }
        });
}
