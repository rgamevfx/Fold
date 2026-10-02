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
                Mutation::PutDocument(registry.create(&kind, id)?),
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
            return crate::packages::builtins().place(snapshot, &request);
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
        .add_filter("Supported media", &["mp4", "wav", "ppm"]);
    if multiple {
        dialog.pick_files()
    } else {
        dialog.pick_file().map(|p| vec![p])
    }
}
