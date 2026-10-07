//! Contextual source editor. Compilation operates on a pinned clone off the UI thread.
use super::state::State;
use crate::{Motion, program};
use fold_foundation::ObjectId;
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::imgui::{Condition, Ui};
use std::sync::mpsc::{self, Receiver};

pub struct Draft {
    definition: ObjectId,
    source: String,
    error: String,
    migration_from: String,
    migration_to: String,
    migration_action: usize,
    job: Option<Job>,
}
struct Job {
    base: Motion,
    rx: Receiver<Result<Motion, String>>,
    cancel: fold_media::Cancel,
}
impl Drop for Job {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub fn open(state: &mut State, definition: ObjectId) {
    if let Some(record) = state
        .motion
        .as_ref()
        .and_then(|m| program::record(m, definition).ok().flatten())
    {
        state.program_draft = Some(Draft {
            definition,
            source: record.source,
            error: String::new(),
            migration_from: String::new(),
            migration_to: String::new(),
            migration_action: 0,
            job: None,
        });
    }
}
pub fn window(ui: &Ui, host: &mut dyn DesktopClient, state: &mut State) {
    let Some(mut draft) = state.program_draft.take() else {
        return;
    };
    let Some(current) = state.motion.as_ref() else {
        return;
    };
    if !current.groups.contains_key(&draft.definition) {
        return;
    }
    let mut open = true;
    let mut apply = None;
    ui.window("Rust asset source")
        .opened(&mut open)
        .size([640., 500.], Condition::FirstUseEver)
        .build(|| {
            ui.text_wrapped("Source updates every instance. Instance controls and animation are preserved.");
            {
                // Keep the visible source identical to the source being compiled.
                let _disabled = ui.begin_disabled_with_cond(draft.job.is_some());
                let size = [ui.content_region_avail()[0], (ui.content_region_avail()[1] - 210.).max(100.)];
                ui.input_text_multiline("##asset-source", &mut draft.source, size).build();
                if ui.collapsing_header("Interface change", fold_ui::sdk::imgui::TreeNodeFlags::empty()) {
                    let selected = current.groups[&draft.definition].inputs.iter().find(|p|p.id==draft.migration_from).map(|p|p.name.as_str()).unwrap_or("None");
                    if let Some(_combo)=ui.begin_combo("Existing control",selected) {
                        if ui.selectable("None") {draft.migration_from.clear();}
                        for port in &current.groups[&draft.definition].inputs {
                            if ui.selectable(&port.name) {draft.migration_from=port.id.clone();draft.migration_to=port.id.clone();}
                        }
                    }
                    let actions=["Rename — preserve edits","Remove — discard edits","Replace — use new default"];
                    if let Some(_combo)=ui.begin_combo("Action",actions[draft.migration_action]) {
                        for (i,label) in actions.iter().enumerate() {if ui.selectable(*label) {draft.migration_action=i;}}
                    }
                    if draft.migration_action!=1 {ui.input_text("New source control ID",&mut draft.migration_to).build();}
                    if draft.migration_action!=0 {ui.text_wrapped("This explicitly discards the old control's values, connections and animation on every instance.");}
                }
            }
            if draft.job.is_some() {
                if ui.button("Cancel compile") {
                    draft.job = None;
                }
            } else if ui.button("Compile and apply") {
                let base = current.clone();
                let mut candidate = base.clone();
                let source = draft.source.clone();
                let definition = draft.definition;
                let from=draft.migration_from.clone();let to=draft.migration_to.clone();let action=draft.migration_action;
                let cancel = fold_media::Cancel::default();
                let token = cancel.clone();
                let (tx, rx) = mpsc::channel();
                std::thread::spawn(move || {
                    let result = (|| {
                        let migrations=if from.is_empty() {vec![]} else {vec![match action {
                            0=>program::Migration::Rename{from,to},
                            1=>program::Migration::Remove{input:from},
                            _=>program::Migration::Replace{from,value:program::compiler::compile(&source,&token)?.input_default(&to).ok_or("Replacement requires a default in the new source")?,to},
                        }]};
                        program::define_with_migrations(&mut candidate, definition, &source, &migrations, &token)?;Ok(candidate)
                    })();
                    let _ = tx.send(result);
                });
                draft.job = Some(Job { base, rx, cancel });
                draft.error.clear();
            }
            if let Some(job) = &draft.job {
                match job.rx.try_recv() {
                    Ok(result) => {
                        match result {
                            Ok(candidate) if current == &job.base => apply = Some(candidate),
                            Ok(_) => draft.error = "Artwork changed during compilation. Compile again against the current revision.".into(),
                            Err(error) => draft.error = error,
                        }
                        draft.job = None;
                    }
                    Err(mpsc::TryRecvError::Empty) => ui.text("Compiling Rust…"),
                    Err(mpsc::TryRecvError::Disconnected) => {
                        draft.job = None;
                        draft.error = "Compiler worker stopped".into();
                    }
                }
            }
            if !draft.error.is_empty() {
                ui.text_wrapped(&draft.error);
            }
        });
    if let Some(candidate) = apply {
        if state.editing {
            draft.error = "Finish the current property edit, then compile again.".into();
        } else {
            draft.migration_from.clear();
            state.motion = Some(candidate);
            state.preview(host);
            state.commit(host);
        }
    }
    if open {
        state.program_draft = Some(draft);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::{IMGUI_TEST_LOCK, tests::Host};
    use fold_ui::sdk::imgui;

    #[test]
    fn completed_source_edit_commits_once_and_stale_result_preserves_artist_edit() {
        let _guard = IMGUI_TEST_LOCK.lock().unwrap();
        let mut host = Host::new();
        let mut state = State::default();
        state.create(&mut host);
        let definition = ObjectId::new();
        let instance = ObjectId::new();
        let source = "let mut a=Asset::new(\"Circle\"); let r=a.scalar(\"radius\",\"Radius\",12.,\"px\"); let c=a.circle(\"circle\",r); a.output(&c); a";
        state.change_scene(&mut host, |m| {
            program::define(m, definition, source, &Default::default())?;
            program::scene_instance(m, definition, instance)?;
            Ok(Some(instance))
        });
        assert!(state.error.is_empty(), "{}", state.error);
        let before = host.project.snapshot();
        let base = state.motion.clone().unwrap();
        let mut revised = base.clone();
        program::define(
            &mut revised,
            definition,
            &source.replace("Circle", "Circles"),
            &Default::default(),
        )
        .unwrap();
        let mut context = imgui::Context::create();
        context.set_ini_filename(None::<String>).unwrap();
        context
            .font_atlas()
            .try_claim_legacy_renderer()
            .unwrap()
            .build();
        context.io_mut().set_display_size([900., 700.]);
        context.io_mut().set_delta_time(1. / 60.);

        // Deliver the same worker completion that the source window consumes.
        for stale in [false, true] {
            open(&mut state, definition);
            let (tx, rx) = mpsc::channel();
            tx.send(Ok(revised.clone())).unwrap();
            let job_base = state.motion.clone().unwrap();
            state.program_draft.as_mut().unwrap().job = Some(Job {
                base: job_base,
                rx,
                cancel: Default::default(),
            });
            if stale {
                state
                    .motion
                    .as_mut()
                    .unwrap()
                    .graph
                    .nodes
                    .iter_mut()
                    .find(|n| n.id == instance)
                    .unwrap()
                    .set("radius", crate::fields::Datum::Scalar(42.));
            }
            let ui = context.frame();
            window(ui, &mut host, &mut state);
            context.end_frame();
            assert_eq!(
                host.project.snapshot().revision().0,
                before.revision().0 + 1
            );
            if stale {
                assert!(
                    state
                        .program_draft
                        .as_ref()
                        .unwrap()
                        .error
                        .contains("Artwork changed")
                );
                assert_eq!(
                    state
                        .motion
                        .as_ref()
                        .unwrap()
                        .graph
                        .node(instance)
                        .unwrap()
                        .inputs["radius"],
                    crate::graph::Input::Value(crate::fields::Datum::Scalar(42.))
                );
            } else {
                assert_eq!(
                    state.motion.as_ref().unwrap().groups[&definition].name,
                    "Circles"
                );
            }
        }
        host.project.undo().unwrap();
        assert_eq!(
            host.project.snapshot().state().documents,
            before.state().documents
        );
    }
}
