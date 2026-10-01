use crate::{
    CommittedSnapshot, Metadata, Project, ProjectError, ProjectState, coordinator::validate,
};
use serde::{Deserialize, Serialize};
use std::{
    fmt,
    fs::File,
    io::{self, BufReader, Write},
    path::Path,
};

const FORMAT: &str = "fold-project";
const VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct Archive {
    format: String,
    version: u32,
    project: ProjectState,
    #[serde(flatten)]
    extensions: Metadata,
}

#[derive(Debug)]
pub enum LoadError {
    Io(io::Error),
    Json(serde_json::Error),
    UnsupportedFormat { format: String, version: u32 },
    InvalidProject(ProjectError),
}

#[derive(Debug)]
pub enum SaveError {
    Io(io::Error),
    Json(serde_json::Error),
    /// Replacement succeeded, but directory durability could not be confirmed.
    /// The new archive is visible; do not report that the old archive survived.
    DirectorySync(io::Error),
}

impl fmt::Display for LoadError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for LoadError {}
impl fmt::Display for SaveError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}
impl std::error::Error for SaveError {}

/// Worker-side synchronous I/O. History and transient previews are not persisted.
pub fn load(path: impl AsRef<Path>, history_limit: usize) -> Result<Project, LoadError> {
    let file = File::open(path).map_err(LoadError::Io)?;
    let mut archive: Archive =
        serde_json::from_reader(BufReader::new(file)).map_err(LoadError::Json)?;
    if archive.format != FORMAT || archive.version != VERSION {
        return Err(LoadError::UnsupportedFormat {
            format: archive.format,
            version: archive.version,
        });
    }
    validate(&archive.project).map_err(LoadError::InvalidProject)?;
    archive.project.archive_extensions = archive.extensions;
    Ok(Project::from_state(archive.project, history_limit))
}

/// Save only a pinned committed snapshot. Run off the UI thread.
/// Same-directory replacement is atomic on the initial Linux reference platform.
/// Transient evaluation snapshots do not carry persistence authority:
/// ```compile_fail
/// use fold_project::{save, Snapshot};
/// fn save_preview(preview: &Snapshot) {
///     save(preview, "preview.fold").unwrap();
/// }
/// ```
pub fn save(snapshot: &CommittedSnapshot, path: impl AsRef<Path>) -> Result<(), SaveError> {
    let archive = Archive {
        format: FORMAT.into(),
        version: VERSION,
        project: snapshot.state().clone(),
        extensions: snapshot.state().archive_extensions.clone(),
    };
    replace_archive(path.as_ref(), |file| {
        serde_json::to_writer(file, &archive).map_err(SaveError::Json)
    })
}

fn replace_archive(
    path: &Path,
    write: impl FnOnce(&mut File) -> Result<(), SaveError>,
) -> Result<(), SaveError> {
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(SaveError::Io)?;
    write(temporary.as_file_mut())?;
    temporary.flush().map_err(SaveError::Io)?;
    temporary.as_file().sync_all().map_err(SaveError::Io)?;
    temporary
        .persist(path)
        .map_err(|error| SaveError::Io(error.error))?;
    #[cfg(unix)]
    File::open(parent)
        .and_then(|directory| directory.sync_all())
        .map_err(SaveError::DirectorySync)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failed_and_abandoned_writes_preserve_previous_archive() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("project.fold");
        let project = Project::new(5);
        save(&project.snapshot(), &path).unwrap();
        let previous = std::fs::read(&path).unwrap();
        let result = replace_archive(&path, |file| {
            file.write_all(b"partial archive").unwrap();
            Err(SaveError::Io(io::Error::other(
                "injected interrupted write",
            )))
        });
        assert!(matches!(result, Err(SaveError::Io(_))));
        assert_eq!(std::fs::read(&path).unwrap(), previous);
        // Model a process dying with an unfinished sibling file left on disk.
        std::fs::write(directory.path().join(".abandoned-save"), b"partial").unwrap();
        assert_eq!(
            load(&path, 5).unwrap().snapshot().state(),
            project.snapshot().state()
        );
        assert!(matches!(
            save(&project.snapshot(), directory.path()),
            Err(SaveError::Io(_))
        ));
        assert_eq!(std::fs::read(&path).unwrap(), previous);
    }
}
