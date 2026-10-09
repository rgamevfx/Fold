//! Authoritative native construction, independent of editor and renderer state.
#[cfg(test)]
use crate::types::Clock;
use crate::validation::{Diagnostic, Kind};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_project::{Document, Metadata, Revision};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const MAX_DOCUMENT_BYTES: usize = 16 * 1024 * 1024;
pub const MAX_OBJECTS: usize = 32_768;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Construction {
    pub seed: u64,
    pub canvas: Canvas,
    pub definitions: Vec<Definition>,
    pub root: Instance,
    #[serde(default)]
    pub presentation: Presentation,
    #[serde(flatten)]
    pub extensions: Metadata,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Canvas {
    pub width: u32,
    pub height: u32,
    pub rate: [u32; 2],
    pub start: Time,
    pub end: Time,
    #[serde(flatten)]
    pub extensions: Metadata,
}
use fold_graph::model::extension_keys;
pub use fold_graph::model::{
    Definition, Edge, Group, Input, Instance, Node, Output, Parameter, Presentation, Reference,
};

impl Construction {
    /// Structural persistence validation deliberately does not execute or resolve operators.
    /// Unknown operations survive a save; semantic qualification happens in the registry.
    pub fn validate_structure(&self) -> Result<(), String> {
        self.validate_structure_detailed()
            .map_err(|error| error.message)
    }
    pub fn validate_structure_detailed(&self) -> Result<(), Diagnostic> {
        self.structural_ids().map(|_| ())
    }
    pub(crate) fn structural_ids(&self) -> Result<BTreeSet<ObjectId>, Diagnostic> {
        extension_keys(
            &self.extensions,
            &["seed", "canvas", "definitions", "root", "presentation"],
        )?;
        extension_keys(
            &self.canvas.extensions,
            &["width", "height", "rate", "start", "end"],
        )?;
        extension_keys(
            &self.presentation.extensions,
            &["positions", "groups", "annotations"],
        )?;
        if self.canvas.width == 0
            || self.canvas.height == 0
            || self.canvas.width > 16_384
            || self.canvas.height > 16_384
            || self.canvas.rate.contains(&0)
            || self.canvas.start >= self.canvas.end
        {
            return Err("Invalid canvas, frame rate or time range".into());
        }
        let mut ids = BTreeSet::new();
        let mut take = |id| -> Result<(), Diagnostic> {
            if !ids.insert(id) {
                return Err(Diagnostic {
                    kind: Kind::Type,
                    message: format!("Duplicate native identity {id:?}"),
                    native_ids: vec![id],
                });
            }
            if ids.len() > MAX_OBJECTS {
                return Err(Diagnostic {
                    kind: Kind::Budget,
                    message: "Native object budget exceeded".into(),
                    native_ids: vec![id],
                });
            }
            Ok(())
        };
        let mut definition_names = BTreeSet::new();
        for definition in &self.definitions {
            extension_keys(
                &definition.extensions,
                &["id", "name", "inputs", "nodes", "instances", "outputs"],
            )?;
            take(definition.id)?;
            name(&definition.name)?;
            if !definition_names.insert(&definition.name) {
                return Err("Duplicate definition name".into());
            }
            let mut names = BTreeSet::new();
            for input in &definition.inputs {
                extension_keys(
                    &input.extensions,
                    &["id", "name", "ty", "default", "protected"],
                )?;
                take(input.id)?;
                name(&input.name)?;
                if !names.insert(&input.name) {
                    return Err("Duplicate binding name".into());
                }
                input.ty.validate()?;
                input.default.validate()?;
                if input.ty != input.default.ty() {
                    return Err(Diagnostic {
                        kind: Kind::Type,
                        message: "Input default does not match its declared type".into(),
                        native_ids: vec![input.id],
                    });
                }
            }
            for node in &definition.nodes {
                extension_keys(
                    &node.extensions,
                    &["id", "name", "operation", "version", "mode", "parameters"],
                )?;
                take(node.id)?;
                name(&node.name)?;
                if !names.insert(&node.name) {
                    return Err("Duplicate binding name".into());
                }
                if node.operation.is_empty()
                    || node.operation.len() > 256
                    || node.version == 0
                    || node.mode.len() > 64
                {
                    return Err("Invalid operation identity/version/mode".into());
                }
                for (key, parameter) in &node.parameters {
                    name(key)?;
                    parameter_extensions(parameter)?;
                    take(parameter.id)?;
                }
            }
            for instance in &definition.instances {
                take(instance.id)?;
                name(&instance.name)?;
                if !names.insert(&instance.name) {
                    return Err("Duplicate binding name".into());
                }
                for parameter in instance.overrides.values() {
                    parameter_extensions(parameter)?;
                    take(parameter.id)?;
                }
            }
            let mut outputs = BTreeSet::new();
            for output in &definition.outputs {
                extension_keys(&output.extensions, &["id", "name", "ty", "value"])?;
                take(output.id)?;
                output.value.validate()?;
                take(output.value.id)?;
                name(&output.name)?;
                output.ty.validate()?;
                if !outputs.insert(&output.name) {
                    return Err("Duplicate output name".into());
                }
            }
        }
        take(self.root.id)?;
        name(&self.root.name)?;
        for parameter in self.root.overrides.values() {
            parameter_extensions(parameter)?;
            take(parameter.id)?;
        }
        for group in &self.presentation.groups {
            extension_keys(&group.extensions, &["id", "name", "members", "locked"])?;
            take(group.id)?;
            name(&group.name)?;
        }
        // Groups may reference groups, but must not form cycles or reference missing IDs.
        for group in &self.presentation.groups {
            let mut seen = BTreeSet::new();
            for member in &group.members {
                if !ids.contains(member) || !seen.insert(member) {
                    return Err("Invalid group membership".into());
                }
            }
        }
        for (id, position) in &self.presentation.positions {
            if !ids.contains(id) || position.iter().any(|v| !v.is_finite()) {
                return Err("Invalid graph position".into());
            }
        }
        for (id, text) in &self.presentation.annotations {
            if !ids.contains(id) || text.len() > 64 * 1024 {
                return Err("Invalid annotation".into());
            }
        }
        for definition in &self.definitions {
            for instance in &definition.instances {
                self.validate_instance(instance)?;
            }
        }
        self.validate_instance(&self.root)?;
        let dependencies: BTreeMap<_, Vec<_>> = self
            .definitions
            .iter()
            .map(|d| (d.id, d.instances.iter().map(|i| i.definition).collect()))
            .collect();
        reject_cycles(&dependencies)?;
        let group_ids: BTreeSet<_> = self.presentation.groups.iter().map(|g| g.id).collect();
        let groups = self
            .presentation
            .groups
            .iter()
            .map(|g| {
                (
                    g.id,
                    g.members
                        .iter()
                        .filter(|id| group_ids.contains(id))
                        .copied()
                        .collect(),
                )
            })
            .collect();
        reject_cycles(&groups)?;
        Ok(ids)
    }
    fn validate_instance(&self, instance: &Instance) -> Result<(), Diagnostic> {
        extension_keys(
            &instance.extensions,
            &["id", "name", "definition", "clock", "overrides"],
        )?;
        let definition = self
            .definition(instance.definition)
            .map_err(|message| Diagnostic {
                kind: Kind::MissingReference,
                message,
                native_ids: vec![instance.id, instance.definition],
            })?;
        if instance
            .overrides
            .keys()
            .any(|id| !definition.inputs.iter().any(|input| input.id == *id))
        {
            return Err("Override does not identify a definition input".into());
        }
        Ok(())
    }
    pub fn definition(&self, id: ObjectId) -> Result<&Definition, String> {
        self.definitions
            .iter()
            .find(|d| d.id == id)
            .ok_or_else(|| format!("Missing definition {id:?}"))
    }
    pub fn document(&self, id: DocumentId) -> Result<Document, String> {
        self.validate_structure()?;
        let payload = serde_json::to_vec(self).map_err(|e| e.to_string())?;
        if payload.len() > MAX_DOCUMENT_BYTES {
            return Err("Native construction exceeds 16 MiB".into());
        }
        Ok(Document {
            id,
            package_id: crate::PACKAGE.into(),
            type_id: crate::CONSTRUCTION.into(),
            schema_version: crate::SCHEMA_VERSION,
            revision: Revision::default(),
            dependencies: vec![],
            assets: vec![],
            payload,
            extensions: Metadata::default(),
        })
    }
    /// Edits preserve project-owned identity, revision and unknown wrapper metadata.
    pub fn replace_document(&self, original: &Document) -> Result<Document, String> {
        if original.package_id != crate::PACKAGE
            || original.type_id != crate::CONSTRUCTION
            || original.schema_version != crate::SCHEMA_VERSION
        {
            return Err("Cannot reinterpret an unsupported document".into());
        }
        Self::from_document(original)?;
        let mut changed = self.document(original.id)?;
        changed.extensions = original.extensions.clone();
        changed.revision = original.revision;
        Ok(changed)
    }
    pub fn from_document(document: &Document) -> Result<Self, String> {
        if document.package_id != crate::PACKAGE
            || document.type_id != crate::CONSTRUCTION
            || document.schema_version != crate::SCHEMA_VERSION
        {
            return Err("Unsupported native Motion document; payload remains opaque".into());
        }
        if document.payload.len() > MAX_DOCUMENT_BYTES {
            return Err("Native construction exceeds 16 MiB".into());
        }
        if !document.dependencies.is_empty() || !document.assets.is_empty() {
            return Err(
                "External resources are not yet supported by this native schema implementation"
                    .into(),
            );
        }
        let value: Self = serde_json::from_slice(&document.payload).map_err(|e| e.to_string())?;
        value.validate_structure()?;
        Ok(value)
    }
}

fn parameter_extensions(parameter: &Parameter) -> Result<(), String> {
    extension_keys(&parameter.extensions, &["id", "value", "protected"])
}
fn name(value: &str) -> Result<(), String> {
    if value.is_empty() || value.len() > 256 || value.chars().any(char::is_control) {
        return Err("Invalid authored name".into());
    }
    Ok(())
}
/// Bounded DFS shared by native dependency and presentation validation. Reports the cycle IDs.
fn reject_cycles(edges: &BTreeMap<ObjectId, Vec<ObjectId>>) -> Result<(), Diagnostic> {
    fn visit(
        id: ObjectId,
        edges: &BTreeMap<ObjectId, Vec<ObjectId>>,
        active: &mut Vec<ObjectId>,
        done: &mut BTreeSet<ObjectId>,
    ) -> Result<(), Diagnostic> {
        if done.contains(&id) {
            return Ok(());
        }
        if let Some(start) = active.iter().position(|v| *v == id) {
            let mut cycle = active[start..].to_vec();
            cycle.push(id);
            return Err(Diagnostic {
                kind: Kind::Cycle,
                message: format!("Cyclic native dependency: {cycle:?}"),
                native_ids: cycle,
            });
        }
        if active.len() >= 256 {
            return Err(Diagnostic {
                kind: Kind::Budget,
                message: "Native dependency depth exceeds 256".into(),
                native_ids: vec![id],
            });
        }
        active.push(id);
        for next in edges.get(&id).into_iter().flatten() {
            visit(*next, edges, active, done)?;
        }
        active.pop();
        done.insert(id);
        Ok(())
    }
    let mut done = BTreeSet::new();
    for id in edges.keys() {
        visit(*id, edges, &mut vec![], &mut done)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn fixture() -> Construction {
        let id = ObjectId::new();
        Construction {
            seed: 1,
            canvas: Canvas {
                width: 1920,
                height: 1080,
                rate: [24, 1],
                start: Time::ZERO,
                end: Time::new(4, 1).unwrap(),
                extensions: Metadata::default(),
            },
            definitions: vec![Definition {
                id,
                name: "Title".into(),
                inputs: vec![],
                nodes: vec![],
                instances: vec![],
                outputs: vec![],
                extensions: Metadata::default(),
            }],
            root: Instance {
                id: ObjectId::new(),
                name: "main".into(),
                definition: id,
                clock: Clock::default(),
                overrides: BTreeMap::new(),
                extensions: Metadata::default(),
            },
            presentation: Presentation::default(),
            extensions: Metadata::default(),
        }
    }
    #[test]
    fn unknown_operator_parameters_and_extensions_round_trip_without_resolution() {
        let mut c = fixture();
        let node = Node {
            id: ObjectId::new(),
            name: "future".into(),
            operation: "vendor.future".into(),
            version: 73,
            mode: "unresolved".into(),
            parameters: [(
                "unknown".into(),
                Parameter {
                    id: ObjectId::new(),
                    value: serde_json::json!({"future_syntax":[1,null,{"keep":true}]}),
                    protected: false,
                    extensions: Metadata::default(),
                },
            )]
            .into(),
            extensions: [("vendor.extra".into(), serde_json::json!([1, 2, 3]))].into(),
        };
        c.presentation.positions.insert(node.id, [20., 40.]);
        c.definitions[0].nodes.push(node);
        c.extensions
            .insert("new_top_level".into(), serde_json::json!({"retain":true}));
        let document = c.document(DocumentId::new()).unwrap();
        assert_eq!(Construction::from_document(&document).unwrap(), c);
        let mut old = document;
        old.package_id = "fold.motion".into();
        assert!(Construction::from_document(&old).is_err());
    }
    #[test]
    fn native_edits_save_reopen_and_undo_preserve_identity_and_wrapper_extensions() {
        use fold_project::{EditBatch, Mutation, Project};
        let id = DocumentId::new();
        let mut document = fixture().document(id).unwrap();
        document.extensions.insert(
            "vendor.wrapper".into(),
            serde_json::json!({"keep":[1,null]}),
        );
        let mut project = Project::new(8);
        project
            .commit(EditBatch {
                base: project.snapshot().revision(),
                mutations: vec![Mutation::PutDocument(document)],
            })
            .unwrap();
        let before = project.snapshot();
        let original = &before.state().documents[&id];
        let mut construction = Construction::from_document(original).unwrap();
        let identity = construction.root.id;
        construction.root.name = "Renamed instance".into();
        construction
            .presentation
            .positions
            .insert(identity, [120., 80.]);
        let changed = construction.replace_document(original).unwrap();
        assert_eq!(changed.extensions, original.extensions);
        project
            .commit(EditBatch {
                base: before.revision(),
                mutations: vec![Mutation::PutDocument(changed)],
            })
            .unwrap();
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("native.fold");
        fold_project::save(&project.snapshot(), &path).unwrap();
        let reopened = fold_project::load(&path, 8).unwrap();
        assert_eq!(
            Construction::from_document(&reopened.snapshot().state().documents[&id]).unwrap(),
            construction
        );
        project.undo().unwrap();
        assert_eq!(
            project.snapshot().state().documents[&id].payload,
            original.payload
        );
        project.redo().unwrap();
        assert_eq!(
            Construction::from_document(&project.snapshot().state().documents[&id])
                .unwrap()
                .root
                .id,
            identity
        );
        assert_eq!(
            project.snapshot().state().documents[&id].extensions,
            original.extensions
        );
    }
    #[test]
    fn duplicate_identity_and_recursive_assets_are_rejected() {
        let mut c = fixture();
        c.root.id = c.definitions[0].id;
        assert!(c.validate_structure().unwrap_err().contains("Duplicate"));
        c.root.id = ObjectId::new();
        let mut nested = c.root.clone();
        nested.id = ObjectId::new();
        nested.name = "recursive".into();
        c.definitions[0].instances.push(nested);
        let error = c.validate_structure().unwrap_err();
        assert!(error.contains("Cyclic") && error.contains(&format!("{:?}", c.definitions[0].id)));
    }
}
