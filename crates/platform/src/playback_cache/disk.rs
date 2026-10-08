//! Versioned cache files. All filesystem operations run on the cache worker.
use super::{ENTRY_LIMIT, TRANSFER_LIMIT};
use crate::{
    desktop::PreviewKey,
    presentation::{Description, Pixels},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::HashMap,
    fs,
    io::{Read, Write},
    path::PathBuf,
};
const MAGIC: &[u8; 8] = b"FLDCCH01";
const HEADER_LIMIT: usize = 64 * 1024;
#[derive(Serialize, Deserialize)]
struct Header {
    key: PreviewKey,
    description: Description,
    checksum: u64,
}
struct Entry {
    path: PathBuf,
    bytes: u64,
    age: u64,
}
pub struct Disk {
    root: PathBuf,
    entries: HashMap<PreviewKey, Entry>,
    bytes: u64,
    budget: u64,
    clock: u64,
    _lock: Option<fs::File>,
}
// Filenames are only an index hint. Full identity and payload checksum are verified.
fn digest(bytes: &[u8]) -> u64 {
    bytes.iter().fold(0xcbf29ce484222325, |hash, byte| {
        (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
    })
}
fn header(file: &mut fs::File) -> Result<Header, String> {
    let mut prefix = [0; 12];
    file.read_exact(&mut prefix).map_err(|e| e.to_string())?;
    if &prefix[..8] != MAGIC {
        return Err("Unknown cache format".into());
    }
    let length = u32::from_le_bytes(prefix[8..].try_into().unwrap()) as usize;
    if length > HEADER_LIMIT {
        return Err("Oversized cache header".into());
    }
    let mut data = vec![0; length];
    file.read_exact(&mut data).map_err(|e| e.to_string())?;
    let value: Header = serde_json::from_slice(&data).map_err(|e| e.to_string())?;
    let bytes = value.description.bytes()?;
    if bytes > TRANSFER_LIMIT || value.description.dimensions != value.key.image_dimensions() {
        return Err("Invalid cache dimensions".into());
    }
    if let Some(region) = value.key.region {
        region.validate(value.key.dimensions)?;
    }
    if file.metadata().map_err(|e| e.to_string())?.len() != (12 + length + bytes) as u64 {
        return Err("Truncated cache file".into());
    }
    Ok(value)
}
impl Disk {
    /// Call only on a worker. `root` must be a dedicated versioned cache directory.
    pub fn open(root: PathBuf, budget: u64, cancel: &fold_media::Cancel) -> Result<Self, String> {
        let mut store = Self {
            root,
            entries: HashMap::new(),
            bytes: 0,
            budget,
            clock: 0,
            _lock: None,
        };
        if budget == 0 {
            return Ok(store);
        }
        cancel.check()?;
        fs::create_dir_all(&store.root).map_err(|e| e.to_string())?;
        let lock = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(store.root.join(".lock"))
            .map_err(|e| e.to_string())?;
        lock.try_lock()
            .map_err(|e| format!("Playback cache is already in use: {e}"))?;
        store._lock = Some(lock);
        for item in fs::read_dir(&store.root).map_err(|e| e.to_string())? {
            cancel.check()?;
            let item = item.map_err(|e| e.to_string())?;
            if !item.file_type().map_err(|e| e.to_string())?.is_file() {
                continue;
            }
            let path = item.path();
            let name = item.file_name();
            let name = name.to_string_lossy();
            let parts: Vec<_> = name.split('.').collect();
            if parts.len() == 3
                && parts[0].len() == 16
                && parts[0].bytes().all(|byte| byte.is_ascii_hexdigit())
                && parts[1].parse::<u32>().is_ok()
                && parts[2] == "tmp"
            {
                fs::remove_file(&path).map_err(|e| e.to_string())?;
                continue;
            }
            if path.extension().and_then(|s| s.to_str()) != Some("foldcache") {
                continue;
            }
            let loaded = fs::File::open(&path)
                .map_err(|e| e.to_string())
                .and_then(|mut f| header(&mut f));
            let Ok(value) = loaded else {
                let _ = fs::remove_file(path);
                continue;
            };
            let metadata = item.metadata().map_err(|e| e.to_string())?;
            let age = metadata
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |t| t.as_millis() as u64);
            store.clock = store.clock.max(age);
            let entry = Entry {
                path,
                bytes: metadata.len(),
                age,
            };
            if let Some(old) = store.entries.insert(value.key, entry) {
                store.bytes -= old.bytes;
            }
            store.bytes += metadata.len();
            store.trim(0)?;
        }
        Ok(store)
    }
    pub fn usage(&self) -> (u64, u64) {
        (self.bytes, self.budget)
    }
    pub fn keys(&self) -> impl Iterator<Item = &PreviewKey> {
        self.entries.keys()
    }
    pub fn contains(&self, key: &PreviewKey) -> bool {
        self.entries.contains_key(key)
    }
    fn remove(&mut self, key: &PreviewKey) -> Result<(), String> {
        let Some(entry) = self.entries.get(key) else {
            return Ok(());
        };
        match fs::remove_file(&entry.path) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => return Err(e.to_string()),
        }
        self.bytes -= entry.bytes;
        self.entries.remove(key);
        Ok(())
    }
    fn trim(&mut self, incoming: u64) -> Result<Vec<PreviewKey>, String> {
        let mut evicted = vec![];
        while self.bytes + incoming > self.budget || self.entries.len() >= ENTRY_LIMIT {
            let Some(old) = self
                .entries
                .iter()
                .min_by_key(|(_, e)| e.age)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            self.remove(&old)?;
            evicted.push(old);
        }
        Ok(evicted)
    }
    pub fn get(&mut self, key: &PreviewKey) -> Result<Option<Pixels>, String> {
        let Some(entry) = self.entries.get(key) else {
            return Ok(None);
        };
        let loaded = (|| {
            let mut file = fs::File::open(&entry.path).map_err(|e| e.to_string())?;
            let value = header(&mut file)?;
            if &value.key != key {
                return Err("Cache identity mismatch".into());
            }
            let mut data = vec![0; value.description.bytes()?];
            file.read_exact(&mut data).map_err(|e| e.to_string())?;
            if digest(&data) != value.checksum {
                return Err("Cache checksum mismatch".into());
            }
            Ok(Pixels {
                description: value.description,
                data,
            })
        })();
        match loaded {
            Ok(pixels) => {
                self.clock += 1;
                self.entries.get_mut(key).unwrap().age = self.clock;
                Ok(Some(pixels))
            }
            Err(error) => {
                self.remove(key)?;
                Err(error)
            }
        }
    }
    pub fn insert(&mut self, key: PreviewKey, pixels: &Pixels) -> Result<Vec<PreviewKey>, String> {
        pixels.validate()?;
        if self.budget == 0 || pixels.data.len() > TRANSFER_LIMIT || self.contains(&key) {
            return Ok(vec![]);
        }
        if pixels.description.dimensions != key.image_dimensions() {
            return Err("Cache identity dimensions mismatch".into());
        }
        let value = Header {
            key: key.clone(),
            description: pixels.description.clone(),
            checksum: digest(&pixels.data),
        };
        let data = serde_json::to_vec(&value).map_err(|e| e.to_string())?;
        if data.len() > HEADER_LIMIT {
            return Err("Cache header too large".into());
        }
        let bytes = (12 + data.len() + pixels.data.len()) as u64;
        if bytes > self.budget {
            return Ok(vec![]);
        }
        let evicted = self.trim(bytes)?;
        let identity = serde_json::to_vec(&key).map_err(|e| e.to_string())?;
        let path = self
            .root
            .join(format!("{:016x}.foldcache", digest(&identity)));
        // Never overwrite a different indexed key on a hash collision.
        if self.entries.values().any(|entry| entry.path == path) {
            return Err("Cache filename collision".into());
        }
        let temp = path.with_extension(format!("{}.tmp", std::process::id()));
        let written = (|| {
            let mut file = fs::File::create(&temp).map_err(|e| e.to_string())?;
            file.write_all(MAGIC)
                .and_then(|_| file.write_all(&(data.len() as u32).to_le_bytes()))
                .and_then(|_| file.write_all(&data))
                .and_then(|_| file.write_all(&pixels.data))
                .and_then(|_| file.flush())
                .map_err(|e| e.to_string())?;
            fs::rename(&temp, &path).map_err(|e| e.to_string())
        })();
        if written.is_err() {
            let _ = fs::remove_file(temp);
        }
        written?;
        self.clock += 1;
        self.bytes += bytes;
        self.entries.insert(
            key,
            Entry {
                path,
                bytes,
                age: self.clock,
            },
        );
        Ok(evicted)
    }
    pub fn clear(&mut self) -> Result<(), String> {
        if self.budget == 0 && self.root.is_dir() {
            // Disabled retention does not index files until an explicit clear.
            return Self::open(self.root.clone(), u64::MAX, &Default::default())?.clear();
        }
        for key in self.entries.keys().cloned().collect::<Vec<_>>() {
            self.remove(&key)?;
        }
        Ok(())
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crate::playback_cache::tests::{key, pixels};
    #[test]
    fn restart_isolation_corruption_and_scoped_clear() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("timeline");
        let mut disk = Disk::open(root.clone(), 4096, &Default::default()).unwrap();
        disk.insert(key(0), &pixels()).unwrap();
        assert!(
            Disk::open(root.clone(), 4096, &Default::default()).is_err(),
            "one writer per disk quota"
        );
        fs::write(root.join("keep.txt"), "unrelated").unwrap();
        let mut other =
            Disk::open(dir.path().join("composition"), 4096, &Default::default()).unwrap();
        other.insert(key(1), &pixels()).unwrap();
        drop(disk);
        let mut disk = Disk::open(root.clone(), 4096, &Default::default()).unwrap();
        assert_eq!(disk.get(&key(0)).unwrap().unwrap().data, pixels().data);
        let path = disk.entries[&key(0)].path.clone();
        let mut bytes = fs::read(&path).unwrap();
        *bytes.last_mut().unwrap() ^= 1;
        fs::write(path, bytes).unwrap();
        assert!(disk.get(&key(0)).is_err());
        assert!(!disk.contains(&key(0)));
        disk.insert(key(2), &pixels()).unwrap();
        disk.clear().unwrap();
        assert!(root.join("keep.txt").exists());
        assert!(other.get(&key(1)).unwrap().is_some());
    }
    #[test]
    fn startup_discards_truncated_files_and_owned_temps_and_disabled_clear_works() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("cache");
        let mut disk = Disk::open(root.clone(), 4096, &Default::default()).unwrap();
        disk.insert(key(0), &pixels()).unwrap();
        let path = disk.entries[&key(0)].path.clone();
        fs::write(&path, MAGIC).unwrap();
        fs::write(root.join("0123456789abcdef.123.tmp"), "partial").unwrap();
        fs::write(root.join("keep.tmp"), "unrelated").unwrap();
        drop(disk);
        let mut disk = Disk::open(root.clone(), 4096, &Default::default()).unwrap();
        assert!(disk.get(&key(0)).unwrap().is_none());
        assert!(!path.exists());
        assert!(!root.join("0123456789abcdef.123.tmp").exists());
        assert!(root.join("keep.tmp").exists());
        disk.insert(key(1), &pixels()).unwrap();
        drop(disk);
        let mut disabled = Disk::open(root.clone(), 0, &Default::default()).unwrap();
        disabled.clear().unwrap();
        drop(disabled);
        let reopened = Disk::open(root.clone(), 4096, &Default::default()).unwrap();
        assert_eq!(reopened.usage().0, 0);
        assert!(root.join("keep.tmp").exists());
    }
    #[test]
    fn disk_budget_evicts_and_disabled_store_does_not_create_folder() {
        let dir = tempfile::tempdir().unwrap();
        let mut disk = Disk::open(dir.path().join("frames"), 700, &Default::default()).unwrap();
        for frame in 0..20 {
            disk.insert(key(frame), &pixels()).unwrap();
            assert!(disk.usage().0 <= 700);
        }
        assert!(disk.contains(&key(19)));
        assert!(!disk.contains(&key(0)));
        let root = dir.path().join("disabled");
        let mut disk = Disk::open(root.clone(), 0, &Default::default()).unwrap();
        disk.insert(key(0), &pixels()).unwrap();
        assert!(!root.exists());
    }
}
