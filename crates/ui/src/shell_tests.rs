use super::*;
use crate::sdk::Panel;
use dear_imgui_rs::{ConfigFlags, Context};
use fold_platform::desktop::{DesktopState, PreviewResult};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[test]
fn image_fit() {
    assert_eq!(fitted_size([640, 360], [800., 600.]), [800., 450.]);
    assert_eq!(fitted_size([640, 360], [-1., 90.]), [0., 0.]);
}

type Seen = Rc<RefCell<Vec<(Option<PanelInstanceId>, Option<DocumentId>, bool, u32)>>>;
struct Marker {
    inspector: bool,
    count: Rc<Cell<u32>>,
    seen: Seen,
}
impl Panel for Marker {
    fn id(&self) -> &'static str {
        if self.inspector {
            "test.inspector"
        } else {
            "test.editor"
        }
    }
    fn document_type(&self) -> Option<&'static str> {
        Some("test.kind")
    }
    fn new_instance(&self) -> Option<EditorPanels> {
        let count = Rc::new(Cell::new(0));
        Some(EditorPanels {
            editor: Box::new(Self {
                inspector: false,
                count: count.clone(),
                seen: self.seen.clone(),
            }),
            inspector: Box::new(Self {
                inspector: true,
                count,
                seen: self.seen.clone(),
            }),
        })
    }
    fn draw(&mut self, context: ExtensionUi<'_>) {
        if !self.inspector {
            self.count.set(self.count.get() + 1);
        }
        self.seen.borrow_mut().push((
            context.instance(),
            context.host.state().selection.document,
            self.inspector,
            self.count.get(),
        ));
        context.ui.text("Instance content");
    }
}
struct Host {
    documents: [DocumentId; 2],
    state: DesktopState,
    delivery: DocumentId,
    commands: Vec<DesktopCommand>,
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn video_info(&self, id: DocumentId) -> Result<fold_platform::VideoInfo, String> {
        if !self.documents.contains(&id) {
            return Err("Missing document".into());
        }
        Ok(fold_platform::VideoInfo {
            width: 64,
            height: 64,
            rate: [24, 1],
            frames: 24,
        })
    }
    fn output_info(&self) -> Result<(DocumentId, fold_platform::VideoInfo), String> {
        Ok((self.delivery, self.video_info(self.delivery)?))
    }
    fn poll(&mut self) {}
    fn command(&mut self, command: DesktopCommand) {
        self.commands.push(command);
    }
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}

#[test]
fn editor_seeks_choose_one_explicit_viewer_and_leave_others_and_delivery_untouched() {
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    let editor = shell.workspace.add_editor("test.editor", "test.kind");
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(ViewLocation {
            document: docs[0],
            time: Time::ZERO,
            label: String::new(),
        });
    shell.workspace.viewers.clear();
    shell.workspace.publish(editor);
    let a = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Linked,
        editor: Some(editor),
        ..Default::default()
    });
    let other = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Pinned(workspace::DocumentRef {
            document: docs[1],
            output: "video".into(),
            extensions: Default::default(),
        }),
        ..Default::default()
    });
    shell.playback_viewer = Some(other);
    shell.seek_from_editor(editor, Time::new(7, 48).unwrap(), &mut host);
    assert_eq!(shell.workspace.viewers[&a].time, Time::new(7, 48).unwrap());
    assert_eq!(shell.workspace.viewers[&other].time, Time::ZERO);
    assert!(host.commands.is_empty());
    let b = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Linked,
        editor: Some(editor),
        ..Default::default()
    });
    shell.seek_from_editor(editor, Time::new(1, 8).unwrap(), &mut host);
    assert!(matches!(
        host.commands.last(),
        Some(DesktopCommand::Notify(_))
    ));
    assert_eq!(shell.workspace.viewers[&b].time, Time::ZERO);
    shell.workspace.inspector_viewer = Some(b);
    shell.seek_from_editor(editor, Time::new(1, 8).unwrap(), &mut host);
    assert_eq!(shell.workspace.viewers[&b].time, Time::new(1, 8).unwrap());
    assert_eq!(shell.workspace.viewers[&a].time, Time::new(7, 48).unwrap());
    assert_eq!(host.delivery, docs[0]);
    assert!(
        !host
            .commands
            .iter()
            .any(|c| matches!(c, DesktopCommand::Pause | DesktopCommand::SetOutput(_)))
    );
}

#[test]
fn following_preserves_viewer_time_on_root_switch_and_maps_nested_navigation_and_back() {
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    let editor = shell.workspace.add_editor("test.editor", "test.kind");
    let location = |document, time| ViewLocation {
        document,
        time,
        label: String::new(),
    };
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(location(docs[0], Time::ZERO));
    shell.workspace.publish(editor);
    let viewer = shell.workspace.add_viewer(ViewerInstance {
        binding: ViewerBinding::Linked,
        editor: Some(editor),
        time: Time::new(7, 48).unwrap(),
        ..Default::default()
    });
    shell.workspace.reconcile();
    shell
        .workspace
        .editors
        .get_mut(&editor)
        .unwrap()
        .bind(location(docs[1], Time::ZERO));
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(7, 48).unwrap()
    );
    // An explicit Open Source carries the associated edit-time context into
    // its breadcrumb, rather than treating it as an ordinary root switch.
    let binding = shell.workspace.editors.get_mut(&editor).unwrap();
    binding.navigation.last_mut().unwrap().time = Time::new(7, 48).unwrap();
    binding.navigate(location(docs[0], Time::new(1, 48).unwrap()));
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(1, 48).unwrap()
    );
    shell.workspace.editors.get_mut(&editor).unwrap().back();
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(7, 48).unwrap()
    );
    assert_eq!(host.delivery, docs[0]);
}

#[test]
fn joining_another_group_preserves_time_even_when_its_source_has_nested_navigation() {
    use workspace::LinkGroup;
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let mut shell = Shell::new(vec![]);
    let a = shell.workspace.add_editor("test.editor", "test.kind");
    let b = shell.workspace.add_editor("test.editor", "test.kind");
    let location = |document, time| ViewLocation {
        document,
        time,
        label: String::new(),
    };
    shell
        .workspace
        .editors
        .get_mut(&a)
        .unwrap()
        .bind(location(docs[0], Time::ZERO));
    shell
        .workspace
        .editors
        .get_mut(&b)
        .unwrap()
        .bind(location(docs[0], Time::ZERO));
    shell
        .workspace
        .editors
        .get_mut(&b)
        .unwrap()
        .navigate(location(docs[1], Time::new(1, 48).unwrap()));
    shell.workspace.publish(a);
    shell.workspace.set_editor_group(b, LinkGroup::B);
    let viewer = *shell.workspace.viewers.keys().next().unwrap();
    shell.workspace.viewers.get_mut(&viewer).unwrap().time = Time::new(7, 48).unwrap();
    shell.refresh_viewer_targets(&mut host);
    shell.workspace.set_viewer_group(viewer, LinkGroup::B);
    shell.refresh_viewer_targets(&mut host);
    assert_eq!(shell.workspace.resolve(viewer).unwrap().document, docs[1]);
    assert_eq!(
        shell.workspace.viewers[&viewer].time,
        Time::new(7, 48).unwrap()
    );
    assert_eq!(host.delivery, docs[0]);
}

#[test]
fn runtime_editors_and_inspectors_are_isolated_at_normal_and_narrow_widths() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .io_mut()
        .set_config_flags(ConfigFlags::DOCKING_ENABLE);
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_delta_time(1. / 60.);
    let docs = [DocumentId::new(), DocumentId::new()];
    let mut host = Host {
        documents: docs,
        state: Default::default(),
        delivery: docs[0],
        commands: vec![],
    };
    let seen = Seen::default();
    let panels = [
        ("test.editor", "Editor", PanelPlacement::Editor, false),
        (
            "test.inspector",
            "Inspector",
            PanelPlacement::Inspector,
            true,
        ),
    ]
    .into_iter()
    .map(|(id, title, placement, inspector)| RegisteredPanel {
        descriptor: fold_platform::packages::PanelDescriptor {
            id,
            title,
            placement,
        },
        key: WindowKey::new(id, title).unwrap(),
        panel: Box::new(Marker {
            inspector,
            count: Default::default(),
            seen: seen.clone(),
        }),
    })
    .collect();
    let mut shell = Shell::new(panels);
    shell.bootstrap = false;
    let a = *shell.workspace.editors.keys().next().unwrap();
    let b = shell.workspace.add_editor("test.editor", "test.kind");
    for (id, document) in [(a, docs[0]), (b, docs[1])] {
        shell
            .workspace
            .editors
            .get_mut(&id)
            .unwrap()
            .bind(ViewLocation {
                document,
                time: Time::ZERO,
                label: String::new(),
            });
    }
    shell.workspace.publish(a);
    shell.workspace.set_editor_group(b, workspace::LinkGroup::B);
    shell.sync_instances();
    for width in [1280., 520.] {
        context.io_mut().set_display_size([width, 600.]);
        shell.reset_layout = true;
        for id in [a, b, a] {
            shell.workspace.inspector_group = shell.workspace.editors[&id].group;
            shell.focus = Some(id);
            for _ in 0..3 {
                shell.prepare_frame(&mut context);
                let ui = context.frame();
                shell.controls(ui, &mut host).unwrap();
                shell.viewer(ui, &Preview::Pending, "test", &mut host);
                context.end_frame();
            }
        }
    }
    let seen = seen.borrow();
    for id in [a, b] {
        let document = shell.workspace.editors[&id].document();
        assert!(
            seen.iter()
                .any(|&(owner, target, inspector, _)| owner == Some(id)
                    && target == document
                    && inspector)
        );
        assert!(
            seen.iter()
                .filter(|&&(owner, _, _, _)| owner == Some(id))
                .all(|&(_, target, _, _)| target == document)
        );
        assert!(
            seen.iter()
                .any(|&(owner, _, inspector, count)| owner == Some(id) && !inspector && count == 1),
            "each instance starts with fresh gesture state"
        );
    }
    assert_eq!(host.delivery, docs[0]);
    assert!(!host.commands.iter().any(|c| matches!(
        c,
        DesktopCommand::ActivateWorkspace(_)
            | DesktopCommand::Navigate(_)
            | DesktopCommand::SetOutput(_)
    )));
}
