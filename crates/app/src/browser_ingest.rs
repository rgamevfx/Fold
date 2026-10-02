//! Worker-only Project import/relink. Native chooser never blocks the frame loop.
use fold_media::Cancel;
use fold_platform::browser::BrowserCommand;
use fold_project::Snapshot;

pub fn run(
    snapshot: &Snapshot,
    command: BrowserCommand,
    cancel: &Cancel,
) -> Result<Option<crate::ingest::IngestProposal>, String> {
    let (paths, destination, relink) = match command {
        BrowserCommand::Import { paths, destination } => (paths, destination, None),
        BrowserCommand::ChooseImport(destination) => {
            let Some(paths) = choose(true)? else {
                return Ok(None);
            };
            (paths, destination, None)
        }
        BrowserCommand::Relink(asset) => {
            let Some(paths) = choose(false)? else {
                return Ok(None);
            };
            (paths, Default::default(), Some(asset))
        }
        _ => return Err("not an ingest operation".into()),
    };
    cancel.check()?;
    if let Some(asset) = relink {
        crate::ingest::relink(
            snapshot,
            &crate::ingest::RelinkRequest {
                base: snapshot.revision(),
                asset,
                path: paths[0].clone(),
            },
            cancel,
        )
        .map(Some)
    } else {
        crate::ingest::import(
            snapshot,
            &crate::ingest::ImportRequest {
                base: snapshot.revision(),
                destination,
                paths,
            },
            cancel,
        )
        .map(Some)
    }
}
fn choose(multiple: bool) -> Result<Option<Vec<std::path::PathBuf>>, String> {
    #[cfg(feature = "desktop")]
    {
        Ok(crate::browser::choose_files(multiple))
    }
    #[cfg(not(feature = "desktop"))]
    {
        let _ = multiple;
        Err("native file selection requires desktop support".into())
    }
}
