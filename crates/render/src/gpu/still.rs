//! Bounded reuse of validated, scene-linear EXR inputs across times and edits.
//! Textures remain immutable. Unix file identity includes inode and ctime, so
//! replacement and writes with restored mtime invalidate reuse. Other platforms
//! rehash hits until an equally strong file-change identity is implemented.
use super::{host::Image, validation::Completion};
use crate::ImageOp;
use std::{collections::VecDeque, sync::Arc};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Key {
    alpha: crate::operations::SourceAlpha,
    fingerprint: String,
    info: Option<fold_media::exr::Info>,
    rgb_dimensions: Option<[u32; 2]>,
    channels: [Option<String>; 4],
    processor: Option<String>,
    dimensions: [u32; 2],
    path: Option<std::path::PathBuf>,
    stamp: Option<fold_media::exr::cache::Stamp>,
}
impl Key {
    pub fn new(
        op: &ImageOp,
        dimensions: [u32; 2],
        processors: &std::collections::BTreeMap<String, fold_color::Processor>,
    ) -> Result<Option<Self>, fold_foundation::Failure> {
        if let ImageOp::Media(image) = op {
            if let Some(identity) = image.still_identity() {
                return Ok(Some(Self {
                    alpha: Default::default(),
                    fingerprint: identity.into(),
                    info: None,
                    rgb_dimensions: Some(image.dimensions()),
                    channels: Default::default(),
                    processor: processors
                        .get(fold_color::settings::SRGB_INPUT)
                        .map(|p| p.identity().to_owned()),
                    dimensions,
                    path: None,
                    stamp: None,
                }));
            }
        }
        let ImageOp::Exr {
            source,
            channels,
            space,
            alpha,
        } = op
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            alpha: *alpha,
            fingerprint: source.fingerprint.clone(),
            info: Some(source.info.clone()),
            rgb_dimensions: None,
            channels: channels.clone(),
            processor: space
                .as_ref()
                .and_then(|s| processors.get(s))
                .map(|p| p.identity().to_owned()),
            dimensions,
            path: Some(source.path.clone()),
            stamp: Some(fold_media::exr::cache::Stamp::read(
                &source.path,
                &fold_media::Cancel::default(),
            )?),
        }))
    }
    fn bytes(&self) -> u64 {
        u64::from(self.dimensions[0]) * u64::from(self.dimensions[1]) * 16
    }
}
struct Entry {
    key: Key,
    image: Arc<Image>,
    ready: Completion,
}
#[derive(Default)]
pub(super) struct Cache {
    entries: VecDeque<Entry>,
    bytes: u64,
}
impl Cache {
    pub fn evict_one(&mut self) -> bool {
        if let Some(entry) = self.entries.pop_front() {
            self.bytes -= entry.image.bytes;
            true
        } else {
            false
        }
    }
    pub fn get(
        &mut self,
        key: &Key,
        cancel: &fold_media::Cancel,
    ) -> Result<Option<Arc<Image>>, fold_foundation::Failure> {
        let Some(index) = self.entries.iter().position(|e| &e.key == key) else {
            return Ok(None);
        };
        // Never hide a failed intermediate validation behind a cache hit.
        if self.entries[index].ready.ready() != Ok(true) {
            return Ok(None);
        }
        cancel.check()?;
        #[cfg(not(unix))]
        if fold_media::fingerprint(&key.path, cancel)? != key.fingerprint {
            return Err("EXR source fingerprint changed; reimport required".into());
        }
        let entry = self.entries.remove(index).unwrap();
        let image = entry.image.clone();
        self.entries.push_back(entry);
        Ok(Some(image))
    }
    pub fn eligible(key: &Key, budget: u64) -> bool {
        key.bytes() <= Self::limit(budget)
    }
    fn limit(budget: u64) -> u64 {
        (budget / 3).min(384 * 1024 * 1024)
    }
    pub fn insert(&mut self, key: Key, image: Arc<Image>, ready: Completion, budget: u64) {
        // Do not publish a stamp sampled after a concurrent source replacement.
        // Decode verified the bytes; the pre/post stamps must also agree.
        if !Self::eligible(&key, budget)
            || key.path.as_ref().is_some_and(|path| {
                fold_media::exr::cache::Stamp::read(path, &fold_media::Cancel::default()).ok()
                    != key.stamp
            })
        {
            return;
        }
        if let Some(index) = self.entries.iter().position(|e| e.key == key) {
            self.bytes -= self.entries.remove(index).unwrap().image.bytes;
        }
        while self.entries.len() >= 64 || self.bytes + image.bytes > Self::limit(budget) {
            self.bytes -= self.entries.pop_front().unwrap().image.bytes;
        }
        self.bytes += image.bytes;
        self.entries.push_back(Entry { key, image, ready });
    }
}
