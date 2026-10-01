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
    pub fn supports(&self, document: &Document) -> bool {
        self.documents.iter().any(|p| {
            p.package_id() == document.package_id
                && p.type_id() == document.type_id
                && p.supports_schema(document.schema_version)
        })
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
        if source.output != "video" {
            return Err(format!("unsupported video port: {}", source.output));
        }
        if path.contains(&source.document) || path.len() >= 64 {
            return Err("recursive or excessively deep document references".into());
        }
        if done.contains(&source.document) {
            return Ok(());
        }
        self.output(snapshot, source.document).map_err(|error| {
            format!(
                "video dependency {:?} / {}: {error}",
                source.document, source.output
            )
        })?;
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
