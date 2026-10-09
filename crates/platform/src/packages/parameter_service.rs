//! Snapshot-scoped parameter discovery and placement-clock dispatch.
use super::PackageRegistry;
use fold_foundation::Time;
use fold_media::Cancel;
use fold_project::{Document, Snapshot};
use std::collections::BTreeSet;

impl PackageRegistry {
    /// Project paths start with the current document name. Bins are organization
    /// only and do not alter evaluation scope. Duplicate names stay ambiguous.
    pub fn project_parameters(
        &self,
        snapshot: &Snapshot,
        cancel: &Cancel,
    ) -> Result<Vec<crate::parameters::Descriptor>, String> {
        let mut result = vec![];
        for document in snapshot.state().documents.values() {
            cancel.check()?;
            if !self.supports(document) {
                continue;
            }
            result.extend(self.document_parameters(snapshot, document, cancel)?);
            if result.len() > 131072 {
                return Err("Project parameter catalog budget exceeded".into());
            }
        }
        Ok(result)
    }

    pub fn document_parameters(
        &self,
        snapshot: &Snapshot,
        document: &Document,
        cancel: &Cancel,
    ) -> Result<Vec<crate::parameters::Descriptor>, String> {
        let name = |id| {
            snapshot
                .state()
                .organization
                .items
                .iter()
                .find(|item| item.id == fold_project::ItemId::Document(id))
                .map(|item| item.name.clone())
                .unwrap_or_else(|| id.to_string())
        };
        let mut result = vec![];
        let mut pending = vec![(document.id, vec![name(document.id)], vec![], vec![], true)];
        let mut occurrences = 0;
        while let Some((id, prefix, owners, mut ancestors, editable)) = pending.pop() {
            occurrences += 1;
            if occurrences > 131072 {
                return Err("Parameter source occurrence budget exceeded".into());
            }
            cancel.check()?;
            if ancestors.contains(&id) || owners.len() > 64 {
                return Err("Parameter source document cycle or depth limit".into());
            }
            ancestors.push(id);
            let current = snapshot
                .state()
                .documents
                .get(&id)
                .ok_or("Missing parameter source document")?;
            if !self.supports(current) {
                continue;
            }
            for mut entry in if self.has_capability(current, super::Capability::Parameters) {
                self.parameters(current)?
            } else {
                vec![]
            } {
                entry.path.splice(0..0, prefix.clone());
                entry.editable &= editable;
                if !owners.is_empty() {
                    entry.target.placement = Some(crate::parameters::Placement {
                        root: document.id,
                        owners: owners.clone(),
                    });
                }
                result.push(entry);
                if result.len() > 131072 {
                    return Err("Parameter catalog budget exceeded".into());
                }
            }
            for source in if self.has_capability(current, super::Capability::ParameterSources) {
                self.parameter_sources(current)?
            } else {
                vec![]
            }
            .into_iter()
            .rev()
            {
                if pending.len() >= 131072 {
                    return Err("Parameter source occurrence budget exceeded".into());
                }
                self.parameter_source_document(snapshot, &source.reference)?;
                let mut path = prefix.clone();
                path.extend(source.path);
                path.push(name(source.reference.document));
                let mut chain = owners.clone();
                chain.push(source.owner);
                pending.push((
                    source.reference.document,
                    path,
                    chain,
                    ancestors.clone(),
                    editable && source.editable,
                ));
            }
        }
        Ok(result)
    }

    pub fn parameters(
        &self,
        document: &Document,
    ) -> Result<Vec<crate::parameters::Descriptor>, String> {
        if !self.supports(document) {
            return Err("Unavailable parameter schema".into());
        }
        let entries = self
            .documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("Unavailable parameter provider")?
            .parameters_capability()
            .ok_or("Unsupported Parameters capability")?
            .parameters(document)?;
        if entries.len() > 131072 {
            return Err("Parameter catalog budget exceeded".into());
        }
        let mut targets = BTreeSet::new();
        for entry in &entries {
            if entry.target.document != document.id
                || entry.target.placement.is_some()
                || !targets.insert(entry.target.clone())
            {
                return Err("Parameter provider returned an invalid or duplicate target".into());
            }
        }
        Ok(entries)
    }

    pub fn sample_placed_parameter(
        &self,
        snapshot: &Snapshot,
        target: &crate::parameters::Target,
        time: Time,
        cancel: &Cancel,
    ) -> Result<fold_graph::types::Literal, String> {
        let mut time = time;
        let mut document_id = target.document;
        if let Some(placement) = &target.placement {
            if placement.owners.is_empty() || placement.owners.len() > 64 {
                return Err("Invalid parameter placement depth".into());
            }
            document_id = placement.root;
            let mut visited = BTreeSet::new();
            for owner in &placement.owners {
                cancel.check()?;
                if !visited.insert(document_id) {
                    return Err("Parameter source document cycle".into());
                }
                let document = snapshot
                    .state()
                    .documents
                    .get(&document_id)
                    .ok_or("Missing placement document")?;
                let source = self
                    .parameter_sources(document)?
                    .into_iter()
                    .find(|s| s.owner == *owner)
                    .ok_or("Missing parameter placement")?;
                time = source.at(time)?;
                document_id = source.reference.document;
                let child = self.parameter_source_document(snapshot, &source.reference)?;
                let provider = self
                    .documents
                    .iter()
                    .find(|p| {
                        p.package_id() == child.package_id
                            && p.type_id() == child.type_id
                            && p.supports_schema(child.schema_version)
                    })
                    .ok_or("Unavailable source parameter provider")?;
                time = provider
                    .parameters_capability()
                    .ok_or("Unsupported Parameters capability")?
                    .parameter_time(child, time)?;
            }
            if document_id != target.document {
                return Err("Parameter placement changed its source document".into());
            }
            if visited.contains(&document_id) {
                return Err("Parameter source document cycle".into());
            }
        }
        let document = snapshot
            .state()
            .documents
            .get(&document_id)
            .ok_or("Missing parameter document")?;
        let mut local = target.clone();
        local.placement = None;
        self.sample_parameter(document, &local, time, cancel)
    }

    fn parameter_source_document<'a>(
        &self,
        snapshot: &'a Snapshot,
        reference: &fold_project::DocumentRef,
    ) -> Result<&'a Document, String> {
        let document = snapshot
            .state()
            .documents
            .get(&reference.document)
            .ok_or("Missing parameter source document")?;
        if self.supports(document)
            && !self
                .document_outputs(document)
                .iter()
                .any(|output| output.reference.output == reference.output && output.info.is_ok())
        {
            return Err("Unavailable parameter source output".into());
        }
        Ok(document)
    }

    pub fn parameter_sources(
        &self,
        document: &Document,
    ) -> Result<Vec<crate::parameters::Source>, String> {
        let provider = self
            .documents
            .iter()
            .find(|p| {
                p.package_id() == document.package_id
                    && p.type_id() == document.type_id
                    && p.supports_schema(document.schema_version)
            })
            .ok_or("Unavailable parameter source provider")?;
        let sources = provider
            .parameter_sources_capability()
            .ok_or("Unsupported ParameterSources capability")?
            .parameter_sources(document)?;
        let mut owners = BTreeSet::new();
        for source in &sources {
            if !owners.insert(source.owner) || !document.dependencies.contains(&source.reference) {
                return Err("Invalid or undeclared parameter source".into());
            }
        }
        Ok(sources)
    }

    pub fn sample_parameter(
        &self,
        document: &Document,
        target: &crate::parameters::Target,
        time: Time,
        cancel: &Cancel,
    ) -> Result<fold_graph::types::Literal, String> {
        cancel.check()?;
        if !self.supports(document) {
            return Err("Unavailable parameter schema".into());
        }
        if target.placement.is_some() {
            return Err("Placed parameters require a project snapshot".into());
        }
        if target.document != document.id {
            return Err("Parameter belongs to a different document".into());
        }
        let provider = self
            .documents
            .iter()
            .find(|p| p.package_id() == document.package_id && p.type_id() == document.type_id)
            .ok_or("Unavailable parameter provider")?;
        let value = provider
            .parameters_capability()
            .ok_or("Unsupported Parameters capability")?
            .sample_parameter(document, target, time, cancel)?;
        value.validate()?;
        cancel.check()?;
        Ok(value)
    }
}
