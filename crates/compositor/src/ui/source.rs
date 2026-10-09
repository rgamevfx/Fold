//! Source drafts never replace the last valid graph until a guarded Apply.
use super::state::State;
use fold_foundation::DocumentId;
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::imgui::Ui;
use std::collections::BTreeMap;
#[derive(Default)]
pub(super) struct Sources(BTreeMap<DocumentId, Draft>);
struct Draft {
    expected: String,
    text: String,
    error: String,
}
impl Sources {
    pub fn draw(&mut self, ui: &Ui, state: &mut State, host: &mut dyn DesktopClient) {
        state.sync(host);
        let (Some(document), Some(graph)) = (state.document, state.graph.as_ref()) else {
            ui.text_wrapped("Select a Compositing layer.");
            return;
        };
        let current = match graph.source(None) {
            Ok(source) => source,
            Err(error) => {
                ui.text_wrapped(error);
                return;
            }
        };
        let draft = self.0.entry(document).or_insert_with(|| Draft {
            expected: current.clone(),
            text: current.clone(),
            error: String::new(),
        });
        if draft.text == draft.expected && draft.expected != current {
            draft.expected = current.clone();
            draft.text = current.clone();
        }
        {
            let _disabled =
                ui.begin_disabled_with_cond(draft.text == draft.expected || state.editing);
            if ui.button("Apply") {
                let mut next = graph.clone();
                match next.replace_source(None, &draft.expected, &draft.text) {
                    Ok(()) => {
                        state.graph = Some(next);
                        state.commit(host);
                        if state.error.is_empty() {
                            draft.expected = state
                                .graph
                                .as_ref()
                                .and_then(|g| g.source(None).ok())
                                .unwrap_or(current.clone());
                            draft.text = draft.expected.clone();
                            draft.error.clear();
                        } else {
                            draft.error = state.error.clone();
                        }
                    }
                    Err(error) => draft.error = error,
                }
            }
        }
        ui.same_line();
        if ui.button("Reload") {
            draft.expected = current.clone();
            draft.text = current;
            draft.error.clear();
        }
        if ui.is_item_hovered() {
            ui.tooltip_text("Discard this draft and reload the current graph");
        }
        if !draft.error.is_empty() {
            ui.text_wrapped(&draft.error);
        }
        let size = ui.content_region_avail();
        ui.input_text_multiline(
            "##compositor-source",
            &mut draft.text,
            [size[0].max(1.), size[1].max(ui.frame_height())],
        )
        .build();
    }
}
