//! Scoped, identity-preserving source for native compositor records.
use crate::{Composite, Node};
use fold_foundation::ObjectId;
use std::collections::BTreeSet;

impl Composite {
    pub fn source(&self, scope: Option<&[ObjectId]>) -> Result<String, String> {
        let ids = self.source_scope(scope)?;
        let nodes: Vec<_> = self.nodes.iter().filter(|n| ids.contains(&n.id)).collect();
        serde_json::to_string_pretty(&nodes).map_err(|e| e.to_string())
    }
    fn source_scope(&self, scope: Option<&[ObjectId]>) -> Result<BTreeSet<ObjectId>, String> {
        let ids: BTreeSet<_> = scope
            .map(|s| s.iter().copied().collect())
            .unwrap_or_else(|| self.nodes.iter().map(|n| n.id).collect());
        if ids.iter().any(|id| !self.nodes.iter().any(|n| n.id == *id)) {
            return Err("Source scope names a missing compositor node".into());
        }
        if scope.is_some_and(|s| s.len() != ids.len()) {
            return Err("Duplicate node in source scope".into());
        }
        Ok(ids)
    }
    pub fn replace_source(
        &mut self,
        scope: Option<&[ObjectId]>,
        expected: &str,
        source: &str,
    ) -> Result<(), String> {
        if self.source(scope)? != expected {
            return Err("Compositor source conflict".into());
        }
        if source.len() > 1024 * 1024 {
            return Err("Compositor source exceeds 1 MiB".into());
        }
        let ids = self.source_scope(scope)?;
        let nodes: Vec<Node> = serde_json::from_str(source).map_err(|e| e.to_string())?;
        let replacement_ids: BTreeSet<_> = nodes.iter().map(|n| n.id).collect();
        if replacement_ids.len() != nodes.len() {
            return Err("Duplicate node in compositor source".into());
        }
        if scope.is_some() && replacement_ids != ids {
            return Err("Scoped source must preserve its node membership".into());
        }
        let mut next = self.clone();
        if scope.is_none() {
            next.nodes = nodes;
        } else {
            for node in nodes {
                let id = node.id;
                *next.node_mut(id)? = node;
            }
        }
        next.validate()?;
        self.check_protection(&next)?;
        *self = next;
        Ok(())
    }
}
