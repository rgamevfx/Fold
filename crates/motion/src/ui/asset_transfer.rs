//! Contextual library file operations; file work never runs on the UI thread.
use super::state::State;
use crate::{Motion, program::bundle};
use fold_foundation::ObjectId;
use fold_platform::desktop::DesktopClient;
use fold_ui::sdk::imgui::{Condition, Ui};
use std::sync::mpsc::{self, Receiver};
#[derive(Clone, Copy)]
pub enum Action {
    Load(ObjectId),
    Save(ObjectId),
}
pub struct Transfer {
    action: Action,
    path: String,
    message: String,
    work: Option<Work>,
}
struct Work {
    base: Motion,
    rx: Receiver<Result<Option<Motion>, String>>,
    cancel: fold_media::Cancel,
}
impl Drop for Work {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
pub fn open(state: &mut State, action: Action) {
    state.asset_transfer = Some(Transfer {
        action,
        path: String::new(),
        message: String::new(),
        work: None,
    });
}
pub fn window(ui: &Ui, host: &mut dyn DesktopClient, state: &mut State) {
    let Some(mut draft) = state.asset_transfer.take() else {
        return;
    };
    let Some(current) = state.motion.as_ref() else {
        return;
    };
    let load = matches!(draft.action, Action::Load(_));
    let mut open = true;
    let mut apply = None;
    ui.window(if load {"Load procedural asset"} else {"Save procedural asset"})
        .opened(&mut open).size([480.,230.],Condition::FirstUseEver).build(||{
            ui.text_wrapped(if load {"Load a .foldasset file into this instance. Compatible values and keys are retained; incompatible inputs and their animation are removed."} else {"Save the reusable definition, source and embedded fonts. Instance values stay in this project."});
            let disabled=ui.begin_disabled_with_cond(draft.work.is_some());
            ui.set_next_item_width(-1.);
            ui.input_text("##asset-path",&mut draft.path).hint("Absolute path to a .foldasset file").build();
            let start=ui.button(if load {"Load asset"} else {"Save asset"});drop(disabled);
            if start {
                let path=std::path::PathBuf::from(&draft.path);
                if !path.is_absolute() {draft.message="Choose an absolute file path.".into();}
                else {
                    let base=current.clone();let mut candidate=base.clone();let action=draft.action;
                    let cancel=fold_media::Cancel::default();let token=cancel.clone();let(tx,rx)=mpsc::channel();
                    std::thread::spawn(move||{
                        let result=(||{
                            token.check()?;
                            match action {
                                Action::Load(instance)=>{
                                    let asset=bundle::read(&path)?;
                                    let definition=bundle::import(&mut candidate,&asset)?;
                                    crate::authoring::assign_group(&mut candidate,instance,definition)?;
                                    candidate.validate()?;token.check()?;Ok(Some(candidate))
                                },
                                Action::Save(definition)=>{
                                    let asset=bundle::export(&candidate,definition)?;token.check()?;
                                    bundle::write(&asset,&path)?;Ok(None)
                                }
                            }
                        })();let _=tx.send(result);
                    });
                    draft.work=Some(Work{base,rx,cancel});draft.message.clear();
                }
            }
            if let Some(work)=&draft.work {
                match work.rx.try_recv() {
                    Ok(result)=>{
                        match result {
                            Ok(Some(m)) if current==&work.base=>apply=Some(m),
                            Ok(Some(_))=>draft.message="Artwork changed while loading. Load again against the current version.".into(),
                            Ok(None)=>draft.message="Asset saved.".into(),
                            Err(e)=>draft.message=e,
                        }draft.work=None;
                    },
                    Err(mpsc::TryRecvError::Empty)=>{ui.text("Working…");},
                    Err(mpsc::TryRecvError::Disconnected)=>{draft.work=None;draft.message="Asset worker stopped.".into();}
                }
            }
            if !draft.message.is_empty(){ui.text_wrapped(&draft.message);}
        });
    if let Some(m) = apply {
        if state.editing {
            draft.message = "Finish the current property edit, then load again.".into();
        } else {
            state.motion = Some(m);
            state.preview(host);
            state.commit(host);
            open = false;
        }
    }
    if open {
        state.asset_transfer = Some(draft);
    }
}
