//! Bounded CPU image access for procedural consumers. Frames are shared within a
//! request; GPU-only sources fail explicitly rather than reading a viewer texture.
use crate::image_source::{self, Outside, Source};
use fold_foundation::Time;
use fold_media::{Cancel, VideoInfo};
use fold_project::{DocumentRef, Snapshot};
use fold_render::Frame;
use serde::{Deserialize, Serialize};
use std::{cell::RefCell, collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Request {
    pub source: Source,
    pub time: Time,
    pub outside: Outside,
    pub color_space: Option<String>,
}
pub trait Resolver {
    fn frame(&self, request: &Request) -> Result<Arc<Frame>, String>;
    fn provenance(&self) -> serde_json::Value {
        serde_json::json!([])
    }
}
type Describe<'a> = dyn Fn(&DocumentRef) -> Result<VideoInfo, String> + 'a;
struct Cache {
    frames: BTreeMap<String, Arc<Frame>>,
    sources: BTreeMap<String, serde_json::Value>,
    bytes: usize,
    decoder: fold_media::Decoder,
    config: Option<fold_color::Config>,
}
pub struct Service<'a> {
    snapshot: &'a Snapshot,
    cancel: &'a Cancel,
    resolve: &'a crate::VideoResolver<'a>,
    describe: &'a Describe<'a>,
    cache: RefCell<Cache>,
}
impl<'a> Service<'a> {
    pub fn new(
        snapshot: &'a Snapshot,
        cancel: &'a Cancel,
        resolve: &'a crate::VideoResolver<'a>,
        describe: &'a Describe<'a>,
    ) -> Self {
        Self {
            snapshot,
            cancel,
            resolve,
            describe,
            cache: RefCell::new(Cache {
                frames: BTreeMap::new(),
                sources: BTreeMap::new(),
                bytes: 0,
                decoder: Default::default(),
                config: None,
            }),
        }
    }
}
impl Resolver for Service<'_> {
    fn provenance(&self) -> serde_json::Value {
        serde_json::json!(self.cache.borrow().sources.values().collect::<Vec<_>>())
    }

    fn frame(&self, input: &Request) -> Result<Arc<Frame>, String> {
        self.cancel.check().map_err(|e| e.to_string())?;
        let info = match &input.source {
            Source::Asset(id) => image_source::asset_info(
                self.snapshot,
                *id,
                &VideoInfo {
                    width: 1,
                    height: 1,
                    rate: [24, 1],
                    frames: 1,
                },
            )?,
            Source::Output(reference) => (self.describe)(reference)?,
        };
        info.validate()?;
        let request = image_source::Request {
            bake: None,
            source: input.source.clone(),
            time: input.time,
            outside: input.outside,
            color_space: input.color_space.clone(),
            info,
        };
        // Key by mapped source frame; reverse seeks and repeated element samples
        // share the same immutable pixels. Document outputs keep fractional time.
        let mut canonical = request.clone();
        if let Some(time) = request.mapped_time()? {
            canonical.time = if matches!(input.source, Source::Asset(_)) {
                request.info.time(
                    time.to_ticks(
                        request.info.rate[0],
                        request.info.rate[1],
                        fold_foundation::Rounding::Floor,
                    )
                    .map_err(|e| e.to_string())? as u32,
                )?
            } else {
                time
            };
        }
        let key = serde_json::to_string(&canonical).map_err(|e| e.to_string())?;
        if let Some(frame) = self.cache.borrow().frames.get(&key) {
            return Ok(frame.clone());
        }
        let bytes = (request.info.width as usize)
            .checked_mul(request.info.height as usize)
            .and_then(|n| n.checked_mul(16))
            .ok_or("Image dimensions overflow")?;
        let limit = (fold_media::budget::working_usage().budget / 2) as usize;
        if bytes > limit
            || self.cache.borrow().bytes.saturating_add(bytes) > limit
            || self.cache.borrow().frames.len() >= 64
        {
            return Err("Image sampling exceeds its share of the CPU memory budget or 64 source frames per request".into());
        }
        let graph = image_source::compile_source(
            &request,
            self.snapshot,
            [request.info.width, request.info.height],
            self.cancel,
            self.resolve,
        )
        .map_err(|e| e.to_string())?;
        if [graph.width, graph.height] != [request.info.width, request.info.height] {
            return Err("Image source dimensions disagree with its native metadata".into());
        }
        let mut cache = self.cache.borrow_mut();
        if cache.config.is_none()
            && let Some(settings) = crate::color::project(self.snapshot)?
        {
            cache.config = Some(settings.load(&crate::color::runtime()?)?);
        }
        let Cache {
            decoder, config, ..
        } = &mut *cache;
        let frame = if let Some(config) = config {
            fold_render::render_aces_with(graph, config, decoder, self.cancel)
        } else {
            fold_render::render_with(graph, decoder, self.cancel)
        }
        .map_err(|e| format!("CPU image sampling failed: {e}"))?;
        let frame = Arc::new(frame);
        cache.bytes += bytes;
        let resource = match &request.source {
            Source::Asset(id) => {
                serde_json::json!({"asset":id,"fingerprint":self.snapshot.state().assets[id].fingerprint})
            }
            Source::Output(reference) => {
                serde_json::json!({"output":reference,"document_revision":self.snapshot.state().documents.get(&reference.document).ok_or("Missing image source document")?.revision,"project_revision":self.snapshot.revision()})
            }
        };
        cache.sources.insert(key.clone(), serde_json::json!({"request":canonical,"resource":resource,"dimensions":frame.dimensions(),"execution":"cpu","working_space":format!("{:?}",frame.working_space())}));
        cache.frames.insert(key, frame.clone());
        Ok(frame)
    }
}

/// Shared worker entry point for inspection and parameter sampling.
pub fn with_registry<R>(
    snapshot: &Snapshot,
    registry: &crate::packages::PackageRegistry,
    cancel: &Cancel,
    f: impl FnOnce(&dyn Resolver) -> R,
) -> R {
    let describe = |reference: &DocumentRef| registry.output_ref(snapshot, reference);
    let resolve = |reference: &DocumentRef, time| {
        let info = describe(reference)?;
        registry.video_with(snapshot, reference, time, [info.width, info.height], cancel)
    };
    let images = Service::new(snapshot, cancel, &resolve, &describe);
    f(&images)
}
