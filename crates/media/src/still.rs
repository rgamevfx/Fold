//! Bounded shared immutable still-image decode cache. A cache hit checks file metadata;
//! new bytes must match the snapshot fingerprint before they can be published.
use crate::{Cancel, RgbImage};
use std::{
    collections::VecDeque,
    io::Read,
    path::{Path, PathBuf},
    sync::{Mutex, OnceLock},
};
struct Entry {
    path: PathBuf,
    fingerprint: String,
    stamp: crate::exr::cache::Stamp,
    image: RgbImage,
    info: Option<crate::raster::Info>,
}
static CACHE: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();
pub fn load(
    path: &Path,
    fingerprint: &str,
    cancel: &Cancel,
) -> Result<(RgbImage, Option<crate::raster::Info>), fold_foundation::Failure> {
    cancel.check()?;
    let stamp = crate::exr::cache::Stamp::read(path, cancel)?;
    let cache = CACHE.get_or_init(|| Mutex::new(VecDeque::new()));
    {
        let mut entries = cache.lock().map_err(|_| "still cache lock poisoned")?;
        if let Some(index) = entries
            .iter()
            .position(|e| e.path == path && e.fingerprint == fingerprint && e.stamp == stamp)
        {
            let entry = entries.remove(index).unwrap();
            let image = (entry.image.clone(), entry.info.clone());
            entries.push_back(entry);
            return Ok(image);
        }
    }
    let size = std::fs::metadata(path).map_err(|e| e.to_string())?.len();
    let _source_storage = crate::budget::reserve_working(size + 1)?;
    let capacity = usize::try_from(size + 1).map_err(|_| "image source size overflow")?;
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(capacity)
        .map_err(|_| "image source allocation failed")?;
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(size + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    cancel.check()?;
    if format!("sha256:{}", crate::content_hash(&bytes)) != fingerprint {
        return Err("image source changed; relink required".into());
    }
    let (image, info) = if bytes.starts_with(b"P6") {
        (RgbImage::decode_ppm(&bytes)?, None)
    } else {
        let (image, info) = crate::raster::decode(&bytes, cancel)?;
        (image, Some(info))
    };
    cancel.check()?;
    let mut entries = cache.lock().map_err(|_| "still cache lock poisoned")?;
    entries.retain(|e| e.path != path);
    let mut used: usize = entries
        .iter()
        .map(|e| e.image.storage_bytes() as usize)
        .sum();
    let size = image.storage_bytes() as usize;
    let limit = crate::budget::decoded_usage().budget as usize / 2;
    if size > limit {
        return Ok((image, info));
    }
    while used + size > limit || entries.len() >= 64 {
        let Some(e) = entries.pop_front() else {
            break;
        };
        used -= e.image.storage_bytes() as usize;
    }
    entries.push_back(Entry {
        path: path.into(),
        fingerprint: fingerprint.into(),
        stamp,
        image: image.clone(),
        info: info.clone(),
    });
    Ok((image, info))
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn shared_still_reuses_identity_and_rejects_changed_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("sample.ppm");
        let bytes = b"P6\n1 1\n255\n\x80\x40\x20";
        std::fs::write(&path, bytes).unwrap();
        let fingerprint = format!("sha256:{}", crate::content_hash(bytes));
        let a = load(&path, &fingerprint, &Cancel::default()).unwrap().0;
        let b = load(&path, &fingerprint, &Cancel::default()).unwrap().0;
        assert!(std::sync::Arc::ptr_eq(&a.rgb, &b.rgb));
        assert_eq!(a.still_identity(), b.still_identity());
        std::fs::write(&path, b"P6\n1 1\n255\n\x10\x40\x20").unwrap();
        assert!(
            load(&path, &fingerprint, &Cancel::default())
                .unwrap_err()
                .message
                .contains("changed")
        );
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(load(&path, &fingerprint, &cancel).is_err());
    }
}
