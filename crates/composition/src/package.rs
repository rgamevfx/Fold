use crate::{Composition, Layer};
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_platform::{
    browser::{Placement, PlacementSource},
    image_source::Source,
    packages::*,
};
use fold_project::{Document, DocumentRef, EditBatch, Mutation, Snapshot};
use serde::{Deserialize, Serialize};

pub const EDIT: &str = "fold.composition.edit";
pub const NEST: &str = "fold.composition.precompose";
pub const PANEL: &str = "fold.composition.editor";
pub const INSPECTOR: &str = "fold.composition.inspector";
pub const PANELS: &[PanelDescriptor] = &[
    PanelDescriptor {
        id: PANEL,
        title: "Composition",
        placement: PanelPlacement::Editor,
    },
    PanelDescriptor {
        id: INSPECTOR,
        title: "Layer properties",
        placement: PanelPlacement::Inspector,
    },
];
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EditArgs {
    pub document: DocumentId,
    pub composition: Composition,
    #[serde(default)]
    pub graph_animation: Vec<GraphAnimationEdit>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphAnimationEdit {
    pub layer: ObjectId,
    /// Curves retain the source graph clock.
    pub channels: Vec<fold_platform::animation::Channel>,
    /// Composition playhead, mapped to source time when removing the last key.
    pub time: fold_foundation::Time,
}
pub fn edit(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: EditArgs = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let mut document = args.composition.document(args.document)?;
    if let Some(old) = snapshot.state().documents.get(&args.document) {
        Composition::from_document(old)?;
        document.extensions = old.extensions.clone();
    }
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![Mutation::PutDocument(document)],
    })
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NestArgs {
    pub document: DocumentId,
    pub child: DocumentId,
    pub name: String,
    pub layers: Vec<ObjectId>,
}
pub fn nest(snapshot: &Snapshot, bytes: &[u8], cancel: &Cancel) -> Result<EditBatch, String> {
    cancel.check()?;
    let args: NestArgs = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    if snapshot.state().documents.contains_key(&args.child) || args.name.trim().is_empty() {
        return Err("Choose a new composition ID and name".into());
    }
    let original = snapshot
        .state()
        .documents
        .get(&args.document)
        .ok_or("Missing composition")?;
    let mut parent = Composition::from_document(original)?;
    let mut child = Composition {
        info: parent.info.clone(),
        ..Default::default()
    };
    let mut selected: std::collections::BTreeSet<_> = args.layers.iter().copied().collect();
    for id in &selected {
        parent.layer(*id)?;
        if parent.locked(*id) {
            return Err("Unlock selected layers before precomposing".into());
        }
    }
    // Selecting a group includes its descendants; a selected descendant is not another root.
    let roots: Vec<_> = parent
        .layers
        .iter()
        .filter(|l| {
            selected.contains(&l.id) && {
                let mut ancestor = l.parent;
                let mut included = false;
                while let Some(id) = ancestor {
                    included |= selected.contains(&id);
                    ancestor = parent.layer(id).unwrap().parent;
                }
                !included
            }
        })
        .map(|l| l.id)
        .collect();
    let group = roots
        .first()
        .and_then(|id| parent.layer(*id).unwrap().parent);
    if roots
        .iter()
        .any(|id| parent.layer(*id).unwrap().parent != group)
    {
        return Err("Precompose adjacent sibling layers or a group".into());
    }
    let siblings: Vec<_> = parent
        .layers
        .iter()
        .filter(|l| l.parent == group)
        .map(|l| l.id)
        .collect();
    if let (Some(first), Some(last)) = (roots.first(), roots.last()) {
        let a = siblings.iter().position(|id| id == first).unwrap();
        let b = siblings.iter().position(|id| id == last).unwrap();
        if b - a + 1 != roots.len() {
            return Err("Precompose adjacent sibling layers".into());
        }
    }
    loop {
        let before = selected.len();
        for l in &parent.layers {
            if l.parent.is_some_and(|p| selected.contains(&p)) {
                selected.insert(l.id);
            }
        }
        if before == selected.len() {
            break;
        }
    }
    if selected.iter().any(|id| parent.locked(*id)) {
        return Err("Unlock descendant layers before precomposing".into());
    }
    let insertion = roots
        .first()
        .and_then(|root| parent.layers.iter().position(|l| l.id == *root))
        .map(|position| {
            parent.layers[..position]
                .iter()
                .filter(|l| !selected.contains(&l.id))
                .count()
        })
        .unwrap_or(0);
    child.layers = parent
        .layers
        .iter()
        .filter(|l| selected.contains(&l.id))
        .cloned()
        .collect();
    for layer in &mut child.layers {
        if roots.contains(&layer.id) {
            layer.parent = None;
        }
    }
    parent.layers.retain(|l| !selected.contains(&l.id));
    let mut layer = Layer::new(
        args.name.clone(),
        Some(Source::Output(DocumentRef {
            document: args.child,
            output: "video".into(),
            extensions: Default::default(),
        })),
        child.info.clone(),
    )?;
    layer.parent = group;
    parent.layers.insert(insertion, layer);
    let mut updated = parent.document(args.document)?;
    updated.extensions = original.extensions.clone();
    let parent_bin = snapshot
        .state()
        .organization
        .items
        .iter()
        .find(|i| i.id == fold_project::ItemId::Document(args.document))
        .map(|i| i.parent)
        .unwrap_or_default();
    Ok(EditBatch {
        base: snapshot.revision(),
        mutations: vec![
            Mutation::PutDocument(child.document(args.child)?),
            Mutation::PutItem(fold_project::Item {
                id: fold_project::ItemId::Document(args.child),
                name: args.name,
                parent: parent_bin,
                order: snapshot.state().organization.items.len() as u64,
                extensions: Default::default(),
            }),
            Mutation::PutDocument(updated),
        ],
    })
}
struct Documents;
impl DocumentProvider for Documents {
    fn package_id(&self) -> &'static str {
        crate::PACKAGE
    }
    fn type_id(&self) -> &'static str {
        crate::COMPOSITION
    }
    fn schema(&self) -> u32 {
        3
    }
    fn supports_schema(&self, schema: u32) -> bool {
        schema == 3
    }
    fn validate(&self, document: &Document) -> Result<(), String> {
        Composition::from_document(document).map(|_| ())
    }
    fn parameters_capability(&self) -> Option<&dyn fold_platform::packages::Parameters> {
        Some(self)
    }
    fn parameter_sources_capability(
        &self,
    ) -> Option<&dyn fold_platform::packages::ParameterSources> {
        Some(self)
    }
    fn settings_capability(&self) -> Option<&dyn fold_platform::packages::Settings> {
        Some(self)
    }
    fn authoring_capability(&self) -> Option<&dyn fold_platform::packages::Authoring> {
        Some(self)
    }
    fn creation_capability(&self) -> Option<&dyn fold_platform::packages::Creation> {
        Some(self)
    }
    fn browser_kind_capability(&self) -> Option<&dyn fold_platform::packages::BrowserKind> {
        Some(self)
    }
    fn placement_capability(&self) -> Option<&dyn fold_platform::packages::Placement> {
        Some(self)
    }
    fn relink_capability(&self) -> Option<&dyn fold_platform::packages::Relink> {
        Some(self)
    }
    fn evaluation_identity_capability(
        &self,
    ) -> Option<&dyn fold_platform::packages::EvaluationIdentity> {
        Some(self)
    }
}
impl fold_platform::packages::Parameters for Documents {
    fn parameter_time(
        &self,
        _document: &Document,
        output_time: fold_foundation::Time,
    ) -> Result<fold_foundation::Time, String> {
        Ok(output_time)
    }

    fn parameters(
        &self,
        document: &Document,
    ) -> Result<Vec<fold_platform::parameters::Descriptor>, String> {
        crate::parameters::catalog(document)
    }
    fn sample_parameter(
        &self,
        document: &Document,
        target: &fold_platform::parameters::Target,
        time: fold_foundation::Time,
        cancel: &fold_media::Cancel,
    ) -> Result<fold_platform::parameters::Literal, String> {
        crate::parameters::sample(document, target, time, cancel)
    }
}
impl fold_platform::packages::ParameterSources for Documents {
    fn parameter_sources(
        &self,
        document: &Document,
    ) -> Result<Vec<fold_platform::parameters::Source>, String> {
        crate::parameters::sources(document)
    }
}
impl fold_platform::packages::Settings for Documents {
    fn settings(
        &self,
        document: &Document,
    ) -> Result<Option<fold_platform::settings::DocumentSettings>, String> {
        Ok(Some(fold_platform::settings::DocumentSettings::fixed(
            Composition::from_document(document)?.info,
        )))
    }
    fn set_settings(
        &self,
        document: &Document,
        settings: &fold_platform::settings::DocumentSettings,
    ) -> Result<Document, String> {
        let mut value = Composition::from_document(document)?;
        value.info = settings.format.clone();
        value.document(document.id)
    }
}
impl fold_platform::packages::Authoring for Documents {
    fn authoring_catalog(&self) -> serde_json::Value {
        crate::automation::catalog()
    }
    fn inspect(&self, document: &Document) -> Result<serde_json::Value, String> {
        serde_json::to_value(Composition::from_document(document)?).map_err(|e| e.to_string())
    }
    fn author(&self, document: &Document, edits: &[serde_json::Value]) -> Result<Document, String> {
        crate::automation::author(document, edits)
    }
}
impl fold_platform::packages::Creation for Documents {
    fn create(&self, id: DocumentId) -> Result<Document, String> {
        Composition::default().document(id)
    }
}
impl fold_platform::packages::BrowserKind for Documents {
    fn browser_kind(&self) -> Option<fold_platform::browser::DocumentKind> {
        Some(fold_platform::browser::DocumentKind {
            type_id: crate::COMPOSITION,
            title: "Composition",
        })
    }
}
impl fold_platform::packages::Placement for Documents {
    fn place(
        &self,
        snapshot: &Snapshot,
        placement: &Placement,
        info: Option<fold_media::VideoInfo>,
    ) -> Result<EditBatch, String> {
        if placement.audio_only {
            return Err("A composition layer requires video or an image".into());
        }
        let original = snapshot
            .state()
            .documents
            .get(&placement.target)
            .ok_or("Missing composition")?;
        let mut composition = Composition::from_document(original)?;
        let info = match &placement.source {
            PlacementSource::Asset(id) => {
                fold_platform::image_source::asset_info(snapshot, *id, &composition.info)?
            }
            PlacementSource::Output(_) => info.ok_or("Missing source video metadata")?,
        };
        let duration = placement.range(info.time(info.frames)?)?;
        let (source, item) = match &placement.source {
            PlacementSource::Asset(id) => (Source::Asset(*id), fold_project::ItemId::Asset(*id)),
            PlacementSource::Output(r) => (
                Source::Output(r.clone()),
                fold_project::ItemId::Document(r.document),
            ),
        };
        let name = fold_platform::browser::item_name(snapshot, item, "Layer");
        let mut layer = Layer::new(name, Some(source), info)?;
        let fit = (f64::from(composition.info.width) / f64::from(layer.info.width))
            .min(f64::from(composition.info.height) / f64::from(layer.info.height))
            .min(1.);
        let width = f64::from(layer.info.width) * fit;
        let height = f64::from(layer.info.height) * fit;
        layer.offset = [
            width / f64::from(composition.info.width),
            0.,
            0.,
            height / f64::from(composition.info.height),
            (f64::from(composition.info.width) - width) / 2.,
            (f64::from(composition.info.height) - height) / 2.,
        ];
        layer.start = placement.at;
        layer.end = placement
            .at
            .checked_add(duration)
            .map_err(|e| e.to_string())?;
        layer.origin = placement.at;
        layer.source_start = placement.source_start;
        let index = if let Some(before) = placement.track {
            if composition.locked(before) {
                return Err("Destination layer is locked".into());
            }
            let index = composition
                .layers
                .iter()
                .position(|l| l.id == before)
                .ok_or("Missing destination layer")?;
            layer.parent = composition.layers[index].parent;
            index
        } else {
            0
        };
        composition.layers.insert(index, layer);
        let mut document = composition.document(placement.target)?;
        document.extensions = original.extensions.clone();
        Ok(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
    }
}
impl fold_platform::packages::Relink for Documents {
    fn validate_relink(
        &self,
        document: &Document,
        asset: fold_foundation::AssetId,
        metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String> {
        for layer in Composition::from_document(document)?.layers {
            if layer.source != Some(Source::Asset(asset)) {
                continue;
            }
            let valid = match &metadata.profile {
                fold_media::ingest::SourceProfile::Video(info) => *info == layer.info,
                fold_media::ingest::SourceProfile::Ppm { dimensions } => {
                    *dimensions == [layer.info.width, layer.info.height]
                }
                fold_media::ingest::SourceProfile::Exr(info) => {
                    info.dimensions == [layer.info.width, layer.info.height]
                        && info
                            .sequence
                            .as_ref()
                            .is_none_or(|s| s.last() - s.first() + 1 == layer.info.frames)
                }
                _ => false,
            };
            if !valid {
                return Err(
                    "Relink changes the composition layer's source dimensions or duration".into(),
                );
            }
        }
        Ok(())
    }
}
impl fold_platform::packages::EvaluationIdentity for Documents {
    fn evaluation_identity(&self, document: &Document) -> Result<Vec<u8>, String> {
        let mut composition = Composition::from_document(document)?;
        for layer in &mut composition.layers {
            layer.name.clear();
            layer.locked = false;
        }
        serde_json::to_vec(&composition).map_err(|e| e.to_string())
    }
}

pub fn register(registry: &mut PackageRegistry) -> Result<(), String> {
    register_with_edit(registry, edit)
}
pub fn register_with_edit(
    registry: &mut PackageRegistry,
    edit: CommandHandler,
) -> Result<(), String> {
    registry.register(Contributions {
        manifest: Manifest {
            id: crate::PACKAGE,
            version: env!("CARGO_PKG_VERSION"),
            host_api: HOST_API,
            dependencies: &[],
            panels: PANELS,
            build: "composition-schema3-evaluator3-effects",
        },
        documents: vec![Box::new(Documents)],
        video: vec![Box::new(crate::compile::Provider)],
        audio: vec![],
        commands: vec![
            CommandRegistration {
                id: EDIT,
                title: "Edit composition",
                execution: Execution::Immediate,
                handler: edit,
            },
            CommandRegistration {
                id: NEST,
                title: "Precompose layers",
                execution: Execution::Immediate,
                handler: nest,
            },
        ],
    })
}
