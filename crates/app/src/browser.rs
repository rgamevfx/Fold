//! Project browser mutations and native file selection; providers retain creative models.
use fold_platform::browser::{BrowserCommand, Entry};
use fold_project::{EditBatch, Item, ItemId, Mutation, Snapshot};

pub fn edit(snapshot: &Snapshot, command: BrowserCommand) -> Result<EditBatch, String> {
    let organization = &snapshot.state().organization;
    let mutations = match command {
        BrowserCommand::NewBin { parent, name } => vec![Mutation::PutBin(fold_project::Bin {
            id: Default::default(),
            name,
            parent,
            order: organization.bins.len() as u64,
            extensions: Default::default(),
        })],
        BrowserCommand::Create { kind, parent } => {
            let registry = crate::packages::builtins();
            let title = registry
                .document_kinds()
                .into_iter()
                .find(|k| k.type_id == kind)
                .ok_or("unavailable document kind")?
                .title;
            let id = Default::default();
            vec![
                Mutation::PutDocument(registry.create_with_defaults(snapshot, &kind, id)?),
                Mutation::PutItem(Item {
                    id: ItemId::Document(id),
                    name: title.into(),
                    parent,
                    order: organization.items.len() as u64,
                    extensions: Default::default(),
                }),
            ]
        }
        BrowserCommand::Rename { entry, name } => match entry {
            Entry::Component(..) => return Err("Manage components in the Gallery".into()),
            Entry::Bin(id) => {
                let mut bin = organization.bins.get(&id).ok_or("missing bin")?.clone();
                bin.name = name;
                vec![Mutation::PutBin(bin)]
            }
            Entry::Item(id) => {
                let mut item = organization
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .ok_or("missing item")?
                    .clone();
                item.name = name;
                vec![Mutation::PutItem(item)]
            }
        },
        BrowserCommand::Move { entry, parent } => match entry {
            Entry::Component(..) => return Err("Manage components in the Gallery".into()),
            Entry::Bin(id) => {
                let mut bin = organization.bins.get(&id).ok_or("missing bin")?.clone();
                bin.parent = parent;
                vec![Mutation::PutBin(bin)]
            }
            Entry::Item(id) => {
                let mut item = organization
                    .items
                    .iter()
                    .find(|i| i.id == id)
                    .ok_or("missing item")?
                    .clone();
                item.parent = parent;
                vec![Mutation::PutItem(item)]
            }
        },
        BrowserCommand::Delete(Entry::Bin(id)) => vec![Mutation::RemoveBin(id)],
        BrowserCommand::Delete(Entry::Item(id)) => {
            return crate::ingest::delete_item(snapshot, id).map_err(|uses| uses.join("\n"));
        }
        BrowserCommand::Place(request) => {
            return place(snapshot, &request);
        }
        _ => return Err("browser operation requires a worker".into()),
    };
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations,
    })
}

#[cfg(feature = "desktop")]
pub fn choose_files(multiple: bool) -> Option<Vec<std::path::PathBuf>> {
    let dialog = rfd::FileDialog::new()
        .set_title(if multiple {
            "Import media"
        } else {
            "Relink media"
        })
        .add_filter("Supported media", fold_media::ingest::SUPPORTED_EXTENSIONS);
    if multiple {
        dialog.pick_files()
    } else {
        dialog.pick_file().map(|p| vec![p])
    }
}

/// Common owning-layer locks also apply to drops into a provider's network.
fn place(
    snapshot: &Snapshot,
    request: &fold_platform::browser::Placement,
) -> Result<EditBatch, String> {
    check_placement_locks(snapshot, request.target)?;
    let mut batch = crate::packages::builtins().place(snapshot, request)?;
    let original = &snapshot.state().documents[&request.target];
    if original.type_id == fold_composition::COMPOSITION {
        let before = fold_composition::Composition::from_document(original)?;
        let mut graphs = vec![];
        for mutation in &mut batch.mutations {
            if let Mutation::PutDocument(document) = mutation
                && document.id == request.target
            {
                let mut composition = fold_composition::Composition::from_document(document)?;
                for layer in &mut composition.layers {
                    if !before.layers.iter().any(|old| old.id == layer.id) {
                        graphs.extend(crate::composition_layers::wrap_footage(
                            snapshot,
                            request.target,
                            layer,
                        )?);
                    }
                }
                let extensions = document.extensions.clone();
                *document = composition.document(request.target)?;
                document.extensions = extensions;
            }
        }
        batch.mutations.extend(graphs);
    }
    Ok(batch)
}

pub(crate) fn check_placement_locks(
    snapshot: &Snapshot,
    target: fold_foundation::DocumentId,
) -> Result<(), String> {
    for doc in snapshot
        .state()
        .documents
        .values()
        .filter(|d| d.type_id == fold_composition::COMPOSITION)
    {
        let composition = fold_composition::Composition::from_document(doc)?;
        for layer in &composition.layers {
            if matches!(&layer.source, Some(fold_platform::image_source::Source::Output(r)) if r.document == target)
                && composition.locked(layer.id)
            {
                return Err("Destination layer is locked".into());
            }
        }
    }
    Ok(())
}
