//! Feature-neutral cache references and source invalidation for prepared textures.
use crate::image_source::{Request, Source};
use serde::{Deserialize, Serialize};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::PathBuf,
    sync::{Mutex, OnceLock},
};
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Reference {
    pub directory: PathBuf,
    pub manifest: String,
    pub options: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Record {
    pub texture: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Manifest {
    pub version: u32,
    pub frames: u32,
    pub bytes: u64,
    pub entries: BTreeMap<String, Record>,
}
pub fn key(snapshot: &fold_project::Snapshot, request: &Request) -> Result<String, String> {
    let mut request = request.clone();
    request.bake = None;
    let mapped = request.mapped_time()?;
    if let Some(time) = mapped {
        request.time = time;
    }
    if let Source::Asset(id) = request.source {
        let profile = crate::browser::asset_metadata(snapshot, id)?.profile;
        if mapped.is_some()
            && (matches!(profile, fold_media::ingest::SourceProfile::Ppm { .. })
                || matches!(profile,fold_media::ingest::SourceProfile::Exr(ref info) if info.sequence.is_none()))
        {
            request.time = fold_foundation::Time::ZERO;
        }
    }
    let mut bytes = serde_json::to_vec(&request).map_err(|e| e.to_string())?;
    bytes.extend(serde_json::to_vec(&snapshot.state().settings).map_err(|e| e.to_string())?);
    fn asset(
        snapshot: &fold_project::Snapshot,
        id: fold_foundation::AssetId,
        bytes: &mut Vec<u8>,
    ) -> Result<(), String> {
        let asset = snapshot
            .state()
            .assets
            .get(&id)
            .ok_or("missing baked asset")?;
        bytes.extend(serde_json::to_vec(asset.as_ref()).map_err(|e| e.to_string())?);
        let stamp = fold_media::exr::cache::Stamp::read(
            std::path::Path::new(&asset.location),
            &fold_media::Cancel::default(),
        )?;
        bytes.extend(format!("{stamp:?}").bytes());
        Ok(())
    }
    fn document(
        snapshot: &fold_project::Snapshot,
        id: fold_foundation::DocumentId,
        seen: &mut BTreeSet<fold_foundation::DocumentId>,
        bytes: &mut Vec<u8>,
    ) -> Result<(), String> {
        if !seen.insert(id) {
            return Ok(());
        }
        if seen.len() > 256 {
            return Err("image source dependency budget exceeded".into());
        }
        let doc = snapshot
            .state()
            .documents
            .get(&id)
            .ok_or("missing baked document source")?;
        bytes.extend(serde_json::to_vec(doc.as_ref()).map_err(|e| e.to_string())?);
        for id in &doc.assets {
            asset(snapshot, *id, bytes)?;
        }
        for dep in &doc.dependencies {
            document(snapshot, dep.document, seen, bytes)?;
        }
        Ok(())
    }
    match &request.source {
        Source::Asset(id) => asset(snapshot, *id, &mut bytes)?,
        Source::Output(r) => document(snapshot, r.document, &mut BTreeSet::new(), &mut bytes)?,
    }
    Ok(fold_media::content_hash(&bytes))
}
pub fn resolve(
    reference: &Reference,
    snapshot: &fold_project::Snapshot,
    request: &Request,
    cancel: &fold_media::Cancel,
) -> Result<std::sync::Arc<fold_render::baked::Texture>, fold_foundation::Failure> {
    cancel.check()?;
    let manifest = manifest(reference, cancel)?;
    let record =
        manifest.data.entries.get(&key(snapshot, request)?).ok_or(
            "Texture bake is stale or this time is outside its range; rebuild or clear it",
        )?;
    if record.texture.len() != 64 || !record.texture.bytes().all(|c| c.is_ascii_hexdigit()) {
        return Err("invalid texture identity".into());
    }
    fold_render::baked::Texture::load(
        &reference
            .directory
            .join(format!("{}.foldtex", record.texture)),
        &record.texture,
        cancel,
    )
}
struct CachedManifest {
    data: Manifest,
    bytes: usize,
    _storage: std::sync::Arc<fold_media::budget::Lease>,
}
fn manifest(
    reference: &Reference,
    cancel: &fold_media::Cancel,
) -> Result<std::sync::Arc<CachedManifest>, String> {
    use std::{collections::VecDeque, io::Read, sync::Arc};
    type Entry = (
        PathBuf,
        String,
        fold_media::exr::cache::Stamp,
        Arc<CachedManifest>,
    );
    static CACHE: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();
    let path = reference.directory.join("manifest.json");
    let stamp = fold_media::exr::cache::Stamp::read(&path, cancel)
        .map_err(|_| "Texture bake is missing; rebuild or clear it")?;
    let cache = CACHE.get_or_init(Default::default);
    {
        let mut entries = cache.lock().map_err(|_| "manifest cache poisoned")?;
        if let Some(i) = entries
            .iter()
            .position(|e| e.0 == path && e.1 == reference.manifest && e.2 == stamp)
        {
            let entry = entries.remove(i).unwrap();
            let result = entry.3.clone();
            entries.push_back(entry);
            return Ok(result);
        }
    }
    let size = std::fs::metadata(&path).map_err(|e| e.to_string())?.len();
    if size > 8 * 1024 * 1024 {
        return Err("Texture bake manifest exceeds its size budget".into());
    }
    // Covers file bytes and the decoded strings/tree; follows the cached value.
    let storage = fold_media::budget::reserve_working(size * 4)?;
    let mut bytes = vec![];
    std::fs::File::open(&path)
        .map_err(|e| e.to_string())?
        .take(8 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 != size || fold_media::content_hash(&bytes) != reference.manifest {
        return Err("Texture bake manifest changed; rebuild it".into());
    }
    let data: Manifest = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
    if data.version != 1 || data.entries.len() > 10000 {
        return Err("unsupported texture bake".into());
    }
    let result = Arc::new(CachedManifest {
        data,
        bytes: bytes.len(),
        _storage: storage,
    });
    let mut entries = cache.lock().map_err(|_| "manifest cache poisoned")?;
    entries.retain(|e| e.0 != path);
    while entries.len() >= 16
        || entries.iter().map(|e| e.3.bytes).sum::<usize>() + result.bytes > 8 * 1024 * 1024
    {
        if entries.pop_front().is_none() {
            break;
        }
    }
    entries.push_back((path, reference.manifest.clone(), stamp, result.clone()));
    Ok(result)
}
