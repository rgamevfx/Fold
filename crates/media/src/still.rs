//! Bounded shared immutable PPM decode cache. A cache hit checks file metadata;
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
}
static CACHE: OnceLock<Mutex<VecDeque<Entry>>> = OnceLock::new();
pub fn ppm(
    path: &Path,
    fingerprint: &str,
    cancel: &Cancel,
) -> Result<RgbImage, fold_foundation::Failure> {
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
            let image = entry.image.clone();
            entries.push_back(entry);
            return Ok(image);
        }
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(crate::MAX_SOURCE_BYTES as u64 + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    cancel.check()?;
    if format!("sha256:{}", crate::content_hash(&bytes)) != fingerprint {
        return Err("image source changed; relink required".into());
    }
    let image = RgbImage::decode_ppm(&bytes)?;
    let mut entries = cache.lock().map_err(|_| "still cache lock poisoned")?;
    entries.retain(|e| e.path != path);
    let mut used: usize = entries
        .iter()
        .map(|e| {
            let d = e.image.dimensions();
            d[0] as usize * d[1] as usize * 3
        })
        .sum();
    let d = image.dimensions();
    let size = d[0] as usize * d[1] as usize * 3;
    if size > 64 * 1024 * 1024 {
        return Ok(image);
    }
    while used + size > 64 * 1024 * 1024 || entries.len() >= 64 {
        let Some(e) = entries.pop_front() else {
            break;
        };
        let d = e.image.dimensions();
        used -= d[0] as usize * d[1] as usize * 3;
    }
    entries.push_back(Entry {
        path: path.into(),
        fingerprint: fingerprint.into(),
        stamp,
        image: image.clone(),
    });
    Ok(image)
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
        let a = ppm(&path, &fingerprint, &Cancel::default()).unwrap();
        let b = ppm(&path, &fingerprint, &Cancel::default()).unwrap();
        assert!(std::sync::Arc::ptr_eq(&a.rgb, &b.rgb));
        assert_eq!(a.rgb_identity(), b.rgb_identity());
        std::fs::write(&path, b"P6\n1 1\n255\n\x10\x40\x20").unwrap();
        assert!(
            ppm(&path, &fingerprint, &Cancel::default())
                .unwrap_err()
                .message
                .contains("changed")
        );
        let cancel = Cancel::default();
        cancel.cancel();
        assert!(ppm(&path, &fingerprint, &cancel).is_err());
    }
}
