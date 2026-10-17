//! Immutable, portable component revisions. Asset bytes are owned by the package.
use fold_media::Cancel;
use fold_platform::gallery::{Kind, Reference};
use fold_project::{Asset, Document, Metadata};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
};
pub const MAX_BYTES: u64 = 64 * 1024 * 1024;
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Resource {
    pub asset: Asset,
    pub bytes: Vec<u8>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Preview {
    pub size: [u32; 2],
    pub frames: Vec<Vec<u8>>,
    pub fps: u32,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Component {
    pub schema: u32,
    pub reference: Reference,
    pub name: String,
    pub category: String,
    pub kind: Kind,
    pub document: Document,
    pub resources: Vec<Resource>,
    pub preview: Option<Preview>,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug)]
pub struct Entry {
    pub reference: Reference,
    pub name: String,
    pub category: String,
    pub kind: Kind,
    pub has_preview: bool,
}
impl Component {
    pub fn entry(&self) -> Entry {
        Entry {
            reference: self.reference,
            name: self.name.clone(),
            category: self.category.clone(),
            kind: self.kind,
            has_preview: self.preview.is_some(),
        }
    }
}
impl Component {
    pub fn validate(&self) -> Result<(), String> {
        if self.schema != 1
            || self.reference.version == 0
            || self.name.trim().is_empty()
            || self.name.len() > 128
            || self.category.len() > 128
            || self.resources.len() > 256
            || !self.document.dependencies.is_empty()
        {
            return Err(
                "Unsupported component or invalid name, version, resource count or dependencies"
                    .into(),
            );
        }
        let ids: BTreeSet<_> = self.resources.iter().map(|r| r.asset.id).collect();
        if ids.len() != self.resources.len()
            || ids != self.document.assets.iter().copied().collect()
            || self
                .resources
                .iter()
                .map(|r| r.bytes.len() as u64)
                .sum::<u64>()
                > MAX_BYTES
        {
            return Err(
                "Component resources must contain exactly its declared assets within 64 MiB".into(),
            );
        }
        if let Some(p) = &self.preview
            && (p.size.contains(&0)
                || p.size[0] > 320
                || p.size[1] > 180
                || !(1..=24).contains(&p.fps)
                || p.frames.is_empty()
                || p.frames.len() > 12
                || p.frames
                    .iter()
                    .any(|f| f.len() != p.size[0] as usize * p.size[1] as usize * 4))
        {
            return Err("Invalid component preview".into());
        }
        crate::packages::builtins().validate_document(&self.document)?;
        super::validate_kind(self)
    }
}
#[derive(Clone)]
pub struct Store {
    pub root: PathBuf,
}
impl Store {
    pub fn user() -> Self {
        let root = std::env::var_os("FOLD_LIBRARY_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|| {
                std::env::var_os("XDG_DATA_HOME")
                    .map(PathBuf::from)
                    .unwrap_or_else(|| {
                        PathBuf::from(std::env::var_os("HOME").unwrap_or_default())
                            .join(".local/share")
                    })
                    .join("fold/library")
            });
        Self { root }
    }
    pub fn path(&self, r: Reference) -> PathBuf {
        self.root
            .join(r.id.to_string())
            .join(format!("{}.foldcomponent", r.version))
    }
    pub fn get(&self, r: Reference) -> Result<Component, String> {
        if let Some(c) = super::builtins::get(r) {
            return c;
        }
        let c = read(&self.path(r))?;
        if c.reference != r {
            return Err("Component identity differs from its file".into());
        }
        Ok(c)
    }
    pub fn list(&self) -> Result<Vec<Entry>, String> {
        let mut result: Vec<_> = super::builtins::all()?
            .iter()
            .map(Component::entry)
            .collect();
        if !self.root.exists() {
            return Ok(result);
        }
        for dir in fs::read_dir(&self.root).map_err(|e| e.to_string())? {
            let dir = dir.map_err(|e| e.to_string())?;
            if !dir.file_type().map_err(|e| e.to_string())?.is_dir() || dir.file_name() == "assets"
            {
                continue;
            }
            let mut versions = Vec::new();
            for file in fs::read_dir(dir.path()).map_err(|e| e.to_string())? {
                let path = file.map_err(|e| e.to_string())?.path();
                if path.extension().is_some_and(|x| x == "foldcomponent")
                    && let Some(v) = path
                        .file_stem()
                        .and_then(|s| s.to_str())
                        .and_then(|s| s.parse::<u32>().ok())
                {
                    versions.push((v, path));
                }
            }
            if let Some((_, path)) = versions.into_iter().max_by_key(|(v, _)| *v) {
                result.push(read(&path)?.entry());
            }
            if result.len() > 1024 {
                return Err("Library contains more than 1024 components".into());
            }
        }
        Ok(result)
    }
    pub fn put(
        &self,
        component: &Component,
        expected: Option<Reference>,
        cancel: &Cancel,
    ) -> Result<(), String> {
        component.validate()?;
        if super::builtins::contains(component.reference.id) {
            return Err("Built-in components are read-only; save a new copy".into());
        }
        let current = self
            .list()?
            .into_iter()
            .find(|c| c.reference.id == component.reference.id);
        if current.as_ref().map(|c| c.reference) != expected
            || component.reference.version != expected.map_or(1, |r| r.version.saturating_add(1))
        {
            return Err("Library revision changed; refresh before saving".into());
        }
        write_new(&self.path(component.reference), component, cancel)
    }
    pub fn materialize(&self, c: &Component, cancel: &Cancel) -> Result<Vec<Asset>, String> {
        if c.resources.is_empty() {
            return Ok(vec![]);
        }
        let root = self.root.join("assets");
        fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        c.resources
            .iter()
            .map(|r| {
                cancel.check()?;
                let hash = format!("{:x}", Sha256::digest(&r.bytes));
                let suffix = Path::new(&r.asset.location)
                    .extension()
                    .and_then(|s| s.to_str())
                    .filter(|s| s.len() < 12 && s.chars().all(|c| c.is_ascii_alphanumeric()))
                    .unwrap_or("bin");
                let path = root.join(format!("{hash}.{suffix}"));
                if !path.exists() {
                    let mut f =
                        tempfile::NamedTempFile::new_in(&root).map_err(|e| e.to_string())?;
                    f.write_all(&r.bytes).map_err(|e| e.to_string())?;
                    f.as_file().sync_all().map_err(|e| e.to_string())?;
                    if let Err(e) = f.persist_noclobber(&path)
                        && !path.exists()
                    {
                        return Err(e.to_string());
                    }
                }
                if read_bytes(&path, MAX_BYTES)? != r.bytes {
                    return Err("Library asset hash collision or damaged asset".into());
                }
                let mut asset = r.asset.clone();
                asset.location = path.to_string_lossy().into();
                Ok(asset)
            })
            .collect()
    }
    pub fn favorites(&self) -> Result<BTreeSet<fold_foundation::ObjectId>, String> {
        let path = self.root.join("favorites.json");
        if !path.exists() {
            return Ok(Default::default());
        }
        serde_json::from_slice(&read_bytes(&path, 128 * 1024)?).map_err(|e| e.to_string())
    }
    pub fn favorite(&self, r: Reference, value: bool) -> Result<(), String> {
        self.get(r)?;
        let mut ids = self.favorites()?;
        if value {
            ids.insert(r.id);
        } else {
            ids.remove(&r.id);
        }
        fs::create_dir_all(&self.root).map_err(|e| e.to_string())?;
        let mut f = tempfile::NamedTempFile::new_in(&self.root).map_err(|e| e.to_string())?;
        serde_json::to_writer(&mut f, &ids).map_err(|e| e.to_string())?;
        f.as_file().sync_all().map_err(|e| e.to_string())?;
        f.persist(self.root.join("favorites.json"))
            .map_err(|e| e.to_string())?;
        Ok(())
    }
}
pub fn read(path: &Path) -> Result<Component, String> {
    let bytes = read_bytes(path, MAX_BYTES)?;
    let c: Component = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    c.validate()?;
    Ok(c)
}
pub fn write_new(path: &Path, c: &Component, cancel: &Cancel) -> Result<(), String> {
    let bytes = serde_json::to_vec(c).map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Component package exceeds 64 MiB".into());
    }
    let parent = path.parent().ok_or("Missing component destination")?;
    fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    let mut f = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    f.write_all(&bytes).map_err(|e| e.to_string())?;
    f.as_file().sync_all().map_err(|e| e.to_string())?;
    cancel.check()?;
    f.persist_noclobber(path).map_err(|e| e.to_string())?;
    fs::File::open(parent)
        .and_then(|f| f.sync_all())
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Bound allocations before decoding or comparing package-owned bytes.
pub fn read_bytes(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    let mut bytes = Vec::new();
    fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("Component data exceeds its byte budget".into());
    }
    Ok(bytes)
}
