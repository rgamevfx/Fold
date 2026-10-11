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
    pub viewer_ram_mib: u32,
    pub timeline_disk_gib: u32,
    pub composition_disk_gib: u32,
    pub timeline_cache_folder: String,
    pub composition_cache_folder: String,
    /// Empty selects the platform cache directory. Used for disposable media scratch.
    pub cache_folder: String,
}
impl Default for Application {
    fn default() -> Self {
        Self {
            automatic: true,
            gpu_mib: 1024,
            viewer_mib: 256,
            ram_mib: (fold_media::budget::automatic_image_memory() / (1024 * 1024)) as u32,
            viewer_ram_mib: 1024,
            timeline_disk_gib: 0,
            composition_disk_gib: 0,
            timeline_cache_folder: String::new(),
            composition_cache_folder: String::new(),
            cache_folder: String::new(),
        }
    }
}
impl Application {
    pub fn validate(&self) -> Result<(), String> {
        if !(128..=65536).contains(&self.gpu_mib)
            || !(32..=32768).contains(&self.viewer_mib)
            || self.viewer_mib > self.gpu_mib / 2
            || !(128..=262144).contains(&self.ram_mib)
            || self.viewer_ram_mib > 262144
            || self.timeline_disk_gib > 1048576
            || self.composition_disk_gib > 1048576
        {
            return Err("Viewer cache must use at most half the GPU budget; memory budgets are out of range".into());
        }
        if [
            &self.cache_folder,
            &self.timeline_cache_folder,
            &self.composition_cache_folder,
        ]
        .iter()
        .any(|path| !path.is_empty() && !std::path::Path::new(path).is_absolute())
        {
            return Err("Cache folder must be an absolute path".into());
        }
        Ok(())
    }
    pub fn effective(&self) -> Self {
        if self.automatic {
            Self {
                cache_folder: self.cache_folder.clone(),
                viewer_ram_mib: self.viewer_ram_mib,
                timeline_disk_gib: self.timeline_disk_gib,
                composition_disk_gib: self.composition_disk_gib,
                timeline_cache_folder: self.timeline_cache_folder.clone(),
                composition_cache_folder: self.composition_cache_folder.clone(),
                ..Self::default()
            }
        } else {
            self.clone()
        }
    }
    pub fn playback_path(&self, timeline: bool) -> PathBuf {
        let folder = if timeline {
            &self.timeline_cache_folder
        } else {
            &self.composition_cache_folder
        };
        let root = if folder.is_empty() {
            self.cache_path()
        } else {
            PathBuf::from(folder)
        };
        root.join("playback-v1")
            .join(if timeline { "timeline" } else { "composition" })
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
