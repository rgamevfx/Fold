//! Versioned in-process contribution contracts. These Rust traits/function
//! pointers are deliberately NOT a durable dynamic-library ABI.
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
pub trait DocumentProvider: Send + Sync {
    fn package_id(&self) -> &'static str;
    fn type_id(&self) -> &'static str;
    fn schema(&self) -> u32;
    fn supports_schema(&self, schema: u32) -> bool {
        schema == self.schema()
    }
    fn validate(&self, document: &Document) -> Result<(), String>;
    fn browser_kind(&self) -> Option<crate::browser::DocumentKind> {
        None
    }
    fn create(&self, _id: DocumentId) -> Result<Document, String> {
        Err("provider does not support document creation".into())
    }
    fn place(
        &self,
        _snapshot: &Snapshot,
        _placement: &crate::browser::Placement,
        _info: Option<VideoInfo>,
    ) -> Result<Document, String> {
        Err("this editor does not support this Project item".into())
    }
    /// Validate every authored use before relinking. Unknown providers fail closed.
    fn validate_relink(
        &self,
        _document: &Document,
        _asset: fold_foundation::AssetId,
        _metadata: &fold_media::ingest::SourceMetadata,
    ) -> Result<(), String> {
        Err("provider does not support asset relinking".into())
    }
    /// Semantic evaluation identity, excluding provider-owned presentation data.
    fn evaluation_identity(&self, document: &Document) -> Result<Vec<u8>, String> {
        self.validate(document)?;
        Ok(document.payload.clone())
    }
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
impl PackageRegistry {
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
                || !owned(provider.type_id())
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
    pub fn document_kinds(&self) -> Vec<crate::browser::DocumentKind> {
        self.documents
            .iter()
            .filter_map(|p| p.browser_kind())
            .collect()
    }
    pub fn create(&self, kind: &str, id: DocumentId) -> Result<Document, String> {
        let provider = self
            .documents
            .iter()
            .find(|p| p.type_id() == kind)
            .ok_or("unavailable document provider")?;
        let document = provider.create(id)?;
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
        let document = provider.place(snapshot, placement, info)?;
        provider.validate(&document)?;
        if document.id != placement.target {
            return Err("placement changed destination identity".into());
        }
        Ok(EditBatch {
            base: snapshot.revision(),
            mutations: vec![Mutation::PutDocument(document)],
        })
    }
    pub fn manifests(&self) -> impl Iterator<Item = &Manifest> {
        self.manifests.values()
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
        provider.validate_relink(document, asset, metadata)
    }
    pub fn supports(&self, document: &Document) -> bool {
        self.documents.iter().any(|p| {
            p.package_id() == document.package_id
                && p.type_id() == document.type_id
                && p.supports_schema(document.schema_version)
        })
    }
    pub fn outputs(
        &self,
        snapshot: &Snapshot,
        id: DocumentId,
    ) -> Vec<crate::workspace::OutputDescriptor> {
        let Some(document) = snapshot.state().documents.get(&id) else {
            return vec![];
        };
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
    pub fn output_ref(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
    ) -> Result<VideoInfo, String> {
        if !snapshot.state().documents.contains_key(&source.document) {
            return Err("missing document".into());
        }
        self.outputs(snapshot, source.document)
            .into_iter()
            .find(|o| o.reference.output == source.output)
            .ok_or_else(|| format!("missing output port {}", source.output))?
            .info
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
            .evaluation_identity(document)
    }
    pub fn video(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        width: u32,
        height: u32,
    ) -> Result<RenderGraph, String> {
        self.video_with(snapshot, source, time, [width, height], &Cancel::default())
    }
    pub fn video_with(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        time: Time,
        dimensions: [u32; 2],
        cancel: &Cancel,
    ) -> Result<RenderGraph, String> {
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
    fn validate_video_dependencies(
        &self,
        snapshot: &Snapshot,
        source: &DocumentRef,
        path: &mut Vec<DocumentId>,
        done: &mut BTreeSet<DocumentId>,
        cancel: &Cancel,
    ) -> Result<(), String> {
        cancel.check()?;
        if path.contains(&source.document) || path.len() >= 64 {
            return Err("recursive or excessively deep document references".into());
        }
        self.output_ref(snapshot, source).map_err(|error| {
            format!(
                "video dependency {:?} / {}: {error}",
                source.document, source.output
            )
        })?;
        if done.contains(&source.document) {
            return Ok(());
        }
        let document = &snapshot.state().documents[&source.document];
        if !self
            .video
            .providers
            .iter()
            .any(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
        {
            return Err(format!("missing video capability: {:?}", document.id));
        }
        for id in &document.assets {
            if !snapshot.state().assets.contains_key(id) {
                return Err(format!("missing asset {id:?}"));
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
