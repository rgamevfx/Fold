//! Desktop assembly. All I/O, decode, and export execute on worker threads.
use fold_platform::desktop::{DesktopClient, DesktopCommand};
fn main() -> std::process::ExitCode {
    let mut session = fold_app::session::Session::default();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.is_empty() {
        if args[0] == "--project" && args.len() == 2 {
            session.command(DesktopCommand::Open(args[1].clone().into()));
        } else if args[0] == "--compositor" && args.len() == 2 {
            session.command(DesktopCommand::OpenInWorkspace {
                path: args[1].clone().into(),
                document_type: fold_compositor::COMPOSITE.into(),
            });
        } else if args[0] == "--motion" && args.len() == 2 {
            session.command(DesktopCommand::OpenInWorkspace {
                path: args[1].clone().into(),
                document_type: fold_motion::MOTION.into(),
            });
        } else if args[0] == "--layers" {
            session.command(DesktopCommand::Import(
                args[1..].iter().map(Into::into).collect(),
            ));
        } else {
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
        }
    }
    let mut panels = fold_ui::sdk::PanelRegistry::new(&fold_app::packages::builtins());
    if let Err(error) = fold_timeline::ui::register(&mut panels)
        .and_then(|()| fold_compositor::ui::register(&mut panels))
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
