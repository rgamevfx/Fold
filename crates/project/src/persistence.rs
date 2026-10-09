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
// Version 3 requires explicit color-aware application interpretation. Older
// applications must reject new ACES archives, not silently render them as sRGB.
const VERSION: u32 = 3;

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
    /// No archive was published because cancellation was observed before publication.
    Cancelled(String),
    /// Exclusive creation found an existing directory entry; it was not replaced.
    AlreadyExists,
    Io(io::Error),
    Json(serde_json::Error),
    /// Archive publication succeeded, but directory durability could not be confirmed.
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
    save_with(snapshot, path.as_ref(), Publication::Replace, &|| Ok(()))
}

/// Create a new archive without replacing any existing directory entry. Run on a
/// worker. Cancellation is checked during serialization and immediately before
/// publication; after publication success/DirectorySync wins a cancellation race.
/// Like save, this requires a committed snapshot, never a transient candidate.
pub fn save_new(
    snapshot: &CommittedSnapshot,
    path: impl AsRef<Path>,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<(), SaveError> {
    save_with(snapshot, path.as_ref(), Publication::CreateNew, check)
}

#[derive(Clone, Copy)]
enum Publication {
    Replace,
    CreateNew,
}

fn save_with(
    snapshot: &CommittedSnapshot,
    path: &Path,
    publication: Publication,
    check: &dyn Fn() -> Result<(), String>,
) -> Result<(), SaveError> {
    let archive = Archive {
        format: FORMAT.into(),
        version: VERSION,
        project: snapshot.state().clone(),
        extensions: snapshot.state().archive_extensions.clone(),
    };
    publish_archive(path, publication, check, |file| {
        let mut writer = CheckedWriter {
            file,
            check,
            cancelled: None,
        };
        let result = serde_json::to_writer(&mut writer, &archive);
        match writer.cancelled {
            Some(reason) => Err(SaveError::Cancelled(reason)),
            None => result.map_err(SaveError::Json),
        }
    })
}

struct CheckedWriter<'a> {
    file: &'a mut File,
    check: &'a dyn Fn() -> Result<(), String>,
    cancelled: Option<String>,
}
impl Write for CheckedWriter<'_> {
    fn write(&mut self, bytes: &[u8]) -> io::Result<usize> {
        if let Err(reason) = (self.check)() {
            self.cancelled = Some(reason);
            return Err(io::Error::other("archive serialization cancelled"));
        }
        self.file.write(bytes)
    }
    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}
fn publish_archive(
    path: &Path,
    publication: Publication,
    check: &dyn Fn() -> Result<(), String>,
    write: impl FnOnce(&mut File) -> Result<(), SaveError>,
) -> Result<(), SaveError> {
    check().map_err(SaveError::Cancelled)?;
    let parent = path
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut temporary = tempfile::NamedTempFile::new_in(parent).map_err(SaveError::Io)?;
    write(temporary.as_file_mut())?;
    temporary.flush().map_err(SaveError::Io)?;
    temporary.as_file().sync_all().map_err(SaveError::Io)?;
    check().map_err(SaveError::Cancelled)?;
    match publication {
        Publication::Replace => temporary.persist(path),
        Publication::CreateNew => temporary.persist_noclobber(path),
    }
    .map_err(|error| {
        if matches!(publication, Publication::CreateNew)
            && error.error.kind() == io::ErrorKind::AlreadyExists
        {
            SaveError::AlreadyExists
        } else {
            SaveError::Io(error.error)
        }
    })?;
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
        let result = publish_archive(&path, Publication::Replace, &|| Ok(()), |file| {
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
    #[test]
    fn exclusive_save_retains_opaque_state_and_preserves_existing_entries() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("new.fold");
        let mut project = Project::new(4);
        let id = fold_foundation::DocumentId::new();
        project
            .commit(crate::EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![crate::Mutation::PutDocument(crate::Document {
                    id,
                    type_id: "unsupported".into(),
                    package_id: "future.package".into(),
                    schema_version: 99,
                    revision: project.snapshot().revision(),
                    dependencies: vec![],
                    assets: vec![],
                    payload: vec![0, 255, 128, 13, 10],
                    extensions: [("opaque".into(), serde_json::json!({"keep":true}))].into(),
                })],
            })
            .unwrap();
        let mut state = project.snapshot().state().clone();
        state
            .archive_extensions
            .insert("future_archive".into(), serde_json::json!({"version":77}));
        state.extensions.insert(
            "future_project".into(),
            serde_json::json!([1,{"nested":true}]),
        );
        let project = Project::from_state(state, 4);
        save_new(&project.snapshot(), &path, &|| {
            if path.exists() {
                Err("late cancellation must not hide publication".into())
            } else {
                Ok(())
            }
        })
        .unwrap();
        assert_eq!(
            load(&path, 4).unwrap().snapshot().state(),
            project.snapshot().state()
        );
        let original = std::fs::read(&path).unwrap();
        assert!(matches!(
            save_new(&Project::new(0).snapshot(), &path, &|| Ok(())),
            Err(SaveError::AlreadyExists)
        ));
        assert_eq!(std::fs::read(&path).unwrap(), original);
        #[cfg(unix)]
        {
            let link = directory.path().join("link.fold");
            std::os::unix::fs::symlink(&path, &link).unwrap();
            assert!(matches!(
                save_new(&Project::new(0).snapshot(), &link, &|| Ok(())),
                Err(SaveError::AlreadyExists)
            ));
            assert!(
                std::fs::symlink_metadata(&link)
                    .unwrap()
                    .file_type()
                    .is_symlink()
            );
            assert_eq!(std::fs::read(&path).unwrap(), original);
        }
    }

    #[test]
    fn cancelled_new_save_removes_temporary_data_before_publication() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("cancelled.fold");
        let snapshot = Project::new(0).snapshot();
        assert!(
            matches!(save_new(&snapshot,&path,&||Err("before start".into())),Err(SaveError::Cancelled(reason)) if reason == "before start")
        );
        let checks = std::cell::Cell::new(0);
        let check = || {
            checks.set(checks.get() + 1);
            if checks.get() >= 3 {
                Err("during serialization".into())
            } else {
                Ok(())
            }
        };
        assert!(
            matches!(save_new(&snapshot,&path,&check),Err(SaveError::Cancelled(reason)) if reason == "during serialization")
        );
        let cancelled = std::cell::Cell::new(false);
        let result = publish_archive(
            &path,
            Publication::CreateNew,
            &|| {
                if cancelled.get() {
                    Err("before publication".into())
                } else {
                    Ok(())
                }
            },
            |file| {
                file.write_all(b"complete staged bytes").unwrap();
                cancelled.set(true);
                Ok(())
            },
        );
        assert!(
            matches!(result,Err(SaveError::Cancelled(reason)) if reason == "before publication")
        );
        assert!(!path.exists());
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 0);
    }

    #[test]
    fn concurrent_exclusive_saves_publish_exactly_one_complete_archive() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("race.fold");
        let snapshots: Vec<_> = (0..2)
            .map(|writer| {
                Project::with_settings(0, [("writer".into(), serde_json::json!(writer))].into())
                    .snapshot()
            })
            .collect();
        let barrier = std::sync::Barrier::new(2);
        let outcomes = std::thread::scope(|scope| {
            let workers: Vec<_> = snapshots
                .iter()
                .map(|snapshot| {
                    scope.spawn(|| {
                        barrier.wait();
                        save_new(snapshot, &path, &|| Ok(()))
                    })
                })
                .collect();
            workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect::<Vec<_>>()
        });
        assert_eq!(outcomes.iter().filter(|outcome| outcome.is_ok()).count(), 1);
        assert_eq!(
            outcomes
                .iter()
                .filter(|outcome| matches!(outcome, Err(SaveError::AlreadyExists)))
                .count(),
            1
        );
        let winner = outcomes.iter().position(Result::is_ok).unwrap();
        assert_eq!(
            load(&path, 0).unwrap().snapshot().state(),
            snapshots[winner].state()
        );
        assert_eq!(std::fs::read_dir(directory.path()).unwrap().count(), 1);
    }
}
