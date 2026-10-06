//! Inline source is a draft until background compilation succeeds. Parameters
//! remain ordinary inspector controls with ordinary transaction semantics.
use crate::{
    graph::Node,
    wrangle::{Settings, compiler, sync_controls},
};
use fold_ui::sdk::{EditResponse, imgui::Ui};
use std::sync::mpsc::{self, Receiver};
pub type Drafts = std::collections::BTreeMap<
    (Option<fold_foundation::ObjectId>, fold_foundation::ObjectId),
    Draft,
>;
pub struct Draft {
    source: String,
    base_source: String,
    error: String,
    job: Option<Job>,
}
struct Job {
    rx: Receiver<Result<Node, String>>,
    base: Node,
    cancel: fold_media::Cancel,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub fn draw(
    ui: &Ui,
    node: &mut Node,
    group: Option<fold_foundation::ObjectId>,
    drafts: &mut Drafts,
    response: &mut EditResponse,
) {
    if node.kind != "fold.motion.wrangle" {
        return;
    }
    let Ok(settings) = node.settings::<Settings>() else {
        return;
    };
    let draft = drafts.entry((group, node.id)).or_insert_with(|| Draft {
        source: settings.source.clone(),
        base_source: settings.source.clone(),
        error: String::new(),
        job: None,
    });
    if draft.base_source != settings.source && draft.source == draft.base_source {
        draft.source = settings.source.clone();
        draft.base_source = settings.source.clone();
    }
    ui.input_text_multiline(
        "##wrangle-source",
        &mut draft.source,
        [ui.content_region_avail()[0], 180.],
    )
    .build();
    if draft.source != node.settings["source"].as_str().unwrap_or_default() {
        ui.text_disabled("Source edits pending Apply");
    }
    if ui.button("Create Controls") {
        let mut candidate = node.clone();
        candidate.settings["source"] = draft.source.clone().into();
        match sync_controls(&mut candidate) {
            Ok(()) => {
                candidate.settings["source"] = settings.source.clone().into();
                *node = candidate;
                response.changed = true;
                response.finished = true;
                draft.error.clear();
            }
            Err(e) => draft.error = e,
        }
    }
    ui.same_line();
    if draft.job.is_some() {
        if ui.button("Cancel Compile") {
            draft.job = None;
        }
    } else if ui.button("Apply") {
        let mut candidate = node.clone();
        candidate.settings["source"] = draft.source.clone().into();
        let base = node.clone();
        let (tx, rx) = mpsc::channel();
        let cancel = fold_media::Cancel::default();
        let token = cancel.clone();
        std::thread::spawn(move || {
            let result = (|| {
                sync_controls(&mut candidate)?;
                let s = candidate.settings::<Settings>()?;
                compiler::compile(&s.source, &s.channels, &token)?;
                Ok(candidate)
            })();
            let _ = tx.send(result);
        });
        draft.job = Some(Job { rx, base, cancel });
        draft.error.clear();
    }
    if let Some(job) = &draft.job {
        match job.rx.try_recv() {
            Ok(result) => {
                match result {
                    Ok(candidate) if *node == job.base => {
                        draft.base_source = candidate.settings["source"].as_str().unwrap_or_default().to_owned();
                        *node = candidate;
                        response.changed = true;
                        response.finished = true;
                    }
                    Ok(_)=>draft.error="Node changed during compilation. Apply again to preserve the latest edits.".into(),
                    Err(e)=>draft.error=e,
                }
                draft.job = None;
            }
            Err(mpsc::TryRecvError::Disconnected) => {
                draft.error = "Compiler worker stopped".into();
                draft.job = None;
            }
            Err(mpsc::TryRecvError::Empty) => {
                ui.text("Compiling Rust…");
            }
        }
    }
    if let Some(_combo) = ui.begin_combo(
        "Run Over",
        node.settings["domain"].as_str().unwrap_or("Points"),
    ) {
        for domain in ["Points", "Instances", "Glyphs", "Children"] {
            if ui.selectable(domain) {
                node.settings["domain"] = domain.into();
                node.settings["geometry_kind"] = if domain == "Points" {
                    "Points"
                } else {
                    "Content"
                }
                .into();
                response.changed = true;
                response.finished = true;
            }
        }
    }
    if !draft.error.is_empty() {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&draft.error) {
            ui.text_wrapped(
                value["message"]
                    .as_str()
                    .unwrap_or("Rust compilation failed"),
            );
            for diagnostic in value["diagnostics"].as_array().into_iter().flatten() {
                let line = diagnostic["spans"][0]["line"].as_u64().unwrap_or(0);
                ui.text_wrapped(format!(
                    "Line {line}: {}",
                    diagnostic["message"].as_str().unwrap_or("Compiler error")
                ));
            }
            if let Some(detail) = value["detail"].as_str().filter(|s| !s.is_empty()) {
                ui.text_wrapped(detail);
            }
        } else {
            ui.text_wrapped(&draft.error);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn applying_a_completed_job_keeps_newer_drafts_and_group_editors_separate() {
        let _guard = crate::ui::IMGUI_TEST_LOCK.lock().unwrap();
        let mut context = fold_ui::sdk::imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([600., 500.]);
        context.io_mut().set_delta_time(1. / 60.);
        let mut node = Node::new("fold.motion.wrangle").unwrap();
        let mut candidate = node.clone();
        candidate.settings["source"] = "position[0] += 2.0;".into();
        let (tx, rx) = mpsc::channel();
        tx.send(Ok(candidate.clone())).unwrap();
        let key = (None, node.id);
        let mut drafts = Drafts::new();
        drafts.insert(
            key,
            Draft {
                source: "position[0] += 3.0;".into(),
                base_source: node.settings["source"].as_str().unwrap().into(),
                error: String::new(),
                job: Some(Job {
                    rx,
                    base: node.clone(),
                    cancel: Default::default(),
                }),
            },
        );
        let mut response = EditResponse::default();
        let ui = context.frame();
        ui.window("Wrangle")
            .size([500., 450.], fold_ui::sdk::imgui::Condition::Always)
            .build(|| {
                draw(ui, &mut node, None, &mut drafts, &mut response);
                let _id = ui.push_id("independent group");
                draw(
                    ui,
                    &mut node,
                    Some(fold_foundation::ObjectId::new()),
                    &mut drafts,
                    &mut EditResponse::default(),
                );
            });
        drop(context.render_legacy());
        assert!(response.changed && response.finished);
        assert_eq!(node, candidate);
        assert_eq!(drafts[&key].base_source, "position[0] += 2.0;");
        assert_eq!(drafts[&key].source, "position[0] += 3.0;");
        assert_eq!(drafts.len(), 2);
    }
    #[test]
    fn cancelling_or_dropping_an_editor_job_cancels_its_worker() {
        let (_, rx) = mpsc::channel();
        let cancel = fold_media::Cancel::default();
        let job = Job {
            rx,
            base: Node::new("fold.motion.wrangle").unwrap(),
            cancel: cancel.clone(),
        };
        drop(job);
        assert!(cancel.check().is_err());
    }
}
