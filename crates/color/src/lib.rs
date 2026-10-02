//! Explicit, worker-owned CPU color processing through the private OCIO runtime.
//! No implicit `$OCIO`, global runtime discovery, working-directory lookup or
//! fallback transform. External configs must be self-contained directories;
//! their bounded resource closure is copied and content-hashed before loading.
pub mod authored;
mod native;
pub mod settings;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
    rc::Rc,
};

pub const OCIO_VERSION: &str = "2.4.2";
pub const BUNDLED_CONFIG: &str = "studio-config-v2.2.0_aces-v1.3_ocio-v2.4";
pub const BUNDLED_SHA256: &str = "d8b361f76750ebfbedf0ded0b5e4315b283eed5e095814486b9d7c416cfbbb4c";
pub const WORKING_SPACE: &str = "ACEScg";
pub const LINEAR_SRGB: &str = "Linear Rec.709 (sRGB)";
const MAX_RESOURCE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_RESOURCE_FILES: usize = 4096;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisplayTransform {
    pub display: String,
    pub view: String,
    /// An additional config-defined look, applied before the display/view.
    pub look: Option<String>,
}
impl Default for DisplayTransform {
    fn default() -> Self {
        Self {
            display: "sRGB - Display".into(),
            view: "ACES 1.0 - SDR Video".into(),
            look: None,
        }
    }
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Catalog {
    pub spaces: Vec<String>,
    pub displays: BTreeMap<String, Vec<String>>,
    pub looks: Vec<String>,
}
/// Reproducible content identity, not a location or a UI label. Resource keys are
/// package-relative; relocating identical packages does not change identity.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigIdentity {
    pub ocio_version: String,
    pub runtime_sha256: String,
    pub content_sha256: String,
    pub resources: BTreeMap<String, String>,
}

/// Installation layout shared by desktop and CLI. An explicit root is useful
/// for packaging tests; normal callers resolve from the executable, never CWD.
pub struct Runtime {
    api: Rc<native::Api>,
    root: PathBuf,
    identity: String,
}
impl Runtime {
    pub fn installed() -> Result<Self, String> {
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let root = executable
            .parent()
            .and_then(Path::parent)
            .ok_or("invalid executable location")?;
        Self::at(root)
    }
    pub fn at(root: &Path) -> Result<Self, String> {
        if !root.is_absolute() {
            return Err("color installation root must be absolute".into());
        }
        let mut digest = Sha256::new();
        for name in ["libfold_ocio.so", "libOpenColorIO.so.2.4"] {
            digest.update(hash(&read_bounded(
                &root.join("lib/fold").join(name),
                MAX_RESOURCE_BYTES,
            )?));
        }
        Ok(Self {
            api: native::Api::load(&root.join("lib/fold/libfold_ocio.so"))?,
            root: root.to_owned(),
            identity: format!("{:x}", digest.finalize()),
        })
    }
    pub fn bundled(&self) -> Result<Config, String> {
        let path = self.root.join("share/fold/color/config.ocio");
        // Only the serialized built-in config is needed; it has no file LUTs.
        let bytes = read_bounded(&path, MAX_RESOURCE_BYTES)?;
        if hash(&bytes) != BUNDLED_SHA256 {
            return Err("bundled OCIO config checksum mismatch".into());
        }
        let package = tempfile::tempdir().map_err(|e| e.to_string())?;
        fs::write(package.path().join("config.ocio"), &bytes).map_err(|e| e.to_string())?;
        self.load(
            package,
            Path::new("config.ocio"),
            BTreeMap::from([("config.ocio".into(), hash(&bytes))]),
        )
    }
    /// Load an explicit config with all resources under its parent directory.
    /// Absolute/external LUT paths, symlinks and environment substitutions are
    /// rejected rather than silently producing non-reproducible output.
    pub fn external(&self, path: &Path) -> Result<Config, String> {
        if !path.is_absolute() {
            return Err("external OCIO config path must be absolute".into());
        }
        let root = path.parent().ok_or("config has no resource directory")?;
        let name = Path::new(path.file_name().ok_or("config has no filename")?);
        let package = tempfile::tempdir().map_err(|e| e.to_string())?;
        let mut resources = BTreeMap::new();
        let mut remaining = (MAX_RESOURCE_BYTES, MAX_RESOURCE_FILES);
        copy_resources(
            root,
            Path::new(""),
            package.path(),
            &mut resources,
            &mut remaining,
            0,
        )?;
        self.load(package, name, resources)
    }
    fn load(
        &self,
        package: tempfile::TempDir,
        name: &Path,
        resources: BTreeMap<String, String>,
    ) -> Result<Config, String> {
        let mut digest = Sha256::new();
        digest.update(OCIO_VERSION);
        digest.update(&self.identity);
        digest.update(name.to_str().ok_or("config filename must be UTF-8")?);
        for (path, hash) in &resources {
            digest.update((path.len() as u64).to_le_bytes());
            digest.update(path);
            digest.update(hash);
        }
        let identity = ConfigIdentity {
            ocio_version: OCIO_VERSION.into(),
            runtime_sha256: self.identity.clone(),
            content_sha256: format!("{:x}", digest.finalize()),
            resources,
        };
        let config = native::Config::load(self.api.clone(), &package.path().join(name))?;
        Ok(Config {
            native: config,
            package: Rc::new(package),
            identity,
        })
    }
}
fn hash(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, String> {
    use std::io::Read;
    let file = fs::File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("OCIO resource is not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(limit + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > limit {
        return Err("OCIO resource budget exceeded (64 MiB)".into());
    }
    Ok(bytes)
}
fn copy_resources(
    root: &Path,
    relative: &Path,
    destination: &Path,
    hashes: &mut BTreeMap<String, String>,
    remaining: &mut (u64, usize),
    depth: usize,
) -> Result<(), String> {
    if depth > 32 {
        return Err("OCIO resource directory nesting exceeds 32".into());
    }
    for entry in fs::read_dir(root.join(relative)).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        if remaining.1 == 0 {
            return Err("OCIO resource entry count exceeds 4096".into());
        }
        remaining.1 -= 1;
        let path = relative.join(entry.file_name());
        let kind = entry.file_type().map_err(|e| e.to_string())?;
        if kind.is_symlink() {
            return Err("OCIO resource symlinks are unsupported".into());
        }
        if kind.is_dir() {
            fs::create_dir(destination.join(&path)).map_err(|e| e.to_string())?;
            copy_resources(root, &path, destination, hashes, remaining, depth + 1)?;
        } else if kind.is_file() {
            let bytes = read_bounded(&root.join(&path), remaining.0)?;
            remaining.0 -= bytes.len() as u64;
            hashes.insert(
                path.to_str()
                    .ok_or("OCIO resource path must be UTF-8")?
                    .into(),
                hash(&bytes),
            );
            fs::write(destination.join(path), bytes).map_err(|e| e.to_string())?;
        } else {
            return Err("OCIO resource must be a regular file or directory".into());
        }
    }
    Ok(())
}

pub struct Config {
    native: native::Config,
    package: Rc<tempfile::TempDir>,
    identity: ConfigIdentity,
}
impl Config {
    pub fn identity(&self) -> &ConfigIdentity {
        &self.identity
    }
    pub fn catalog(&self) -> Result<Catalog, String> {
        let spaces = self.native.names(0, "")?;
        let displays = self
            .native
            .names(1, "")?
            .into_iter()
            .map(|display| Ok((display.clone(), self.native.names(2, &display)?)))
            .collect::<Result<_, String>>()?;
        Ok(Catalog {
            spaces,
            displays,
            looks: self.native.names(3, "")?,
        })
    }
    pub fn conversion(&self, source: &str, destination: &str) -> Result<Processor, String> {
        self.processor(source, destination, "", "")
    }
    pub fn display(&self, source: &str, transform: &DisplayTransform) -> Result<Processor, String> {
        if transform.view.is_empty() || transform.display.is_empty() {
            return Err("display and view must be explicit".into());
        }
        self.processor(
            source,
            &transform.display,
            &transform.view,
            transform.look.as_deref().unwrap_or(""),
        )
    }
    fn processor(
        &self,
        source: &str,
        destination: &str,
        view: &str,
        look: &str,
    ) -> Result<Processor, String> {
        let native = self.native.processor(source, destination, view, look)?;
        for path in self.native.files(&native)? {
            let path = fs::canonicalize(path).map_err(|e| e.to_string())?;
            if !path.starts_with(self.package.path()) {
                return Err("OCIO LUT escapes the pinned resource package".into());
            }
        }
        // Processor cache ID alone is not sufficient: include config/LUT contents.
        let mut digest = Sha256::new();
        digest.update(&self.identity.content_sha256);
        digest.update(native.id()?);
        Ok(Processor {
            native,
            _package: self.package.clone(),
            source: source.into(),
            config_identity: self.identity.content_sha256.clone(),
            identity: format!("{:x}", digest.finalize()),
        })
    }
}
/// Worker-local processor. Deliberately not Send/Sync: native calls and resource
/// construction stay on the owning worker, rather than behind a UI-thread lock.
pub struct Processor {
    native: native::Processor,
    _package: Rc<tempfile::TempDir>,
    source: String,
    config_identity: String,
    identity: String,
}
impl Processor {
    pub fn source(&self) -> &str {
        &self.source
    }
    pub fn config_identity(&self) -> &str {
        &self.config_identity
    }
    pub fn identity(&self) -> &str {
        &self.identity
    }
    /// Apply to premultiplied RGBA32F. Nonlinear transforms see straight RGB.
    /// Alpha is retained; zero alpha discards hidden RGB. The caller owns the
    /// mutable work buffer and must discard it on error (no partial publication).
    pub fn apply(&self, pixels: &mut [[f32; 4]]) -> Result<(), String> {
        for p in pixels.iter() {
            validate_pixel(*p)?;
        }
        for p in pixels.iter_mut() {
            if p[3] == 0. {
                p[..3].fill(0.);
            } else {
                for c in 0..3 {
                    p[c] /= p[3];
                }
            }
            if p.iter().any(|v| !v.is_finite()) {
                return Err("unpremultiply overflow".into());
            }
        }
        // The native RGB descriptor excludes coverage entirely. Even an OCIO
        // exponent's identity-alpha approximation must not touch this channel.
        self.native.apply(pixels)?;
        for p in pixels {
            let alpha = p[3];
            for value in &mut p[..3] {
                *value *= alpha;
            }
            validate_pixel(*p)?;
        }
        Ok(())
    }
}
pub fn validate_pixel(pixel: [f32; 4]) -> Result<(), String> {
    if pixel.iter().any(|v| !v.is_finite()) || !(0. ..=1.).contains(&pixel[3]) {
        return Err("expected finite RGB and alpha in 0..=1".into());
    }
    Ok(())
}

#[cfg(test)]
mod resource_tests {
    use super::*;

    #[test]
    fn copy_limits_bytes_entries_and_nesting_before_native_load() {
        let source = tempfile::tempdir().unwrap();
        let destination = tempfile::tempdir().unwrap();
        fs::write(source.path().join("lut"), b"12345").unwrap();
        assert!(read_bounded(&source.path().join("lut"), 4).is_err());
        for (mut budget, depth) in [((4, 10), 0), ((10, 0), 0), ((10, 10), 33)] {
            assert!(
                copy_resources(
                    source.path(),
                    Path::new(""),
                    destination.path(),
                    &mut BTreeMap::new(),
                    &mut budget,
                    depth
                )
                .is_err()
            );
        }
        let mut hashes = BTreeMap::new();
        let mut budget = (5, 1);
        copy_resources(
            source.path(),
            Path::new(""),
            destination.path(),
            &mut hashes,
            &mut budget,
            0,
        )
        .unwrap();
        assert_eq!(budget, (0, 0));
        assert_eq!(hashes["lut"], hash(b"12345"));
        fs::write(source.path().join("lut"), b"changed").unwrap();
        assert_eq!(fs::read(destination.path().join("lut")).unwrap(), b"12345");
    }
}
