use super::*;
use fold_platform::desktop::{DesktopState, PreviewKey, PreviewResult};
use fold_project::{CommittedSnapshot, Project};
use imgui::{Condition, Context};
struct Host {
    project: Project,
    state: DesktopState,
    imports: Vec<(Parent, Vec<std::path::PathBuf>)>,
}
impl DesktopClient for Host {
    fn state(&self) -> &DesktopState {
        &self.state
    }
    fn snapshot(&self) -> Option<CommittedSnapshot> {
        Some(self.project.snapshot())
    }
    fn document_kinds(&self) -> Vec<fold_platform::browser::DocumentKind> {
        crate::packages::builtins().document_kinds()
    }
    fn supports_document(&self, document: &fold_project::Document) -> bool {
        crate::packages::builtins().supports(document)
    }
    fn command(&mut self, command: DesktopCommand) {
        if let DesktopCommand::Browser(command) = command {
            if let Command::Import { paths, destination } = command {
                self.imports.push((destination, paths));
                return;
            }
            let batch = crate::browser::edit(&self.project.snapshot(), command).unwrap();
            self.project.commit(batch).unwrap();
        } else {
            panic!("unexpected command {command:?}");
        }
    }
    fn poll(&mut self) {}
    fn request_preview(&mut self, _: PreviewKey) {}
    fn cancel_preview(&mut self) {}
    fn take_preview(&mut self) -> Option<PreviewResult> {
        None
    }
}
fn host() -> Host {
    let mut host = Host {
        project: Project::new(16),
        state: DesktopState::default(),
        imports: vec![],
    };
    send(
        &mut host,
        Command::NewBin {
            parent: Parent::Root,
            name: "Footage".into(),
        },
    );
    let bin = *host
        .project
        .snapshot()
        .state()
        .organization
        .bins
        .keys()
        .next()
        .unwrap();
    send(
        &mut host,
        Command::Create {
            kind: fold_timeline::SEQUENCE.into(),
            parent: Parent::Bin(bin),
        },
    );
    send(
        &mut host,
        Command::Create {
            kind: fold_compositor::COMPOSITE.into(),
            parent: Parent::Root,
        },
    );
    host
}
#[test]
fn folders_tree_search_and_draw_are_ui_state_only() {
    let mut host = host();
    let snapshot = host.project.snapshot();
    let bin = *snapshot.state().organization.bins.keys().next().unwrap();
    let mut panel = ProjectPanel::default();
    assert_eq!(panel.rows(&snapshot).len(), 2);
    panel.folder = Parent::Bin(bin);
    assert_eq!(panel.rows(&snapshot)[0].name, "Sequence");
    panel.tree = true;
    assert_eq!(panel.rows(&snapshot).len(), 2);
    panel.expanded.insert(bin);
    let rows = panel.rows(&snapshot);
    assert_eq!(rows.len(), 3);
    assert_eq!(rows[0].entry, Entry::Bin(bin));
    assert_eq!(rows[1].name, "Sequence");
    assert_eq!(rows[1].depth, 1);
    panel.search = "SEQu".into();
    assert_eq!(panel.rows(&snapshot).len(), 1);
    panel.search.clear();
    panel.folder = Parent::Root;
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 700.]);
    context.io_mut().set_delta_time(1. / 60.);
    context
        .io_mut()
        .set_config_flags(imgui::ConfigFlags::NAV_ENABLE_KEYBOARD);
    for width in [600., 240., 180.] {
        for tree in [false, true] {
            panel.tree = tree;
            for frame in 0..4 {
                let ui = context.frame();
                ui.window("Project")
                    .position([0.; 2], Condition::Always)
                    .size([width, 600.], Condition::Always)
                    .build(|| {
                        panel.draw(ExtensionUi {
                            ui,
                            host: &mut host,
                        });
                        if frame > 1 {
                            assert_eq!(ui.scroll_max_x(), 0.);
                        }
                    });
                assert!(context.render_legacy().total_vtx_count() > 100);
            }
        }
    }
    assert_eq!(host.project.snapshot().state(), snapshot.state());
}

#[test]
fn typed_drag_moves_an_item_into_a_bin_as_one_undoable_edit() {
    let mut host = host();
    let before = host.project.snapshot();
    let bin = *before.state().organization.bins.keys().next().unwrap();
    let item = before
        .state()
        .organization
        .items
        .iter()
        .find(|i| i.name == "Composition")
        .unwrap()
        .id;
    let mut panel = ProjectPanel::default();
    let mut context = Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([1000., 700.]);
    context.io_mut().set_delta_time(1. / 60.);
    let frame = |context: &mut Context, panel: &mut ProjectPanel, host: &mut Host| {
        let ui = context.frame();
        ui.window("Project")
            .position([0.; 2], Condition::Always)
            .size([600., 600.], Condition::Always)
            .build(|| panel.draw(ExtensionUi { ui, host }));
        let _ = context.render_legacy();
    };
    for _ in 0..3 {
        frame(&mut context, &mut panel, &mut host);
    }
    let (_, min, max) = panel.bin_rects[0];
    let paths = vec![std::path::PathBuf::from("source.mp4")];
    assert!(panel.files_dropped([min[0] + 2., min[1] + 2.], &paths, &mut host));
    assert_eq!(
        host.imports.pop().unwrap(),
        (Parent::Bin(bin), paths.clone())
    );
    assert!(panel.files_dropped([20., 550.], &paths, &mut host));
    assert_eq!(host.imports.pop().unwrap(), (Parent::Root, paths.clone()));
    assert!(!panel.files_dropped([900., 650.], &paths, &mut host));
    let source = [max[0] + 20., min[1] + 25.];
    let target = [min[0] + 25., min[1] + 25.];
    context.io_mut().add_mouse_pos_event(source);
    frame(&mut context, &mut panel, &mut host);
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, true);
    frame(&mut context, &mut panel, &mut host);
    context
        .io_mut()
        .add_mouse_pos_event([source[0] + 12., source[1]]);
    frame(&mut context, &mut panel, &mut host);
    context.io_mut().add_mouse_pos_event(target);
    for _ in 0..2 {
        frame(&mut context, &mut panel, &mut host);
    }
    context
        .io_mut()
        .add_mouse_button_event(MouseButton::Left, false);
    for _ in 0..2 {
        frame(&mut context, &mut panel, &mut host);
    }
    assert_eq!(
        host.project
            .snapshot()
            .state()
            .organization
            .items
            .iter()
            .find(|i| i.id == item)
            .unwrap()
            .parent,
        Parent::Bin(bin)
    );
    host.project.undo().unwrap();
    assert_eq!(
        host.project.snapshot().state().organization,
        before.state().organization
    );
    assert_eq!(
        host.project.snapshot().state().documents,
        before.state().documents
    );
    // Keyboard folder navigation changes browser state, never the project.
    panel.selected = Some(Entry::Bin(bin));
    context.io_mut().add_key_event(Key::Enter, true);
    frame(&mut context, &mut panel, &mut host);
    context.io_mut().add_key_event(Key::Enter, false);
    frame(&mut context, &mut panel, &mut host);
    assert_eq!(panel.folder, Parent::Bin(bin));
}
