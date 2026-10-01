//! A non-timeline package exercises panel registration and generic dispatch.
use crate::sdk::*;
use fold_platform::{ProjectRevision, desktop::*, packages::*};

#[test]
fn independent_package_panel_dispatches_a_registered_command() {
    let _guard = crate::IMGUI_TEST_LOCK.lock().unwrap();
    let mut packages = PackageRegistry::default();
    packages
        .register(Contributions {
            manifest: Manifest {
                id: "test.panel",
                version: "1",
                host_api: HOST_API,
                dependencies: &[],
                build: "test",
                panels: &[PanelDescriptor {
                    id: "test.panel.editor",
                    title: "Test editor",
                    placement: PanelPlacement::Editor,
                }],
            },
            documents: vec![],
            commands: vec![CommandRegistration {
                id: "test.panel.command",
                title: "Test command",
                execution: Execution::Immediate,
                handler: |_, _, _| Err("UI dispatch test only".into()),
            }],
            video: vec![],
            audio: vec![],
        })
        .unwrap();
    struct TestPanel;
    impl Panel for TestPanel {
        fn id(&self) -> &'static str {
            "test.panel.editor"
        }
        fn draw(&mut self, context: ExtensionUi<'_>) {
            context.ui.text("Independent registered panel");
            context
                .host
                .command(DesktopCommand::Extension(CommandRequest {
                    id: "test.panel.command".into(),
                    base: ProjectRevision(0),
                    arguments: vec![],
                }));
        }
    }
    let mut panels = PanelRegistry::new(&packages);
    assert!(panels.register("foreign", TestPanel).is_err());
    panels.register("test.panel", TestPanel).unwrap();
    assert!(panels.register("test.panel", TestPanel).is_err());
    let mut entries = panels.finish().unwrap();
    #[derive(Default)]
    struct Client {
        state: DesktopState,
        request: Option<CommandRequest>,
    }
    impl DesktopClient for Client {
        fn state(&self) -> &DesktopState {
            &self.state
        }
        fn poll(&mut self) {}
        fn command(&mut self, command: DesktopCommand) {
            if let DesktopCommand::Extension(request) = command {
                self.request = Some(request);
            }
        }
        fn request_preview(&mut self, _: PreviewKey) {}
        fn cancel_preview(&mut self) {}
        fn take_preview(&mut self) -> Option<PreviewResult> {
            None
        }
    }
    let mut client = Client::default();
    let mut context = imgui::Context::create();
    context.set_ini_filename(None::<String>).unwrap();
    context
        .font_atlas()
        .try_claim_legacy_renderer()
        .unwrap()
        .build();
    context.io_mut().set_display_size([800.0, 600.0]);
    context.io_mut().set_delta_time(1.0 / 60.0);
    let ui = context.frame();
    entries[0].panel.draw(ExtensionUi {
        ui,
        host: &mut client,
    });
    context.end_frame();
    assert_eq!(
        packages.command(&client.request.unwrap().id).unwrap().title,
        "Test command"
    );
}
