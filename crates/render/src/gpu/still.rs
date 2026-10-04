//! Bounded reuse of validated, scene-linear EXR inputs across times and edits.
//! Textures remain immutable. Unix file identity includes inode and ctime, so
//! replacement and writes with restored mtime invalidate reuse. Other platforms
//! rehash hits until an equally strong file-change identity is implemented.
use super::{host::Image, validation::Completion};
use crate::ImageOp;
use std::{collections::VecDeque, sync::Arc};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Key {
    fingerprint: String,
    info: fold_media::exr::Info,
    channels: [Option<String>; 4],
    processor: Option<String>,
    dimensions: [u32; 2],
    path: std::path::PathBuf,
    stamp: Stamp,
}
impl Key {
    pub fn new(
        op: &ImageOp,
        dimensions: [u32; 2],
        processors: &std::collections::BTreeMap<String, fold_color::Processor>,
    ) -> Result<Option<Self>, String> {
        let ImageOp::Exr {
            source,
            channels,
            space,
        } = op
        else {
            return Ok(None);
        };
        Ok(Some(Self {
            fingerprint: source.fingerprint.clone(),
            info: source.info.clone(),
            channels: channels.clone(),
            processor: space
                .as_ref()
                .and_then(|s| processors.get(s))
                .map(|p| p.identity().to_owned()),
            dimensions,
            path: source.path.clone(),
            stamp: stamp(&source.path)?,
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
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
    pub fn get(
        &mut self,
        key: &Key,
        cancel: &fold_media::Cancel,
    ) -> Result<Option<Arc<Image>>, String> {
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
        (budget / 8).min(128 * 1024 * 1024)
    }
    pub fn insert(&mut self, key: Key, image: Arc<Image>, ready: Completion, budget: u64) {
        // Do not publish a stamp sampled after a concurrent source replacement.
        // Decode verified the bytes; the pre/post stamps must also agree.
        if !Self::eligible(&key, budget) || stamp(&key.path).ok().as_ref() != Some(&key.stamp) {
            return;
        }
        if let Some(index) = self.entries.iter().position(|e| e.key == key) {
            self.bytes -= self.entries.remove(index).unwrap().image.bytes;
        }
        while self.entries.len() >= 8 || self.bytes + image.bytes > Self::limit(budget) {
            self.bytes -= self.entries.pop_front().unwrap().image.bytes;
        }
        self.bytes += image.bytes;
        self.entries.push_back(Entry { key, image, ready });
    }
}

#[cfg(unix)]
#[derive(Clone, PartialEq, Eq)]
struct Stamp {
    device: u64,
    inode: u64,
    size: u64,
    modified: (i64, i64),
    changed: (i64, i64),
}
#[cfg(unix)]
fn stamp(path: &std::path::Path) -> Result<Stamp, String> {
    use std::os::unix::fs::MetadataExt;
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    if !metadata.is_file() {
        return Err("EXR source must be a regular file".into());
    }
    Ok(Stamp {
        device: metadata.dev(),
        inode: metadata.ino(),
        size: metadata.len(),
        modified: (metadata.mtime(), metadata.mtime_nsec()),
        changed: (metadata.ctime(), metadata.ctime_nsec()),
    })
}
#[cfg(not(unix))]
type Stamp = ();
#[cfg(not(unix))]
fn stamp(_path: &std::path::Path) -> Result<Stamp, String> {
    Ok(())
}
