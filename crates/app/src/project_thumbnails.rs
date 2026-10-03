//! One thumbnail job, one result, 128 cached 128×72 textures (<5 MiB RGBA).
use fold_foundation::AssetId;
use fold_media::Cancel;
use fold_ui::sdk::imgui::{
    self,
    texture::{ManagedTextureId, OwnedTextureData, TextureFormat},
};
use std::{
    collections::BTreeMap,
    sync::mpsc::{self, Receiver},
};

#[derive(Clone, PartialEq, Eq)]
struct Key {
    id: AssetId,
    location: String,
    fingerprint: String,
}
struct Cached {
    key: Key,
    value: Result<ManagedTextureId, String>,
    used: u64,
}
type ResultMessage = (Key, Result<Vec<u8>, String>);
#[derive(Default)]
pub struct Thumbnails {
    cache: BTreeMap<AssetId, Cached>,
    pending: Option<Receiver<ResultMessage>>,
    cancel: Cancel,
    clock: u64,
}
impl Thumbnails {
    pub fn prepare(&mut self, context: &mut imgui::Context) {
        let Some(receiver) = &self.pending else {
            return;
        };
        let (key, pixels) = match receiver.try_recv() {
            Ok(result) => result,
            Err(mpsc::TryRecvError::Empty) => return,
            Err(mpsc::TryRecvError::Disconnected) => {
                self.pending = None;
                return;
            }
        };
        self.pending = None;
        // Capacity pressure is temporary: allow a later browser request to retry
        // instead of retaining it as a permanent source/codec failure.
        if pixels.as_ref().is_err_and(|error| {
            error.contains("budget exhausted") || error.contains("capacity exhausted")
        }) {
            return;
        }
        if let Some(old) = self.cache.remove(&key.id)
            && let Ok(id) = old.value
        {
            let _ = context.remove_texture(id);
        }
        if self.cache.len() >= 128 {
            let oldest = self
                .cache
                .iter()
                .min_by_key(|(_, c)| c.used)
                .map(|(id, _)| *id)
                .unwrap();
            if let Some(old) = self.cache.remove(&oldest)
                && let Ok(id) = old.value
            {
                let _ = context.remove_texture(id);
            }
        }
        let value = pixels
            .and_then(|pixels| {
                OwnedTextureData::from_pixels(TextureFormat::RGBA32, 128, 72, &pixels)
                    .map_err(|e| e.to_string())
            })
            .map(|texture| context.register_texture(texture));
        self.cache.insert(
            key.id,
            Cached {
                key,
                value,
                used: self.clock,
            },
        );
    }
    pub fn get(&mut self, asset: &fold_project::Asset) -> Option<Result<ManagedTextureId, String>> {
        self.clock += 1;
        let key = Key {
            id: asset.id,
            location: asset.location.clone(),
            fingerprint: asset.fingerprint.clone(),
        };
        if let Some(cached) = self.cache.get_mut(&asset.id).filter(|c| c.key == key) {
            cached.used = self.clock;
            return Some(cached.value.clone());
        }
        if self.pending.is_none() {
            let metadata = asset
                .extensions
                .get(fold_media::ingest::METADATA_KEY)
                .cloned();
            let (sender, receiver) = mpsc::sync_channel(1);
            self.cancel = Cancel::default();
            let cancel = self.cancel.clone();
            if std::thread::Builder::new()
                .name("fold-thumbnail".into())
                .spawn(move || {
                    let admission = fold_render::scheduling::Scheduler::shared()
                        .enter(fold_render::scheduling::Class::Background, &cancel);
                    let result = admission.and_then(|_permit| {
                        metadata
                            .ok_or("Source metadata unavailable".to_owned())
                            .and_then(|value| {
                                serde_json::from_value::<fold_media::ingest::SourceMetadata>(value)
                                    .map_err(|e| e.to_string())
                            })
                            .and_then(|metadata| {
                                fold_media::thumbnail::thumbnail(
                                    std::path::Path::new(&key.location),
                                    &key.fingerprint,
                                    &metadata.profile,
                                    &cancel,
                                )
                            })
                    });
                    let _ = sender.send((key, result));
                })
                .is_ok()
            {
                self.pending = Some(receiver);
            }
        }
        None
    }
}
impl Drop for Thumbnails {
    fn drop(&mut self) {
        self.cancel.cancel();
    }
}
