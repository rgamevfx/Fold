//! Bounded, disposable simulation checkpoints. No authored or UI state lives here.
use crate::{
    fields::Budget,
    geometry::{Content, Geometry},
};
use fold_foundation::ObjectId;
use std::{
    collections::VecDeque,
    sync::{Arc, Mutex},
};

#[derive(Clone, PartialEq, Eq)]
pub(crate) struct Key {
    pub identity: Arc<Vec<u8>>,
    pub timing: Option<fold_animation::timing::Range>,
    pub format: Option<fold_platform::format::Format>,
    pub scope: Vec<ObjectId>,
    pub object: Option<ObjectId>,
    pub node: ObjectId,
}
struct Checkpoint {
    key: Key,
    step: i64,
    state: Content,
    bytes: usize,
}
/// Shared by a package provider or a headless caller. Eviction affects work, never results.
pub struct Runtime {
    limit: usize,
    entries: Mutex<VecDeque<Checkpoint>>,
}
impl Default for Runtime {
    fn default() -> Self {
        Self::new(64 * 1024 * 1024)
    }
}
impl Runtime {
    pub fn new(byte_limit: usize) -> Self {
        Self {
            limit: byte_limit,
            entries: Mutex::new(VecDeque::new()),
        }
    }
    pub fn clear(&self) {
        if let Ok(mut entries) = self.entries.lock() {
            entries.clear();
        }
    }
    pub fn retained_bytes(&self) -> usize {
        self.entries
            .lock()
            .map(|entries| entries.iter().map(|c| c.bytes).sum())
            .unwrap_or(0)
    }
    pub(crate) fn nearest(&self, key: &Key, step: i64) -> Option<(i64, Content)> {
        let mut entries = self.entries.lock().ok()?;
        let index = entries
            .iter()
            .enumerate()
            .filter(|(_, c)| c.key == *key && c.step <= step)
            .max_by_key(|(_, c)| c.step)?
            .0;
        let entry = entries.remove(index)?;
        let result = (entry.step, entry.state.clone());
        entries.push_back(entry);
        Some(result)
    }
    pub(crate) fn insert(&self, key: &Key, step: i64, state: &Content, state_bytes: usize) {
        let bytes = state_bytes.saturating_add(key.identity.len());
        if bytes > self.limit {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        entries.retain(|c| !(c.key == *key && c.step == step));
        let mut retained: usize = entries.iter().map(|c| c.bytes).sum();
        while retained.saturating_add(bytes) > self.limit || entries.len() >= 128 {
            if let Some(old) = entries.pop_front() {
                retained -= old.bytes;
            } else {
                break;
            }
        }
        entries.push_back(Checkpoint {
            key: key.clone(),
            step,
            state: state.clone(),
            bytes,
        });
    }
}

/// Conservatively charge shared geometry each time it occurs, and bound depth/work.
/// This is a retained payload budget, not an allocator/driver memory measurement.
pub(crate) fn state_bytes(content: &Content, budget: &mut Budget) -> Result<usize, String> {
    fn walk(content: &Content, budget: &mut Budget, depth: usize) -> Result<usize, String> {
        if depth > 32 || content.len() > 16384 {
            return Err("solver state exceeds geometry limits".into());
        }
        let mut bytes = std::mem::size_of::<Vec<crate::geometry::Element>>();
        let mut ids = std::collections::BTreeSet::new();
        for element in content.iter() {
            budget.spend()?;
            if !ids.insert(element.id) {
                return Err(
                    "solver state requires unique element IDs within each collection".into(),
                );
            }
            if !element.transform.iter().all(|v| v.is_finite()) || !element.opacity.is_finite() {
                return Err("nonfinite solver state".into());
            }
            if element.effects.is_some() {
                return Err("solver state must precede image effects".into());
            }
            bytes = bytes.saturating_add(std::mem::size_of_val(element));
            for (name, value) in &element.attributes {
                value.validate()?;
                bytes = bytes.saturating_add(
                    name.len()
                        + 128
                        + match value {
                            crate::fields::Datum::Text(t) => t.len(),
                            _ => 0,
                        },
                );
            }
            bytes = bytes.saturating_add(
                serde_json::to_vec(&element.path_attributes)
                    .map_err(|e| e.to_string())?
                    .len(),
            );
            bytes = bytes.saturating_add(match &element.geometry {
                Geometry::Path(path) | Geometry::Glyph { path, .. } => path
                    .len()
                    .saturating_mul(std::mem::size_of::<fold_render::vector::Segment>()),
                Geometry::Group(children) | Geometry::Instance(children) => {
                    walk(children, budget, depth + 1)?
                }
                Geometry::Image(_) => return Err("Solver state cannot retain render-dependent image geometry; apply images after the Solver".into()),
                Geometry::Point => 0,
            });
            if bytes > 64 * 1024 * 1024 {
                return Err("solver state exceeds 64 MiB payload limit".into());
            }
        }
        Ok(bytes)
    }
    walk(content, budget, 0)
}
