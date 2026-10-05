//! Machine preferences, independent of authored project and workspace state.
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Application {
    pub automatic: bool,
    pub gpu_mib: u32,
    pub viewer_mib: u32,
    pub ram_mib: u32,
    /// Empty selects the platform cache directory. Used for disposable media scratch.
    pub cache_folder: String,
}
impl Default for Application {
    fn default() -> Self {
        Self {
            automatic: true,
            gpu_mib: 1024,
            viewer_mib: 256,
            ram_mib: 768,
            cache_folder: String::new(),
        }
    }
}
impl Application {
    pub fn validate(&self) -> Result<(), String> {
        if !(128..=65536).contains(&self.gpu_mib)
            || !(32..=16384).contains(&self.viewer_mib)
            || self.viewer_mib > self.gpu_mib / 2
            || !(128..=262144).contains(&self.ram_mib)
        {
            return Err("Viewer cache must use at most half the GPU budget; memory budgets are out of range".into());
        }
        if !self.cache_folder.is_empty() && !std::path::Path::new(&self.cache_folder).is_absolute()
        {
            return Err("Cache folder must be an absolute path".into());
        }
        Ok(())
    }
    pub fn effective(&self) -> Self {
        if self.automatic {
            Self {
                cache_folder: self.cache_folder.clone(),
                ..Self::default()
            }
        } else {
            self.clone()
        }
    }
    pub fn cache_path(&self) -> PathBuf {
        if !self.cache_folder.is_empty() {
            return PathBuf::from(&self.cache_folder);
        }
        std::env::var_os("XDG_CACHE_HOME")
            .filter(|v| !v.is_empty())
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
            .unwrap_or_else(std::env::temp_dir)
            .join("fold")
    }
    /// Startup only; existing leases remain accounted if called after allocation.
    pub fn configure_media(&self) -> Result<(), String> {
        self.validate()?;
        let value = self.effective();
        fold_media::scratch::configure(value.cache_path()).map_err(|e| e.to_string())?;
        fold_media::budget::configure_image_memory(u64::from(value.ram_mib) * 1024 * 1024);
        Ok(())
    }
}
