//! Worker-owned disposable playback storage. No render scheduling or feature models.
use crate::{desktop::PreviewKey, presentation::Pixels};
use std::{collections::HashMap, sync::Arc};
mod disk;
pub use disk::Disk;

pub const TRANSFER_LIMIT: usize = 64 * 1024 * 1024;
/// Metadata is separately bounded; byte limits govern retained pixel storage.
pub const ENTRY_LIMIT: usize = 131_072;

pub struct Memory {
    entries: HashMap<PreviewKey, (Arc<Pixels>, u64)>,
    clock: u64,
    bytes: usize,
    budget: usize,
}
impl Memory {
    pub fn new(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
            bytes: 0,
            budget,
        }
    }
    pub fn usage(&self) -> (usize, usize) {
        (self.bytes, self.budget)
    }
    pub fn keys(&self) -> impl Iterator<Item = &PreviewKey> {
        self.entries.keys()
    }
    pub fn get(&mut self, key: &PreviewKey) -> Option<Arc<Pixels>> {
        self.clock += 1;
        let (pixels, age) = self.entries.get_mut(key)?;
        *age = self.clock;
        Some(pixels.clone())
    }
    pub fn insert(&mut self, key: PreviewKey, pixels: Arc<Pixels>) -> Vec<PreviewKey> {
        let size = pixels.data.len();
        let mut evicted = vec![];
        if size > self.budget || self.entries.contains_key(&key) {
            return evicted;
        }
        while self.bytes + size > self.budget || self.entries.len() >= ENTRY_LIMIT {
            let oldest = self
                .entries
                .iter()
                .min_by_key(|(_, (_, age))| *age)
                .unwrap()
                .0
                .clone();
            self.bytes -= self.entries.remove(&oldest).unwrap().0.data.len();
            evicted.push(oldest);
        }
        self.clock += 1;
        self.bytes += size;
        self.entries.insert(key, (pixels, self.clock));
        evicted
    }
    pub fn clear(&mut self) {
        self.entries.clear();
        self.bytes = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    pub(super) fn key(frame: u32) -> PreviewKey {
        PreviewKey {
            region: None,
            target: None,
            output: "video".into(),
            content: "test".into(),
            frame,
            dimensions: [2, 2],
            view: 1,
            channels: Default::default(),
        }
    }
    pub(super) fn pixels() -> Arc<Pixels> {
        Arc::new(Pixels {
            description: crate::presentation::Description {
                dimensions: [2, 2],
                format: crate::presentation::Format::Rgba8,
            },
            data: vec![42; 16],
        })
    }
    #[test]
    fn byte_budget_lru_and_disabled_memory() {
        let mut memory = Memory::new(32);
        memory.insert(key(0), pixels());
        memory.insert(key(1), pixels());
        memory.get(&key(0));
        assert_eq!(memory.insert(key(2), pixels()), vec![key(1)]);
        assert_eq!(memory.usage(), (32, 32));
        let held = memory.get(&key(0)).unwrap();
        memory.clear();
        assert_eq!(held.data, vec![42; 16]);
        let mut disabled = Memory::new(0);
        disabled.insert(key(0), pixels());
        assert!(disabled.get(&key(0)).is_none());
    }
}
