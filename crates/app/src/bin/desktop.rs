//! Desktop composition root. Backend details stay inside the shared UI SDK.
fn main() -> std::process::ExitCode {
    match fold_ui::run_desktop() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Fold desktop failed: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}
