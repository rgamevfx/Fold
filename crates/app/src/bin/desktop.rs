//! Desktop assembly. All I/O, decode, and export execute on worker threads.
use fold_platform::desktop::{DesktopClient, DesktopCommand};
fn main() -> std::process::ExitCode {
    let mut session = fold_app::session::Session::default();
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if !args.is_empty() {
        if args[0] == "--project" && args.len() == 2 {
            session.command(DesktopCommand::Open(args[1].clone().into()));
        } else {
            session.command(DesktopCommand::Import(
                args.into_iter().map(Into::into).collect(),
            ));
        }
    }
    match fold_ui::run_desktop(Box::new(session)) {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Fold desktop failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
