//! Installed-package acceptance: real color, native Motion, new code and RIFE.
fn main() {
    if std::env::args().any(|arg| arg == "--isolated")
        && std::path::Path::new(env!("CARGO_MANIFEST_DIR")).exists()
    {
        eprintln!("Isolation failed: the original checkout is accessible");
        std::process::exit(1);
    }
    if let Err(error) = fold_app::runtime_check::run() {
        eprintln!("Fold runtime qualification failed: {error}");
        std::process::exit(1);
    }
}
