//! Byte-bounded LRU policy, independent of GPU APIs for deterministic tests.
use std::collections::VecDeque;
pub(crate) struct Entry<K, V> {
    pub key: K,
    pub value: V,
    pub bytes: usize,
}
pub(crate) struct Cache<K, V> {
    entries: VecDeque<Entry<K, V>>,
    pub bytes: usize,
    pub budget: usize,
    pub hits: u64,
    pub misses: u64,
}
impl<K: PartialEq, V> Cache<K, V> {
    pub fn new(budget: usize) -> Self {
        Self {
            entries: VecDeque::new(),
            bytes: 0,
            budget,
            hits: 0,
            misses: 0,
        }
    }
    pub fn get(&mut self, key: &K) -> Option<&V> {
        if let Some(index) = self.entries.iter().position(|e| &e.key == key) {
            let entry = self.entries.remove(index).unwrap();
            self.entries.push_back(entry);
            self.hits += 1;
            Some(&self.entries.back().unwrap().value)
        } else {
            self.misses += 1;
            None
        }
    }
    pub fn peek(&self, key: &K) -> Option<&V> {
        self.entries
            .iter()
            .find(|e| &e.key == key)
            .map(|e| &e.value)
    }
    pub fn entries_mut(&mut self) -> impl Iterator<Item = &mut Entry<K, V>> {
        self.entries.iter_mut()
    }
    pub fn needs_room(&self, bytes: usize) -> bool {
        self.bytes + bytes > self.budget || self.entries.len() >= 512
    }
    pub fn evict(&mut self, eligible: impl Fn(&K, &V) -> bool) -> Option<V> {
        let index = self
            .entries
            .iter()
            .position(|e| eligible(&e.key, &e.value))?;
        let entry = self.entries.remove(index).unwrap();
        self.bytes -= entry.bytes;
        Some(entry.value)
    }
    pub fn insert(&mut self, key: K, value: V, bytes: usize) {
        assert!(!self.needs_room(bytes));
        assert!(self.peek(&key).is_none());
        self.bytes += bytes;
        self.entries.push_back(Entry { key, value, bytes });
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn content_time_resolution_and_view_are_distinct() {
        use fold_platform::desktop::PreviewKey;
        let key = PreviewKey {
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
