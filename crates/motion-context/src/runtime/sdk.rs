//! Native-worker integrity and deferred script-library verification.
use super::{file_digest_guarded, hash_part, io_error};
use fold_foundation::Failure;
use fold_media::Cancel;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Component, Path, PathBuf},
    sync::atomic::{AtomicBool, Ordering},
    time::{Duration, Instant},
};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    version: u32,
    profile: String,
    rustc: String,
    inputs: BTreeMap<String, String>,
    files: BTreeMap<String, String>,
    externs: BTreeMap<String, String>,
    native_worker: String,
}
pub(super) struct Libraries {
    pub directory: PathBuf,
    pub externs: BTreeMap<String, PathBuf>,
    pub identity: String,
    pub native_worker: PathBuf,
    pub native_digest: String,
    files: BTreeMap<String, String>,
    scripts_verified: AtomicBool,
}
fn relative(root: &Path, name: &str) -> Result<PathBuf, Failure> {
    if name.is_empty()
        || !Path::new(name)
            .components()
            .all(|c| matches!(c, Component::Normal(_)))
    {
        return Err("Invalid SDK input path".into());
    }
    Ok(root.join(name))
}
impl Libraries {
    pub fn load(
        bridge: &Path,
        compiler: &[u8],
        cancel: &Cancel,
        timeout: Duration,
    ) -> Result<Self, Failure> {
        let start = Instant::now();
        let root = bridge
            .parent()
            .and_then(Path::parent)
            .ok_or("Missing Motion SDK root")?;
        let directory = root.join("libraries");
        let body = fs::read(directory.join("manifest.json")).map_err(io_error)?;
        let manifest: Manifest = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        if manifest.version != 2
            || !matches!(manifest.profile.as_str(), "debug" | "release")
            || manifest.rustc.as_bytes() != compiler
            || manifest.inputs.is_empty()
            || manifest.files.is_empty()
        {
            return Err("Motion SDK compiler or library manifest differs; rebuild the SDK".into());
        }
        verify_files(root, &manifest.inputs, cancel, start, timeout)?;
        for name in manifest.files.keys() {
            relative(&directory, name)?;
        }
        for entry in fs::read_dir(&directory).map_err(io_error)? {
            let entry = entry.map_err(io_error)?;
            let name = entry.file_name();
            let name = name.to_str().ok_or("Invalid Motion SDK library name")?;
            if name != "manifest.json" && !manifest.files.contains_key(name) {
                return Err(format!("Unlisted Motion SDK library: {name}; rebuild the SDK").into());
            }
        }
        if manifest.externs.len() != 2 {
            return Err("Motion SDK must expose fframes and fold_motion_fframes".into());
        }
        let mut externs = BTreeMap::new();
        for name in ["fframes", "fold_motion_fframes"] {
            let file = manifest
                .externs
                .get(name)
                .ok_or("Missing Motion SDK extern")?;
            if !manifest.files.contains_key(file)
                || !file.starts_with(&format!("lib{name}-"))
                || !file.ends_with(".rlib")
            {
                return Err("Invalid Motion SDK extern".into());
            }
            externs.insert(name.into(), relative(&directory, file)?);
        }
        if manifest.native_worker != "fold-motion-native-worker" {
            return Err("Invalid native Motion worker".into());
        }
        let native_digest = manifest
            .files
            .get(&manifest.native_worker)
            .ok_or("Missing native Motion worker digest")?
            .clone();
        let native_worker = relative(&directory, &manifest.native_worker)?;
        verify_files(
            &directory,
            &[(manifest.native_worker.clone(), native_digest.clone())].into(),
            cancel,
            start,
            timeout,
        )?;
        let mut hash = Sha256::new();
        hash_part(&mut hash, &body);
        Ok(Self {
            directory,
            externs,
            identity: format!("{:x}", hash.finalize()),
            native_worker,
            native_digest,
            files: manifest.files,
            scripts_verified: AtomicBool::new(false),
        })
    }
    /// Called under the runtime's compiler lock before compiling or reusing a script.
    pub fn verify_scripts(&self, cancel: &Cancel, timeout: Duration) -> Result<(), Failure> {
        cancel.check()?;
        if self.scripts_verified.load(Ordering::Acquire) {
            return Ok(());
        }
        let files = self
            .files
            .iter()
            .filter(|(name, _)| name.as_str() != "fold-motion-native-worker")
            .map(|(name, digest)| (name.clone(), digest.clone()))
            .collect();
        verify_files(&self.directory, &files, cancel, Instant::now(), timeout)?;
        self.scripts_verified.store(true, Ordering::Release);
        Ok(())
    }
}

fn verify_files(
    base: &Path,
    files: &BTreeMap<String, String>,
    cancel: &Cancel,
    start: Instant,
    timeout: Duration,
) -> Result<(), Failure> {
    for (name, expected) in files {
        if file_digest_guarded(&relative(base, name)?, || {
            cancel.check()?;
            if start.elapsed() >= timeout {
                return Err("Motion SDK verification deadline exceeded".into());
            }
            Ok(())
        })? != *expected
        {
            return Err(format!("Motion SDK input changed: {name}; rebuild the SDK").into());
        }
    }
    Ok(())
}
