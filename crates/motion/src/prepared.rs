//! Small package-owned cache of validated authoring roots. It stores no
//! time-dependent evaluation results and holds no lock while parsing/evaluating.
use crate::Motion;
use fold_project::Document;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

const MAX_ROOTS: usize = 4;
const MAX_PAYLOAD_BYTES: usize = 8 * 1024 * 1024;

pub(super) struct Prepared {
    pub motion: Motion,
    pub identity: Vec<u8>,
}
#[derive(Default)]
pub(super) struct Cache(Mutex<VecDeque<(Document, Arc<Prepared>)>>);
impl Cache {
    pub fn get(&self, document: &Document) -> Result<Arc<Prepared>, String> {
        {
            let mut entries = self
                .0
                .lock()
                .map_err(|_| "motion preparation cache poisoned")?;
            if let Some(index) = entries.iter().position(|(key, _)| key == document) {
                let entry = entries.remove(index).unwrap();
                let result = entry.1.clone();
                entries.push_back(entry);
                return Ok(result);
            }
        }
        let motion = Motion::from_document(document)?;
        let mut semantic = motion.clone();
        for node in &mut semantic.graph.nodes {
            node.position = [0.; 2];
        }
        semantic.graph.nodes.sort_by_key(|n| n.id);
        for group in semantic.groups.values_mut() {
            for node in &mut group.graph.nodes {
                node.position = [0.; 2];
            }
            group.graph.nodes.sort_by_key(|n| n.id);
        }
        let mut identity = super::BUILD.as_bytes().to_vec();
        identity.extend(serde_json::to_vec(&semantic).map_err(|e| e.to_string())?);
        let result = Arc::new(Prepared { motion, identity });
        // This bounds retained source payloads, not an exact allocator byte count.
        // Large valid documents still evaluate, but are not retained here.
        if document.payload.len() > MAX_PAYLOAD_BYTES {
            return Ok(result);
        }
        let mut entries = self
            .0
            .lock()
            .map_err(|_| "motion preparation cache poisoned")?;
        if let Some((_, prepared)) = entries.iter().find(|(key, _)| key == document) {
            return Ok(prepared.clone());
        }
        while entries.len() >= MAX_ROOTS
            || entries.iter().map(|(d, _)| d.payload.len()).sum::<usize>() + document.payload.len()
                > MAX_PAYLOAD_BYTES
        {
            entries.pop_front();
        }
        entries.push_back((document.clone(), result.clone()));
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn roots_are_reused_invalidated_and_evicted_without_mutating_retained_snapshots() {
        let cache = Cache::default();
        let mut document = Motion::empty().document(Default::default()).unwrap();
        let first = cache.get(&document).unwrap();
        assert!(Arc::ptr_eq(&first, &cache.get(&document).unwrap()));
        let mut changed = first.motion.clone();
        changed.info.frames += 1;
        document = changed.document(document.id).unwrap();
        let next = cache.get(&document).unwrap();
        assert!(!Arc::ptr_eq(&first, &next));
        assert_eq!(first.motion.info.frames + 1, next.motion.info.frames);
        for _ in 0..MAX_ROOTS {
            let other = Motion::empty().document(Default::default()).unwrap();
            cache.get(&other).unwrap();
        }
        assert_eq!(cache.0.lock().unwrap().len(), MAX_ROOTS);
        assert!(!Arc::ptr_eq(&next, &cache.get(&document).unwrap()));
        let mut invalid = document;
        invalid.schema_version = 5;
        assert!(cache.get(&invalid).is_err());
    }
}
