//! Delivery owns export form state; Session remains the job and project authority.
use super::*;
pub(super) struct Delivery {
    export_path: String,
    start: i32,
    end: i32,
    pub(super) output: Option<(DocumentId, u32)>,
}
impl Default for Delivery {
    fn default() -> Self {
        Self {
            export_path: "/tmp/fold-export.mp4".into(),
            start: 0,
            end: 1,
            output: None,
        }
    }
}
impl Delivery {
    pub(super) fn refresh(&mut self, client: &dyn DesktopClient) {
        let output = client
            .output_info()
            .ok()
            .map(|(id, info)| (id, info.frames));
        if self.output != output {
            self.start = 0;
            self.end = output.map_or(0, |(_, frames)| frames as i32);
            self.output = output;
        }
    }
    pub(super) fn draw(
        &mut self,
        ui: &Ui,
        key: &WindowKey,
        viewed: Option<workspace::DocumentRef>,
        labels: &BTreeMap<DocumentId, String>,
        client: &mut dyn DesktopClient,
    ) {
        let state = client.state().clone();
        ui.window(key).build(|| {
            if let Some((document, frames)) = self.output {
                ui.text_wrapped(format!(
                    "Delivery: {} · {frames} frames",
                    labels
                        .get(&document)
                        .map(String::as_str)
                        .unwrap_or("Missing output")
                ));
            } else {
                ui.text_disabled("No valid delivery output selected");
            }
            let viewed = viewed;
            if let Some(output) = viewed
                && output.output == "video"
                && ui.button("Use first viewer as delivery output")
            {
                client.command(DesktopCommand::SetOutput(output.document));
            }
            if let (Some(snapshot), Some((document, _))) = (client.snapshot(), self.output)
                && fold_platform::color::project(&snapshot)
                    .ok()
                    .flatten()
                    .is_some()
            {
                match fold_platform::color::output(&snapshot, document) {
                    Ok(mut transform) => {
                        if crate::sdk::color_controls::output(
                            ui,
                            &state.color_choices,
                            &mut transform,
                        ) {
                            client.command(DesktopCommand::SetOutputColor {
                                document,
                                transform,
                            });
                        }
                    }
                    Err(error) => ui.text_wrapped(error),
                }
            }
            let mut prores = std::path::Path::new(&self.export_path)
                .extension()
                .is_some_and(|e| e.eq_ignore_ascii_case("mov"));
            if let Some(_combo) = ui.begin_combo(
                "Format",
                if prores {
                    "ProRes 4444 • Alpha (.mov)"
                } else {
                    "H.264 (.mp4)"
                },
            ) {
                for (value, label) in [
                    (false, "H.264 (.mp4)"),
                    (true, "ProRes 4444 • Alpha (.mov)"),
                ] {
                    if ui
                        .selectable_config(label)
                        .selected(value == prores)
                        .build()
                    {
                        prores = value;
                        self.export_path = std::path::Path::new(&self.export_path)
                            .with_extension(if prores { "mov" } else { "mp4" })
                            .to_string_lossy()
                            .into_owned();
                    }
                }
            }
            ui.input_text("Output file", &mut self.export_path).build();
            ui.input_int("Start frame", &mut self.start);
            ui.input_int("End (exclusive)", &mut self.end);
            if ui.button("Export range") && self.start >= 0 && self.end > self.start {
                client.command(DesktopCommand::Export {
                    path: self.export_path.clone().into(),
                    start: self.start as u32,
                    end: self.end as u32,
                });
            }
            ui.same_line();
            if ui.button("Cancel job") {
                client.command(DesktopCommand::Cancel);
            }
            ui.text_wrapped(&state.status);
        });
    }
}
