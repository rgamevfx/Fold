//! Worker-only Project import/relink. Native chooser never blocks the frame loop.
use fold_media::Cancel;
use fold_platform::browser::BrowserCommand;
use fold_project::{Candidate, CommittedSnapshot};

pub fn run(
    snapshot: &CommittedSnapshot,
    command: BrowserCommand,
    cancel: &Cancel,
) -> Result<Option<crate::ingest::IngestProposal>, String> {
    if let BrowserCommand::ImportAndPlace { paths, placement } = command {
        let proposal = crate::ingest::import(
            snapshot,
            &crate::ingest::ImportRequest {
                base: snapshot.revision(),
                destination: Default::default(),
                paths,
            },
            cancel,
        )?;
        let items = proposal.items.clone();
        let mut candidate = Candidate::new(snapshot.clone());
        let batch = proposal.into_batch()?;
        if !batch.mutations.is_empty() {
            candidate = candidate.apply(batch)?;
        }
        let mut placement = placement;
        for item in &items {
            cancel.check()?;
            let fold_project::ItemId::Asset(asset) = item else {
                continue;
            };
            placement.source = fold_platform::browser::PlacementSource::Asset(*asset);
            let before_clips: std::collections::BTreeSet<_> = candidate
                .snapshot()
                .state()
                .documents
                .get(&placement.target)
                .filter(|d| d.type_id == fold_timeline::SEQUENCE)
                .map(|d| fold_timeline::Sequence::from_document(d))
                .transpose()?
                .into_iter()
                .flat_map(|s| s.clips.into_iter().map(|c| c.id))
                .collect();
            let batch = crate::browser::edit(
                candidate.snapshot(),
                BrowserCommand::Place(placement.clone()),
            )?;
            candidate = candidate.apply(batch)?;
            if let Some(graph) = &mut placement.graph {
                graph.position[0] += 240.;
            }
            if let Some(position) = &mut placement.canvas_position {
                position[0] += 24.;
                position[1] += 24.;
            }
            // Batch timeline drops append each source, preserving its exact duration.
            if candidate
                .snapshot()
                .state()
                .documents
                .get(&placement.target)
                .is_some_and(|d| d.type_id == fold_timeline::SEQUENCE)
            {
                let sequence = fold_timeline::Sequence::from_document(
                    &candidate.snapshot().state().documents[&placement.target],
                )?;
                placement.at = sequence
                    .clips
                    .iter()
                    .filter(|c| !before_clips.contains(&c.id))
                    .filter_map(|c| c.start.checked_add(c.duration).ok())
                    .max()
                    .unwrap_or(placement.at);
                if placement.new_track {
                    placement.track = sequence
                        .clips
                        .iter()
                        .find(|c| {
                            !before_clips.contains(&c.id)
                                && sequence.track(c.track).is_ok_and(|t| {
                                    (t.kind == fold_timeline::TrackKind::Audio)
                                        == placement.audio_only
                                })
                        })
                        .map(|c| c.track);
                    placement.new_track = placement.track.is_none();
                }
            }
        }
        let objects = crate::packages::builtins().placed_objects(
            snapshot
                .state()
                .documents
                .get(&placement.target)
                .ok_or("Missing placement target")?,
            candidate
                .snapshot()
                .state()
                .documents
                .get(&placement.target)
                .ok_or("Missing placed document")?,
        )?;
        let mut result =
            crate::ingest::IngestProposal::new(candidate.proposal(), cancel.clone(), items);
        if !objects.is_empty() {
            result.placement_selection = Some(fold_platform::desktop::Selection {
                document: Some(placement.target),
                objects,
            });
        }
        return Ok(Some(result));
    }
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
