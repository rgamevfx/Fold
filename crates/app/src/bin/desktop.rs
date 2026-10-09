//! Desktop assembly. All I/O, decode, and export execute on worker threads.
use fold_platform::desktop::{DesktopClient, DesktopCommand};
fn main() -> std::process::ExitCode {
    let mut session = fold_app::session::Session::default();
    let mut args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.first().is_some_and(|arg| arg == "--protocol-dir") {
        #[cfg(target_os = "linux")]
        {
            let Some(parent) = args.get(1) else {
                eprintln!("--protocol-dir requires a directory");
                return std::process::ExitCode::FAILURE;
            };
            match session.enable_agent_transport(std::path::Path::new(parent)) {
                Ok(path) => eprintln!("Fold protocol socket: {}", path.display()),
                Err(error) => {
                    eprintln!("Cannot enable Fold protocol: {error}");
                    return std::process::ExitCode::FAILURE;
                }
            }
            args.drain(..2);
        }
        #[cfg(not(target_os = "linux"))]
        {
            eprintln!("Local protocol transport is not qualified on this platform");
            return std::process::ExitCode::FAILURE;
        }
    }
    if !args.is_empty() {
        if args[0] == "--project" && args.len() == 2 {
            session.command(DesktopCommand::Open(args[1].clone().into()));
        } else if args[0] == "--composition" && args.len() == 2 {
            session.command(DesktopCommand::OpenInWorkspace {
                path: args[1].clone().into(),
                document_type: fold_composition::COMPOSITION.into(),
            });
        } else if args[0] == "--compositor" && args.len() == 2 {
            session.command(DesktopCommand::OpenInWorkspace {
                path: args[1].clone().into(),
                document_type: fold_compositor::COMPOSITE.into(),
            });
        } else if args[0] == "--timeline" {
            // Explicit sequence import; ordinary file arguments are asset-only.
            let snapshot = session.snapshot().unwrap();
            let request = fold_timeline::package::request(
                &snapshot,
                fold_timeline::package::IMPORT,
                &fold_timeline::ImportArgs {
                    document: None,
                    paths: args[usize::from(args[0] == "--timeline")..]
                        .iter()
                        .map(Into::into)
                        .collect(),
                    at: None,
                    video_track: None,
                    audio_track: None,
                    audio_only: false,
                },
            )
            .expect("serializable import arguments");
            session.command(DesktopCommand::Extension(request));
        } else {
            session.command(DesktopCommand::Browser(
                fold_platform::browser::BrowserCommand::Import {
                    paths: args.iter().map(Into::into).collect(),
                    destination: fold_project::Parent::Root,
                },
            ));
        }
    }
    let mut panels = fold_ui::sdk::PanelRegistry::new(&fold_app::packages::builtins());
    if let Err(error) = fold_app::project_panel::register(&mut panels)
        .and_then(|()| fold_timeline::ui::register(&mut panels))
        .and_then(|()| fold_compositor::ui::register(&mut panels))
        .and_then(|()| fold_composition::ui::register(&mut panels))
        .and_then(|()| fold_motion::ui::register(&mut panels))
    {
        eprintln!("Fold panel registration failed: {error}");
        return std::process::ExitCode::FAILURE;
    }
    match fold_ui::run_desktop(Box::new(session), panels) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Fold desktop failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
