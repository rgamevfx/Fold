//! Feature-neutral Project creation and placement contracts. No media I/O.
use fold_foundation::{AssetId, DocumentId, ObjectId, Time};
use fold_project::{DocumentRef, Snapshot};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Entry {
    Bin(fold_foundation::BinId),
    Item(fold_project::ItemId),
}
impl Entry {
    pub fn source(self) -> Result<PlacementSource, String> {
        match self {
            Self::Item(fold_project::ItemId::Asset(id)) => Ok(PlacementSource::Asset(id)),
            Self::Item(fold_project::ItemId::Document(document)) => {
                Ok(PlacementSource::Output(DocumentRef {
                    document,
                    output: "video".into(),
                    extensions: Default::default(),
                }))
            }
            Self::Bin(_) => Err("Place a media item or document, not a bin".into()),
        }
    }
}
#[derive(Clone, Debug)]
pub enum BrowserCommand {
    Import {
        paths: Vec<std::path::PathBuf>,
        destination: fold_project::Parent,
    },
    ChooseImport(fold_project::Parent),
    Relink(AssetId),
    NewBin {
        parent: fold_project::Parent,
        name: String,
    },
    Create {
        kind: String,
        parent: fold_project::Parent,
    },
    Rename {
        entry: Entry,
        name: String,
    },
    Move {
        entry: Entry,
        parent: fold_project::Parent,
    },
    Delete(Entry),
    Place(Placement),
}

#[derive(Clone, Copy, Debug)]
pub struct DocumentKind {
    pub type_id: &'static str,
    pub title: &'static str,
}
#[derive(Clone, Debug)]
pub enum PlacementSource {
    Asset(AssetId),
    Output(DocumentRef),
}
#[derive(Clone, Debug)]
pub struct Placement {
    pub source: PlacementSource,
    pub target: DocumentId,
    pub track: Option<ObjectId>,
    pub at: Time,
    pub source_start: Time,
    /// None uses the remaining source duration.
    pub duration: Option<Time>,
    pub audio_only: bool,
}
impl Placement {
    pub fn range(&self, total: Time) -> Result<Time, String> {
        let duration = self.duration.unwrap_or(
            total
                .checked_sub(self.source_start)
                .map_err(|e| e.to_string())?,
        );
        if self.at < Time::ZERO
            || self.source_start < Time::ZERO
            || duration <= Time::ZERO
            || self
                .source_start
                .checked_add(duration)
                .map_err(|e| e.to_string())?
                > total
        {
            return Err(
                "placement requires a valid source range and nonnegative insertion time".into(),
            );
        }
        self.at.checked_add(duration).map_err(|e| e.to_string())?;
        Ok(duration)
    }
}
pub fn asset_metadata(
    snapshot: &Snapshot,
    id: AssetId,
) -> Result<fold_media::ingest::SourceMetadata, String> {
    let asset = snapshot
        .state()
        .assets
        .get(&id)
        .ok_or("missing source asset")?;
    serde_json::from_value(
        asset
            .extensions
            .get(fold_media::ingest::METADATA_KEY)
            .ok_or("legacy asset needs verified metadata before Project placement")?
            .clone(),
    )
    .map_err(|e| e.to_string())
}
