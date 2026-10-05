//! Disposable files retain their existing RAII ownership in the selected cache root.
use std::{
    path::PathBuf,
    sync::{OnceLock, RwLock},
};
static ROOT: OnceLock<RwLock<PathBuf>> = OnceLock::new();
fn root() -> &'static RwLock<PathBuf> {
    ROOT.get_or_init(|| RwLock::new(std::env::temp_dir()))
}
pub fn configure(path: PathBuf) -> std::io::Result<()> {
    std::fs::create_dir_all(&path)?;
    // Validate write access before changing the active root.
    let _probe = tempfile::NamedTempFile::new_in(&path)?;
    *root().write().unwrap() = path;
    Ok(())
}
pub(crate) fn file() -> std::io::Result<tempfile::NamedTempFile> {
    let path = root().read().unwrap().clone();
    tempfile::NamedTempFile::new_in(path)
}
