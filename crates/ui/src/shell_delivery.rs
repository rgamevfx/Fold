//! Delivery owns export form state; Session remains the job and project authority.
use super::*;
pub(super) struct Delivery {
    settings: fold_platform::delivery::Settings,
    start: i32,
    end: i32,
    pub(super) output: Option<(DocumentId, u32)>,
}
impl Default for Delivery {
    fn default() -> Self {
        Self {
            settings: fold_platform::delivery::Settings {
                destination: "/tmp/fold-export.mp4".into(),
                file_type: fold_platform::delivery::FileType::Mp4,
                ..Default::default()
            },
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
            let mut response = crate::sdk::EditResponse::default();
            crate::sdk::delivery_controls::file_type(ui, &mut self.settings, true, &mut response);
            crate::sdk::delivery_controls::movie_options(ui, &mut self.settings, &mut response);
            ui.input_text("Output file", &mut self.settings.destination)
                .build();
            ui.input_int("Start frame", &mut self.start);
            ui.input_int("End (exclusive)", &mut self.end);
            if ui.button("Export range") && self.start >= 0 && self.end > self.start {
                client.command(DesktopCommand::Export {
                    encoding: self
                        .settings
                        .movie()
                        .expect("Delivery selects a movie format"),
                    path: self.settings.destination.clone().into(),
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
