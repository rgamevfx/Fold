//! Byte-bounded LRU policy, independent of GPU APIs for deterministic tests.
use std::{collections::HashMap, hash::Hash};
struct Entry<V> {
    value: V,
    bytes: usize,
    age: u64,
}
pub(crate) struct Cache<K, V> {
    entries: HashMap<K, Entry<V>>,
    clock: u64,
    pub bytes: usize,
    pub budget: usize,
    pub hits: u64,
    pub misses: u64,
}
impl<K: Eq + Hash + Clone, V> Cache<K, V> {
    pub fn new(budget: usize) -> Self {
        Self {
            entries: HashMap::new(),
            clock: 0,
            bytes: 0,
            budget,
            hits: 0,
            misses: 0,
        }
    }
    pub fn get(&mut self, key: &K) -> Option<&V> {
        self.clock += 1;
        if let Some(entry) = self.entries.get_mut(key) {
            entry.age = self.clock;
            self.hits += 1;
            Some(&entry.value)
        } else {
            self.misses += 1;
            None
        }
    }
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.entries.get(key).map(|entry| &entry.value)
    }
    pub fn keys(&self) -> impl Iterator<Item = &K> {
        self.entries.keys()
    }
    pub fn entries_mut(&mut self) -> impl Iterator<Item = (&K, &mut V)> {
        self.entries
            .iter_mut()
            .map(|(key, entry)| (key, &mut entry.value))
    }
    pub fn needs_room(&self, bytes: usize) -> bool {
        bytes > self.budget.saturating_sub(self.bytes)
    }
    pub fn evict(&mut self, eligible: impl Fn(&K, &V) -> bool) -> Option<V> {
        let key = self
            .entries
            .iter()
            .filter(|(key, entry)| eligible(key, &entry.value))
            .min_by_key(|(_, entry)| entry.age)?
            .0
            .clone();
        let entry = self.entries.remove(&key).unwrap();
        self.bytes -= entry.bytes;
        Some(entry.value)
    }
    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        assert!(!self.needs_room(bytes));
        assert!(!self.entries.contains_key(&key));
        self.clock += 1;
        self.bytes += bytes;
        self.entries.insert(
            key,
            Entry {
                value,
                bytes,
                age: self.clock,
            },
        );
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_time_resolution_and_view_are_distinct() {
        use fold_platform::desktop::PreviewKey;
        let key = PreviewKey {
            region: None,
            output: "video".into(),
            target: None,
            content: "content-a".into(),
            frame: 7,
            dimensions: [640, 360],
            view: 1,
            channels: Default::default(),
        };
        let mut cache = Cache::new(16);
        cache.insert(key.clone(), 1, 4);
        for changed in [
            PreviewKey {
                region: Some(fold_platform::desktop::PreviewRegion {
                    x: 10,
                    y: 20,
                    width: 80,
                    height: 60,
                }),
                ..key.clone()
            },
            PreviewKey {
                content: "content-b".into(),
                ..key.clone()
            },
            PreviewKey {
                frame: 8,
                ..key.clone()
            },
            PreviewKey {
                dimensions: [320, 180],
                ..key.clone()
            },
            PreviewKey {
                view: 2,
                ..key.clone()
            },
        ] {
            assert!(cache.get(&changed).is_none());
        }
        assert_eq!(cache.get(&key), Some(&1));
        // Repeated seek/evict loops remain within the byte budget.
        for frame in 0..10000 {
            let next = PreviewKey {
                frame,
                ..key.clone()
            };
            if cache.get(&next).is_some() {
                continue;
            }
            while cache.needs_room(4) {
                cache.evict(|_, _| true).unwrap();
            }
            cache.insert(next, 2, 4);
            assert!(cache.bytes <= cache.budget);
        }
    }

    #[test]
    fn retention_is_not_limited_to_512_small_frames() {
        let mut cache = Cache::new(4096);
        for key in 0..1024 {
            cache.insert(key, (), 4);
        }
        assert_eq!(cache.bytes, 4096);
        assert!(cache.needs_room(1));
    }
    #[test]
    fn lru_byte_budget_and_protected_entries() {
        let mut cache = Cache::new(8);
        cache.insert("first", 1, 4);
        cache.insert("second", 2, 4);
        assert_eq!(cache.get(&"first"), Some(&1));
        assert!(cache.needs_room(4));
        assert_eq!(cache.evict(|key, _| *key != "first"), Some(2));
        assert_eq!(cache.evict(|_, _| false), None);
        cache.insert("third", 3, 4);
        assert_eq!(cache.evict(|_, _| true), Some(1));
        assert_eq!(cache.bytes, 4);
        assert_eq!(cache.get(&"absent"), None);
        assert_eq!((cache.hits, cache.misses), (1, 1));
    }
}
