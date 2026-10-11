//! Versioned in-process contribution contracts. These Rust traits/function
//! pointers are deliberately NOT a durable dynamic-library ABI.
use crate::proposals::FailureKind;
mod parameter_service;
use crate::{VideoProvider, VideoRegistry};
use fold_foundation::{DocumentId, Time};
use fold_media::{Cancel, VideoInfo};
use fold_project::{Document, DocumentRef, EditBatch, Mutation, Revision, Snapshot};
use fold_render::{RenderGraph, audio::AudioPlan};
use std::collections::{BTreeMap, BTreeSet};

pub const HOST_API: u32 = 1;
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PanelPlacement {
    Browser,
    Editor,
    Inspector,
}
#[derive(Clone, Copy, Debug)]
pub struct PanelDescriptor {
    pub id: &'static str,
    pub title: &'static str,
    pub placement: PanelPlacement,
}
#[derive(Clone, Debug)]
pub struct Manifest {
    pub id: &'static str,
    pub version: &'static str,
    pub host_api: u32,
    pub dependencies: &'static [&'static str],
    pub panels: &'static [PanelDescriptor],
    pub build: &'static str,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Execution {
    Immediate,
    Worker,
}
#[derive(Clone, Debug)]
pub struct CommandRequest {
    pub id: String,
    pub base: Revision,
    /// Package-defined, versioned arguments; host neither interprets nor owns them.
    pub arguments: Vec<u8>,
}
pub type CommandHandler = fn(&Snapshot, &[u8], &Cancel) -> Result<EditBatch, String>;
pub struct CommandRegistration {
    pub id: &'static str,
    pub title: &'static str,
    pub execution: Execution,
    pub handler: CommandHandler,
}
/// Required identity, schema support and validation only. Optional contracts are
/// discovered from actual trait references; absence never means an empty success.
/// Packages are assembled statically; this is not a plugin ABI.
pub trait DocumentProvider: Send + Sync {
    fn package_id(&self) -> &'static str;
    fn type_id(&self) -> &'static str;
    fn schema(&self) -> u32;
    fn supports_schema(&self, schema: u32) -> bool {
        schema == self.schema()
    }
    fn validate(&self, document: &Document) -> Result<(), String>;
    fn observations_capability(&self) -> Option<&dyn crate::observations::Observations> {
        None
    }
    fn scoped_proposals_capability(&self) -> Option<&dyn crate::proposals::ScopedProposals> {
        None
    }
    fn parameters_capability(&self) -> Option<&dyn Parameters> {
        None
    }
    fn parameter_sources_capability(&self) -> Option<&dyn ParameterSources> {
        None
    }
    fn animation_capability(&self) -> Option<&dyn Animation> {
        None
    }
    fn hierarchy_capability(&self) -> Option<&dyn Hierarchy> {
        None
    }
    fn artist_unlock_capability(&self) -> Option<&dyn ArtistUnlock> {
        None
    }
    fn settings_capability(&self) -> Option<&dyn Settings> {
        None
    }
    fn image_inputs_capability(&self) -> Option<&dyn ImageInputs> {
        None
    }
    fn authoring_capability(&self) -> Option<&dyn Authoring> {
        None
    }
    fn query_capability(&self) -> Option<&dyn Query> {
        None
    }
    fn authoring_receipt_capability(&self) -> Option<&dyn AuthoringReceipt> {
        None
    }
    fn creation_capability(&self) -> Option<&dyn Creation> {
        None
    }
    fn browser_kind_capability(&self) -> Option<&dyn BrowserKind> {
        None
    }
    fn placement_capability(&self) -> Option<&dyn Placement> {
        None
    }
    fn relink_capability(&self) -> Option<&dyn Relink> {
        None
    }
    fn evaluation_identity_capability(&self) -> Option<&dyn EvaluationIdentity> {
        None
    }
}
pub trait Parameters: Send + Sync {
    fn sample_parameter_with_images(
        &self,
        document: &Document,
        target: &crate::parameters::Target,
        time: Time,
        cancel: &Cancel,
        _images: &dyn crate::image_sampling::Resolver,
    ) -> Result<fold_graph::types::Literal, String> {
        self.sample_parameter(document, target, time, cancel)
    }

    fn parameters(
        &self,
        _document: &Document,
    ) -> Result<Vec<crate::parameters::Descriptor>, String>;
    fn sample_parameter(
        &self,
        _document: &Document,
        _target: &crate::parameters::Target,
        _time: Time,
        _cancel: &Cancel,
    ) -> Result<fold_graph::types::Literal, String>;
    /// Map placed output-relative time to authored document time exactly once.
    fn parameter_time(&self, document: &Document, output_time: Time) -> Result<Time, String>;
}
pub trait ParameterSources: Send + Sync {
    fn parameter_sources(
        &self,
        _document: &Document,
    ) -> Result<Vec<crate::parameters::Source>, String>;
}
pub trait Animation: Send + Sync {
    fn animation_channels(
        &self,
        _document: &Document,
        _object: Option<fold_foundation::ObjectId>,
    ) -> Result<Vec<crate::animation::Channel>, String>;
    fn edit_animation(
        &self,
        _document: &Document,
        _channels: &[crate::animation::Channel],
        _time: Time,
    ) -> Result<Document, String>;
}
pub trait Hierarchy: Send + Sync {
    fn hierarchy(&self, _document: &Document) -> Result<crate::hierarchy::Hierarchy, String>;
    fn edit_hierarchy(
        &self,
        _document: &Document,
        _edits: &[crate::hierarchy::Edit],
    ) -> Result<Document, String>;
}
/// Trusted local artist action, separate from agent authoring. Implementations
/// may clear only the requested locks, never change the protected content.
pub trait ArtistUnlock: Send + Sync {
    fn unlock_for_artist(
        &self,
        _document: &Document,
        _object: Option<fold_foundation::ObjectId>,
        _targets: &[fold_foundation::ObjectId],
    ) -> Result<Document, String>;
}
pub trait Settings: Send + Sync {
    fn settings(
        &self,
        _document: &Document,
    ) -> Result<Option<crate::settings::DocumentSettings>, String>;
    fn set_settings(
        &self,
        _document: &Document,
        _settings: &crate::settings::DocumentSettings,
    ) -> Result<Document, String>;
}
pub trait ImageInputs: Send + Sync {
    fn image_inputs(
        &self,
        _document: &Document,
    ) -> Result<Vec<crate::layer_graph::ImageInput>, String>;
    fn bind_image_input(
        &self,
        _snapshot: &Snapshot,
        _document: &Document,
        _port: &str,
        _source: Option<crate::image_source::Source>,
        _info: VideoInfo,
    ) -> Result<Document, String>;
}
pub trait Authoring: Send + Sync {
    fn authoring_catalog(&self) -> serde_json::Value;
    fn inspect(&self, _document: &Document) -> Result<serde_json::Value, String>;
    fn author(
        &self,
        _document: &Document,
        _edits: &[serde_json::Value],
    ) -> Result<Document, String>;
    fn author_cancellable(
        &self,
        document: &Document,
        edits: &[serde_json::Value],
        cancel: &Cancel,
    ) -> Result<Document, String> {
        cancel.check()?;
        self.author(document, edits)
    }
}
pub trait Query: Send + Sync {
    fn query(
        &self,
        _document: &Document,
        _query: &serde_json::Value,
    ) -> Result<serde_json::Value, String>;
    fn query_cancellable(
        &self,
        document: &Document,
        query: &serde_json::Value,
        cancel: &Cancel,
    ) -> Result<serde_json::Value, String> {
        cancel.check()?;
        self.query(document, query)
    }
}
pub trait AuthoringReceipt: Send + Sync {
    fn authoring_receipt(
        &self,
        _before: &Document,
        _after: &Document,
    ) -> Result<serde_json::Value, String>;
}
pub trait Creation: Send + Sync {
    fn create(&self, _id: DocumentId) -> Result<Document, String>;
}
pub trait BrowserKind: Send + Sync {
    fn browser_kind(&self) -> Option<crate::browser::DocumentKind>;
}
pub trait Placement: Send + Sync {
    fn place(
        &self,
        _snapshot: &Snapshot,
        _placement: &crate::browser::Placement,
        _info: Option<VideoInfo>,
    ) -> Result<EditBatch, String>;
}
pub trait Relink: Send + Sync {
    fn validate_relink(
        &self,
        _document: &Document,
        _asset: fold_foundation::AssetId,
        _metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String>;
}
/// Optional semantic identity excluding provider-owned presentation data.
/// Without this specialization the registry validates and uses opaque payload bytes.
pub trait EvaluationIdentity: Send + Sync {
    fn evaluation_identity(&self, document: &Document) -> Result<Vec<u8>, String>;
}
pub trait AudioProvider: Send + Sync {
    fn package_id(&self) -> &'static str;
    fn type_id(&self) -> &'static str;
    fn compile(&self, snapshot: &Snapshot, document: &Document) -> Result<AudioPlan, String>;
}
pub struct Contributions {
    pub manifest: Manifest,
    pub documents: Vec<Box<dyn DocumentProvider>>,
    pub commands: Vec<CommandRegistration>,
    pub video: Vec<Box<dyn VideoProvider>>,
    pub audio: Vec<Box<dyn AudioProvider>>,
}
#[derive(Default)]
pub struct PackageRegistry {
    manifests: BTreeMap<&'static str, Manifest>,
    documents: Vec<Box<dyn DocumentProvider>>,
    commands: BTreeMap<&'static str, CommandRegistration>,
    video: VideoRegistry,
    audio: Vec<Box<dyn AudioProvider>>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Capability {
    Observations,
    ScopedProposals,
    Parameters,
    ParameterSources,
    Animation,
    Hierarchy,
    ArtistUnlock,
    Settings,
    ImageInputs,
    Authoring,
    Query,
    AuthoringReceipt,
    Creation,
    BrowserKind,
    Placement,
    Relink,
    EvaluationIdentity,
}
impl PackageRegistry {
    pub fn observations(
        &self,
        document: &Document,
    ) -> Result<&dyn crate::observations::Observations, crate::proposals::Failure> {
        self.documents
            .iter()
            .find(|p| {
                p.package_id() == document.package_id
                    && p.type_id() == document.type_id
                    && p.supports_schema(document.schema_version)
            })
            .and_then(|p| p.observations_capability())
            .ok_or_else(|| {
                crate::proposals::Failure::new(
                    FailureKind::Unsupported,
                    "Document measurements unavailable",
                )
            })
    }
    pub fn parameter_provider_types(&self) -> Vec<&str> {
        self.documents
            .iter()
            .filter(|p| p.parameters_capability().is_some())
            .map(|p| p.type_id())
            .collect()
    }
    pub fn scoped_proposals(
        &self,
        document: &Document,
    ) -> Result<&dyn crate::proposals::ScopedProposals, crate::proposals::Failure> {
        self.documents
            .iter()
            .find(|p| {
                p.package_id() == document.package_id
                    && p.type_id() == document.type_id
                    && p.supports_schema(document.schema_version)
            })
            .and_then(|p| p.scoped_proposals_capability())
            .ok_or_else(|| {
                crate::proposals::Failure::new(
                    FailureKind::Unsupported,
                    "Scoped proposals unavailable for document",
                )
            })
    }
    pub fn proposal_authoring_catalog(&self) -> Vec<serde_json::Value> {
        self.documents
            .iter()
            .filter(|p| p.scoped_proposals_capability().is_some())
            .filter_map(|p| {
                p.authoring_capability().map(
                    |c| serde_json::json!({"type":p.type_id(),"catalog":c.authoring_catalog()}),
                )
            })
            .collect()
    }
    pub fn proposal_catalog(&self) -> Vec<serde_json::Value> {
        self.documents.iter().filter_map(|p| p.scoped_proposals_capability().map(|c| serde_json::json!({"type":p.type_id(),"schema":p.schema(),"requests":c.schema()}))).collect()
    }
    pub fn has_capability(&self, document: &Document, capability: Capability) -> bool {
        self.documents
            .iter()
            .find(|p| {
                p.package_id() == document.package_id
                    && p.type_id() == document.type_id
                    && p.supports_schema(document.schema_version)
            })
            .is_some_and(|p| match capability {
                Capability::Observations => p.observations_capability().is_some(),
                Capability::ScopedProposals => p.scoped_proposals_capability().is_some(),
                Capability::Parameters => p.parameters_capability().is_some(),
                Capability::ParameterSources => p.parameter_sources_capability().is_some(),
                Capability::Animation => p.animation_capability().is_some(),
                Capability::Hierarchy => p.hierarchy_capability().is_some(),
                Capability::ArtistUnlock => p.artist_unlock_capability().is_some(),
                Capability::Settings => p.settings_capability().is_some(),
                Capability::ImageInputs => p.image_inputs_capability().is_some(),
                Capability::Authoring => p.authoring_capability().is_some(),
                Capability::Query => p.query_capability().is_some(),
                Capability::AuthoringReceipt => p.authoring_receipt_capability().is_some(),
                Capability::Creation => p.creation_capability().is_some(),
                Capability::BrowserKind => p.browser_kind_capability().is_some(),
                Capability::Placement => p.placement_capability().is_some(),
                Capability::Relink => p.relink_capability().is_some(),
                Capability::EvaluationIdentity => p.evaluation_identity_capability().is_some(),
            })
    }

    pub fn hierarchy(&self, document: &Document) -> Result<crate::hierarchy::Hierarchy, String> {
        self.documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("Unavailable hierarchy provider")?
            .hierarchy_capability()
            .ok_or("Unsupported Hierarchy capability")?
            .hierarchy(document)
    }
    pub fn edit_hierarchy(
        &self,
        document: &Document,
        edits: &[crate::hierarchy::Edit],
    ) -> Result<Document, String> {
        let provider = self
            .documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("Unavailable hierarchy provider")?;
        let result = provider
            .hierarchy_capability()
            .ok_or("Unsupported Hierarchy capability")?
            .edit_hierarchy(document, edits)?;
        if result.id != document.id
            || result.package_id != document.package_id
            || result.type_id != document.type_id
            || result.schema_version != document.schema_version
        {
            return Err("Hierarchy edit changed document identity".into());
        }
        provider.validate(&result)?;
        Ok(result)
    }

    pub fn animation_channels(
        &self,
        document: &Document,
        object: Option<fold_foundation::ObjectId>,
    ) -> Result<Vec<crate::animation::Channel>, String> {
        self.documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("Unavailable animation provider")?
            .animation_capability()
            .ok_or("Unsupported Animation capability")?
            .animation_channels(document, object)
    }
    pub fn edit_animation(
        &self,
        document: &Document,
        channels: &[crate::animation::Channel],
        time: Time,
    ) -> Result<Document, String> {
        let provider = self
            .documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("Unavailable animation provider")?;
        let result = provider
            .animation_capability()
            .ok_or("Unsupported Animation capability")?
            .edit_animation(document, channels, time)?;
        if result.id != document.id
            || result.type_id != document.type_id
            || result.package_id != document.package_id
        {
            return Err("Animation edit changed document identity".into());
        }
        provider.validate(&result)?;
        Ok(result)
    }

    pub fn image_inputs(
        &self,
        snapshot: &Snapshot,
        id: DocumentId,
    ) -> Result<Vec<crate::layer_graph::ImageInput>, String> {
        let document = snapshot.state().documents.get(&id).ok_or("Missing graph")?;
        self.document_image_inputs(document)
    }
    /// Inspect provider-owned ports for newly created, uncommitted graphs too.
    pub fn document_image_inputs(
        &self,
        document: &Document,
    ) -> Result<Vec<crate::layer_graph::ImageInput>, String> {
        self.documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("Unavailable graph provider")?
            .image_inputs_capability()
            .ok_or("Unsupported ImageInputs capability")?
            .image_inputs(document)
    }
    pub fn bind_image_input(
        &self,
        snapshot: &Snapshot,
        id: DocumentId,
        port: &str,
        source: Option<crate::image_source::Source>,
        info: VideoInfo,
    ) -> Result<Document, String> {
        let original = snapshot.state().documents.get(&id).ok_or("Missing graph")?;
        let provider = self
            .documents
            .iter()
            .find(|p| p.package_id() == original.package_id && p.type_id() == original.type_id)
            .ok_or("Unavailable graph provider")?;
        if !provider
            .image_inputs_capability()
            .ok_or("Unsupported ImageInputs capability")?
            .image_inputs(original)?
            .iter()
            .any(|input| input.id == port)
        {
            return Err("Unavailable graph image input".into());
        }
        let document = provider
            .image_inputs_capability()
            .ok_or("Unsupported ImageInputs capability")?
            .bind_image_input(snapshot, original, port, source, info)?;
        if document.id != original.id
            || document.package_id != original.package_id
            || document.type_id != original.type_id
        {
            return Err("Binding changed the graph identity".into());
        }
        provider.validate(&document)?;
        Ok(document)
    }

    /// Validate the complete contribution set before publishing any of it.
    pub fn register(&mut self, package: Contributions) -> Result<(), String> {
        let manifest = &package.manifest;
        if manifest.id.is_empty()
            || manifest.version.is_empty()
            || manifest.build.is_empty()
            || manifest.host_api != HOST_API
            || self.manifests.contains_key(manifest.id)
            || manifest
                .dependencies
                .iter()
                .any(|id| !self.manifests.contains_key(id))
        {
            return Err("incompatible/duplicate package or missing dependency".into());
        }
        let owned = |id: &str| id.starts_with(&format!("{}.", manifest.id));
        let mut ids = BTreeSet::new();
        for command in &package.commands {
            if !owned(command.id)
                || !ids.insert(command.id)
                || self.commands.contains_key(command.id)
            {
                return Err("duplicate or foreign command contribution".into());
            }
        }
        ids.clear();
        for panel in manifest.panels {
            if !owned(panel.id)
                || !ids.insert(panel.id)
                || self
                    .manifests
                    .values()
                    .flat_map(|m| m.panels)
                    .any(|p| p.id == panel.id)
            {
                return Err("duplicate or foreign panel contribution".into());
            }
        }
        ids.clear();
        for provider in &package.documents {
            if provider.package_id() != manifest.id
                || provider.type_id().is_empty()
                || self
                    .documents
                    .iter()
                    .any(|existing| existing.type_id() == provider.type_id())
                || !ids.insert(provider.type_id())
            {
                return Err("duplicate or foreign document contribution".into());
            }
        }
        let mut video = BTreeSet::new();
        for provider in &package.video {
            if provider.package_id() != manifest.id
                || !ids.contains(provider.type_id())
                || !video.insert(provider.type_id())
            {
                return Err("video contribution requires one owned document provider".into());
            }
        }
        let mut audio = BTreeSet::new();
        for provider in &package.audio {
            if provider.package_id() != manifest.id
                || !ids.contains(provider.type_id())
                || !audio.insert(provider.type_id())
            {
                return Err("audio contribution requires one owned document provider".into());
            }
        }
        // New package IDs and locally unique types guarantee registration cannot fail.
        for provider in package.video {
            self.video.register_boxed(provider)?;
        }
        self.documents.extend(package.documents);
        self.audio.extend(package.audio);
        self.commands
            .extend(package.commands.into_iter().map(|c| (c.id, c)));
        self.manifests.insert(package.manifest.id, package.manifest);
        Ok(())
    }
    pub fn document_settings(
        &self,
        document: &Document,
    ) -> Result<Option<crate::settings::DocumentSettings>, String> {
        self.documents
            .iter()
            .find(|p| p.type_id() == document.type_id && p.package_id() == document.package_id)
            .ok_or("Unavailable document provider")?
            .settings_capability()
            .ok_or("Unsupported Settings capability")?
            .settings(document)
    }
    pub fn set_document_settings(
        &self,
        original: &Document,
        settings: &crate::settings::DocumentSettings,
    ) -> Result<Document, String> {
        settings.format.validate()?;
        let provider = self
            .documents
            .iter()
            .find(|p| p.type_id() == original.type_id && p.package_id() == original.package_id)
            .ok_or("Unavailable document provider")?;
        let mut document = provider
            .settings_capability()
            .ok_or("Unsupported Settings capability")?
            .set_settings(original, settings)?;
        if document.id != original.id
            || document.type_id != original.type_id
            || document.package_id != original.package_id
        {
            return Err("Settings changed document identity".into());
        }
        document.extensions = original.extensions.clone();
        provider.validate(&document)?;
        Ok(document)
    }
    pub fn create_with_defaults(
        &self,
        snapshot: &Snapshot,
        kind: &str,
        id: DocumentId,
    ) -> Result<Document, String> {
        let document = self.create(kind, id)?;
        if !self.has_capability(&document, Capability::Settings) {
            return Ok(document);
        }
        let Some(mut settings) = self.document_settings(&document)? else {
            return Ok(document);
        };
        settings.format = crate::settings::ProjectDefaults::read(snapshot)?.format;
        self.set_document_settings(&document, &settings)
    }
    pub fn document_kinds(&self) -> Vec<crate::browser::DocumentKind> {
        self.documents
            .iter()
            .filter_map(|p| p.browser_kind_capability().and_then(|c| c.browser_kind()))
            .collect()
    }
    pub fn create(&self, kind: &str, id: DocumentId) -> Result<Document, String> {
        let provider = self
            .documents
            .iter()
            .find(|p| p.type_id() == kind)
            .ok_or("unavailable document provider")?;
        let document = provider
            .creation_capability()
            .ok_or("Unsupported Creation capability")?
            .create(id)?;
        if document.id != id
            || document.type_id != provider.type_id()
            || document.package_id != provider.package_id()
        {
            return Err("creation changed the requested document identity or provider".into());
        }
        provider.validate(&document)?;
        Ok(document)
    }
    pub fn place(
        &self,
        snapshot: &Snapshot,
        placement: &crate::browser::Placement,
    ) -> Result<EditBatch, String> {
        use crate::browser::PlacementSource;
        let target = snapshot
            .state()
            .documents
            .get(&placement.target)
            .ok_or("missing placement target")?;
        let provider = self
            .documents
            .iter()
            .find(|p| p.type_id() == target.type_id && p.package_id() == target.package_id)
            .ok_or("unavailable destination provider")?;
        let info = match &placement.source {
            PlacementSource::Asset(id) => {
                if !snapshot.state().assets.contains_key(id) {
                    return Err("missing source asset".into());
                }
                None
            }
            PlacementSource::Output(source) => {
                if source.output != "video" {
                    return Err("unsupported document output".into());
                }
                // Reuse evaluation's capability/dependency validation. Seeding
                // the path with the destination also rejects new placement cycles.
                self.validate_video_dependencies(
                    snapshot,
                    source,
                    &mut vec![placement.target],
                    &mut BTreeSet::new(),
                    &Cancel::default(),
                )
                .map_err(|e| format!("invalid placement dependency/cycle: {e}"))?;
                Some(self.output(snapshot, source.document)?)
            }
        };
        let batch = provider
            .placement_capability()
            .ok_or("Unsupported Placement capability")?
            .place(snapshot, placement, info)?;
        if batch.base != snapshot.revision()
            || !batch
                .mutations
                .iter()
                .any(|m| matches!(m, Mutation::PutDocument(d) if d.id == placement.target && d.type_id == target.type_id && d.package_id == target.package_id))
        {
            return Err("placement changed destination identity or revision".into());
        }
        for mutation in &batch.mutations {
            if let Mutation::PutDocument(document) = mutation {
                self.validate_document(document)?;
            }
        }
        Ok(batch)
    }
    pub fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.manifests.values()
    }
    pub fn authoring_catalog(&self) -> Vec<serde_json::Value> {
        self.documents
            .iter()
            .filter_map(|p| {
                let catalog = p.authoring_capability()?.authoring_catalog();
                (!catalog.is_null())
                    .then(|| serde_json::json!({"type":p.type_id(),"catalog":catalog}))
            })
            .collect()
    }
    pub fn inspect(&self, document: &Document) -> Result<serde_json::Value, String> {
        self.documents
            .iter()
            .find(|p| p.type_id() == document.type_id && p.package_id() == document.package_id)
            .ok_or("missing provider")?
            .authoring_capability()
            .ok_or("Unsupported Authoring capability")?
            .inspect(document)
    }
    pub fn query(
        &self,
        document: &Document,
        query: &serde_json::Value,
    ) -> Result<serde_json::Value, String> {
        self.query_cancellable(document, query, &Cancel::default())
    }
    pub fn query_cancellable(
        &self,
        document: &Document,
        query: &serde_json::Value,
        cancel: &Cancel,
    ) -> Result<serde_json::Value, String> {
        self.documents
            .iter()
            .find(|p| p.type_id() == document.type_id && p.package_id() == document.package_id)
            .ok_or("missing provider")?
            .query_capability()
            .ok_or("Unsupported Query capability")?
            .query_cancellable(document, query, cancel)
    }
    pub fn author(
        &self,
        document: &Document,
        edits: &[serde_json::Value],
    ) -> Result<Document, String> {
        let provider = self
            .documents
            .iter()
            .find(|p| p.type_id() == document.type_id && p.package_id() == document.package_id)
            .ok_or("missing provider")?;
        let result = provider
            .authoring_capability()
            .ok_or("Unsupported Authoring capability")?
            .author(document, edits)?;
        provider.validate(&result)?;
        Ok(result)
    }
    pub fn author_cancellable(
        &self,
        document: &Document,
        edits: &[serde_json::Value],
        cancel: &Cancel,
    ) -> Result<(Document, serde_json::Value), String> {
        let provider = self
            .documents
            .iter()
            .find(|p| p.type_id() == document.type_id && p.package_id() == document.package_id)
            .ok_or("missing provider")?;
        let result = provider
            .authoring_capability()
            .ok_or("Unsupported Authoring capability")?
            .author_cancellable(document, edits, cancel)?;
        provider.validate(&result)?;
        let receipt = match provider.authoring_receipt_capability() {
            Some(capability) => capability.authoring_receipt(document, &result)?,
            None => serde_json::Value::Null,
        };
        Ok((result, receipt))
    }
    pub fn authoring_receipt(
        &self,
        before: &Document,
        after: &Document,
    ) -> Result<serde_json::Value, String> {
        self.documents
            .iter()
            .find(|p| p.type_id() == after.type_id && p.package_id() == after.package_id)
            .ok_or("missing provider")?
            .authoring_receipt_capability()
            .ok_or("Unsupported AuthoringReceipt capability")?
            .authoring_receipt(before, after)
    }
    /// Called only by the native desktop unlock action, not ordinary command dispatch.
    pub fn unlock_for_artist(
        &self,
        snapshot: &Snapshot,
        base: Revision,
        document: DocumentId,
        object: Option<fold_foundation::ObjectId>,
        targets: &[fold_foundation::ObjectId],
    ) -> Result<EditBatch, String> {
        if snapshot.revision() != base {
            return Err("stale unlock revision".into());
        }
        if targets.is_empty() || targets.len() > 128 {
            return Err("Select 1..128 controls or groups to unlock".into());
        }
        let original = snapshot
            .state()
            .documents
            .get(&document)
            .ok_or("missing document")?;
        let provider = self
            .documents
            .iter()
            .find(|p| {
                p.package_id() == original.package_id
                    && p.type_id() == original.type_id
                    && p.supports_schema(original.schema_version)
            })
            .ok_or("missing document provider")?;
        let after = provider
            .artist_unlock_capability()
            .ok_or("Unsupported ArtistUnlock capability")?
            .unlock_for_artist(original, object, targets)?;
        if after.id != original.id
            || after.package_id != original.package_id
            || after.type_id != original.type_id
            || after.schema_version != original.schema_version
        {
            return Err("unlock changed document identity".into());
        }
        provider.validate(&after)?;
        Ok(EditBatch {
            base,
            mutations: vec![Mutation::PutDocument(after)],
        })
    }
    pub fn commands(&self) -> impl Iterator<Item = &CommandRegistration> {
        self.commands.values()
    }
    pub fn command(&self, id: &str) -> Result<&CommandRegistration, String> {
        self.commands
            .get(id)
            .ok_or_else(|| format!("unavailable command: {id}"))
    }
    pub fn stage(
        &self,
        snapshot: &Snapshot,
        request: &CommandRequest,
        cancel: &Cancel,
    ) -> Result<EditBatch, String> {
        cancel.check()?;
        if request.base != snapshot.revision() {
            return Err("stale command revision".into());
        }
        if request.arguments.len() > 1024 * 1024 {
            return Err("command arguments exceed 1 MiB".into());
        }
        let batch = (self.command(&request.id)?.handler)(snapshot, &request.arguments, cancel)?;
        cancel.check()?;
        if batch.base != request.base {
            return Err("command returned a mismatched revision".into());
        }
        for mutation in &batch.mutations {
            if let Mutation::PutDocument(document) = mutation {
                let provider = self
                    .documents
                    .iter()
                    .find(|p| {
                        p.package_id() == document.package_id && p.type_id() == document.type_id
                    })
                    .ok_or("command returned an unregistered document type")?;
                provider.validate(document)?;
            }
        }
        Ok(batch)
    }
    pub fn validate_document(&self, document: &Document) -> Result<(), String> {
        self.documents
            .iter()
            .find(|p| {
                p.package_id() == document.package_id
                    && p.type_id() == document.type_id
                    && p.supports_schema(document.schema_version)
            })
            .ok_or("missing document provider")?
            .validate(document)
    }
    pub fn validate_relink(
        &self,
        document: &Document,
        asset: fold_foundation::AssetId,
        metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String> {
        let provider = self
            .documents
            .iter()
            .find(|p| {
                p.package_id() == document.package_id
                    && p.type_id() == document.type_id
                    && p.supports_schema(document.schema_version)
            })
            .ok_or("missing relink document provider")?;
        provider
            .relink_capability()
            .ok_or("Unsupported Relink capability")?
            .validate_relink(document, asset, metadata)
    }
    pub fn supports(&self, document: &Document) -> bool {
        self.documents.iter().any(|p| {
            p.package_id() == document.package_id
                && p.type_id() == document.type_id
                && p.supports_schema(document.schema_version)
        })
    }
    pub fn effect_interface(
        &self,
        snapshot: &Snapshot,
        id: DocumentId,
    ) -> Result<crate::effects::Interface, String> {
        self.video.effect_interface(
            snapshot
                .state()
                .documents
                .get(&id)
                .ok_or("Missing effect definition")?,
        )
    }
    pub fn outputs(
        &self,
        snapshot: &Snapshot,
        id: DocumentId,
    ) -> Vec<crate::workspace::OutputDescriptor> {
        let Some(document) = snapshot.state().documents.get(&id) else {
            return vec![];
        };
        self.document_outputs(document)
    }
    /// Resolve outputs before an atomic create-and-place transaction is committed.
    pub fn document_outputs(&self, document: &Document) -> Vec<crate::workspace::OutputDescriptor> {
        let mut outputs = self.video.outputs(document);
        let validation = self
            .documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or_else(|| "Unavailable document provider".to_owned())
            .and_then(|p| p.validate(document));
        if let Err(error) = validation {
            for output in &mut outputs {
                output.info = Err(error.clone());
            }
        }
        outputs
    }
    /// The provider's first declared output is its default. Preserve its exact
    /// port identity; never synthesize a conventional name or skip an invalid
    /// default in favor of another output.
    pub fn default_output(
        &self,
        snapshot: &Snapshot,
        id: DocumentId,
    ) -> Result<DocumentRef, String> {
        let output = self
            .outputs(snapshot, id)
            .into_iter()
            .next()
            .ok_or("Document has no video outputs")?;
        output.info?;
        Ok(output.reference)
    }
    pub fn output_ref(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
    ) -> Result<VideoInfo, String> {
        if !snapshot.state().documents.contains_key(&source.document) {
            return Err("missing document".into());
        }
        let outputs = self.outputs(snapshot, source.document);
        let available = outputs
            .iter()
            .map(|o| o.reference.output.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        outputs
            .into_iter()
            .find(|o| o.reference.output == source.output)
            .ok_or_else(|| {
                format!(
                    "missing output port {:?}; available output ports: {}",
                    source.output,
                    if available.is_empty() {
                        "none"
                    } else {
                        &available
                    }
                )
            })?
            .info
    }
    /// Registered rendering capability, independent of document editing support.
    pub fn has_video(&self, document: &Document) -> bool {
        self.video
            .providers
            .iter()
            .any(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
    }
    pub fn output(&self, snapshot: &Snapshot, id: DocumentId) -> Result<VideoInfo, String> {
        let document = snapshot
            .state()
            .documents
            .get(&id)
            .ok_or("missing document")?;
        let provider = self
            .documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("missing document provider")?;
        provider.validate(document)?;
        self.video
            .providers
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("document has no video capability")?
            .video_info(document)
    }
    pub fn evaluation_identity(&self, document: &Document) -> Result<Vec<u8>, String> {
        self.documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("missing document provider")?
            .evaluation_identity_capability()
            .map_or_else(
                || {
                    self.validate_document(document)?;
                    Ok(document.payload.clone())
                },
                |capability| capability.evaluation_identity(document),
            )
    }
    pub fn video(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, fold_foundation::Failure> {
        self.video_with(snapshot, source, time, [width, height], &Cancel::default())
    }
    pub fn video_with(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
    ) -> Result<RenderGraph, fold_foundation::Failure> {
        self.validate_video_dependencies(
            snapshot,
            source,
            &mut Vec::new(),
            &mut BTreeSet::new(),
            cancel,
        )?;
        self.video
            .compile_with(snapshot, source, time, dimensions, cancel)
    }
    pub fn channels(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
    ) -> Result<Vec<fold_render::channels::ChannelName>, String> {
        self.video.channels(snapshot, source)
    }
    pub fn video_channels(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
        selection: &fold_render::view::Selection,
    ) -> Result<RenderGraph, fold_foundation::Failure> {
        self.validate_video_dependencies(
            snapshot,
            source,
            &mut Vec::new(),
            &mut BTreeSet::new(),
            cancel,
        )?;
        self.video
            .compile_channels(snapshot, source, time, dimensions, cancel, selection)
    }
    /// Validate references and dependency cycles without evaluating media or geometry.
    pub fn validate_video_source(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        cancel: &Cancel,
    ) -> Result<(), fold_foundation::Failure> {
        self.validate_video_dependencies(
            snapshot,
            source,
            &mut Vec::new(),
            &mut BTreeSet::new(),
            cancel,
        )
    }
    fn validate_video_dependencies(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        path: &mut Vec<DocumentId>,
        done: &mut BTreeSet<DocumentId>,
        cancel: &Cancel,
    ) -> Result<(), fold_foundation::Failure> {
        cancel.check()?;
        if path.contains(&source.document) || path.len() >= 64 {
            return Err(fold_foundation::Failure::new(
                fold_foundation::FailureKind::InvalidInput,
                "recursive or excessively deep document references",
            )
            .context(source.document));
        }
        let document = snapshot
            .state()
            .documents
            .get(&source.document)
            .ok_or_else(|| {
                fold_foundation::Failure::new(
                    fold_foundation::FailureKind::MissingResource,
                    "missing document",
                )
                .context(source.document)
            })?;
        if !self.supports(document) || !self.has_video(document) {
            return Err(fold_foundation::Failure::new(
                fold_foundation::FailureKind::UnsupportedCapability,
                "missing video provider or unsupported schema",
            )
            .context(document.id));
        }
        self.output_ref(snapshot, source).map_err(|error| {
            fold_foundation::Failure::new(fold_foundation::FailureKind::InvalidInput, error)
                .context(source.document)
                .context(&source.output)
        })?;
        if done.contains(&source.document) {
            return Ok(());
        }
        for id in &document.assets {
            if !snapshot.state().assets.contains_key(id) {
                return Err(fold_foundation::Failure::new(
                    fold_foundation::FailureKind::MissingResource,
                    "missing asset",
                )
                .context(id)
                .context(document.id));
            }
        }
        path.push(source.document);
        for dependency in &document.dependencies {
            self.validate_video_dependencies(snapshot, dependency, path, done, cancel)?;
        }
        path.pop();
        done.insert(source.document);
        Ok(())
    }
    /// A video-only document uses a silent clock; a failing audio provider must
    /// still report its error rather than silently falling back to that clock.
    pub fn supports_audio(&self, snapshot: &Snapshot, id: DocumentId) -> bool {
        snapshot.state().documents.get(&id).is_some_and(|document| {
            self.audio
                .iter()
                .any(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
        })
    }
    pub fn audio(&self, snapshot: &Snapshot, id: DocumentId) -> Result<AudioPlan, String> {
        let document = snapshot
            .state()
            .documents
            .get(&id)
            .ok_or("missing document")?;
        self.documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("missing document provider")?
            .validate(document)?;
        self.audio
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("missing audio provider")?
            .compile(snapshot, document)
    }
}
