//! Provider-owned native edits, committed through the feature-neutral coordinator.
use crate::{
    expression_source::Diagnostic,
    model::{Construction, Definition},
    protection, reconcile, source,
};
use fold_foundation::{DocumentId, ObjectId};
use fold_project::{CommittedSnapshot, Document, EditBatch, Mutation, Project, Revision};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Document,
    Definition(ObjectId),
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    MissingDocument,
    Invalid(String),
    Validation(crate::validation::Diagnostic),
    Locked(protection::Violation),
    OutsideScope,
    Conflict,
    RebaseConflict(Vec<crate::merge::Conflict>),
    NoChange,
    Patch(crate::patch::Error),
}
/// Fields stay private: a validated proposal cannot be mutated before applying it.
#[derive(Clone)]
pub struct Candidate {
    base: Revision,
    snapshot: fold_project::Snapshot,
    scope: Scope,
    before: Document,
    after: Document,
    source_spans: std::collections::BTreeMap<ObjectId, crate::expression_source::Span>,
}
impl Candidate {
    pub fn scope(&self) -> Scope {
        self.scope
    }
    pub fn base_revision(&self) -> Revision {
        self.base
    }

    pub fn document(&self) -> &Document {
        &self.after
    }
    /// Evaluation sees this proposal and its retained project settings/resources.
    /// The transient snapshot grants no save or Apply authority. Its provisional
    /// revision is not published; observers identify base_revision and generation.
    pub fn snapshot(&self) -> &fold_project::Snapshot {
        &self.snapshot
    }
    /// Diff is always computed against this immutable candidate's own base.
    pub fn diff(&self) -> Result<Vec<crate::diff::Change>, Error> {
        let before = Construction::from_document(&self.before).map_err(Error::Invalid)?;
        let after = Construction::from_document(&self.after).map_err(Error::Invalid)?;
        crate::diff::construction(&before, &after).map_err(Error::Invalid)
    }
    pub fn source_spans(
        &self,
    ) -> &std::collections::BTreeMap<ObjectId, crate::expression_source::Span> {
        &self.source_spans
    }
    /// Rebase into a new candidate. The original generation is never amended or
    /// applied implicitly; transport code must expose and inspect the new generation.
    pub fn rebase(&self, snapshot: &CommittedSnapshot) -> Result<Candidate, Error> {
        let current = snapshot
            .state()
            .documents
            .get(&self.before.id)
            .ok_or(Error::Conflict)?;
        let base = Construction::from_document(&self.before).map_err(Error::Invalid)?;
        let proposed = Construction::from_document(&self.after).map_err(Error::Invalid)?;
        let current = Construction::from_document(current).map_err(Error::Invalid)?;
        let merged = crate::merge::construction(&base, &proposed, &current)
            .map_err(Error::RebaseConflict)?;
        prepare(snapshot, self.before.id, self.scope, merged).map_err(|error| match error {
            Error::Invalid(reason)
            | Error::Validation(crate::validation::Diagnostic {
                message: reason, ..
            }) => Error::RebaseConflict(vec![crate::merge::Conflict {
                path: vec![],
                native_id: None,
                expected: None,
                actual: None,
                proposed: None,
                reason: format!("Merged dependency validation failed: {reason}"),
            }]),
            error => error,
        })
    }
    pub fn apply(self, project: &mut Project) -> Result<CommittedSnapshot, Error> {
        let current = project.snapshot();
        if current.revision() != self.base
            || current
                .state()
                .documents
                .get(&self.before.id)
                .map(|d| d.as_ref())
                != Some(&self.before)
        {
            return Err(Error::Conflict);
        }
        project
            .commit_named(
                EditBatch {
                    base: self.base,
                    mutations: vec![Mutation::PutDocument(self.after)],
                },
                "Edit Motion construction".into(),
            )
            .map_err(|error| Error::Invalid(error.to_string()))
    }
}
/// Structured proposals share the source candidate gate and its single undo group.
/// The supplied snapshot is the base for every expected-value precondition.
pub fn prepare_patches(
    snapshot: &CommittedSnapshot,
    document: DocumentId,
    scope: Scope,
    patches: &[crate::patch::Patch],
) -> Result<Candidate, Error> {
    let original = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or(Error::MissingDocument)?;
    let before = Construction::from_document(original).map_err(Error::Invalid)?;
    let after = crate::patch::construction(&before, patches).map_err(Error::Patch)?;
    prepare(snapshot, document, scope, after)
}

/// Trusted local artist action. Do not register this as an agent capability.
/// Only clears selected lock flags: no replacement content or bypass option.
pub fn unlock_for_artist(
    project: &mut Project,
    expected_revision: Revision,
    document: DocumentId,
    targets: &[ObjectId],
) -> Result<CommittedSnapshot, Error> {
    let snapshot = project.snapshot();
    if snapshot.revision() != expected_revision {
        return Err(Error::Conflict);
    }
    let original = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or(Error::MissingDocument)?;
    let after = unlock_document_for_artist(original, targets)?;
    project
        .commit_named(
            EditBatch {
                base: snapshot.revision(),
                mutations: vec![Mutation::PutDocument(after)],
            },
            "Unlock Motion controls".into(),
        )
        .map_err(|error| Error::Invalid(error.to_string()))
}

/// Trusted desktop provider route; never expose through agent authoring.
pub(crate) fn unlock_document_for_artist(
    original: &Document,
    targets: &[ObjectId],
) -> Result<Document, Error> {
    unlock_construction_for_artist(
        &Construction::from_document(original).map_err(Error::Invalid)?,
        targets,
    )?
    .replace_document(original)
    .map_err(Error::Invalid)
}

pub(crate) fn unlock_construction_for_artist(
    original: &Construction,
    targets: &[ObjectId],
) -> Result<Construction, Error> {
    if targets.is_empty() || targets.len() > crate::patch::MAX_PATCHES {
        return Err(Error::Invalid(
            "Select 1..128 controls or groups to unlock".into(),
        ));
    }
    let mut remaining: std::collections::BTreeSet<_> = targets.iter().copied().collect();
    if remaining.len() != targets.len() {
        return Err(Error::Invalid("Duplicate unlock target".into()));
    }
    let mut after = original.clone();
    let mut changed = false;
    let mut clear = |id: ObjectId, flag: &mut bool| {
        if remaining.remove(&id) {
            changed |= *flag;
            *flag = false;
        }
    };
    for definition in &mut after.definitions {
        for input in &mut definition.inputs {
            clear(input.id, &mut input.protected);
        }
        for node in &mut definition.nodes {
            for parameter in node.parameters.values_mut() {
                clear(parameter.id, &mut parameter.protected);
            }
        }
        for instance in &mut definition.instances {
            for parameter in instance.overrides.values_mut() {
                clear(parameter.id, &mut parameter.protected);
            }
        }
    }
    for parameter in after.root.overrides.values_mut() {
        clear(parameter.id, &mut parameter.protected);
    }
    for group in &mut after.presentation.groups {
        clear(group.id, &mut group.locked);
    }
    if !remaining.is_empty() {
        return Err(Error::Invalid(format!(
            "Unknown or non-lockable unlock targets: {remaining:?}"
        )));
    }
    if !changed {
        return Err(Error::NoChange);
    }
    // Flags do not change execution semantics. Preserve unresolved payloads too.
    Ok(after)
}

pub fn prepare(
    snapshot: &CommittedSnapshot,
    document: DocumentId,
    scope: Scope,
    after: Construction,
) -> Result<Candidate, Error> {
    let original = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or(Error::MissingDocument)?;
    let after = validate_document(original, scope, after)?;
    let workspace = Project::fork(snapshot);
    let mut overlay = workspace.begin_edit();
    workspace
        .update_edit(&mut overlay, vec![Mutation::PutDocument(after.clone())])
        .map_err(|error| Error::Invalid(error.to_string()))?;
    Ok(Candidate {
        base: snapshot.revision(),
        snapshot: overlay.snapshot(),
        scope,
        before: original.as_ref().clone(),
        after,
        source_spans: std::collections::BTreeMap::new(),
    })
}

/// The common gate for committed candidates and transient desktop commands.
pub(crate) fn validate_document(
    original: &Document,
    scope: Scope,
    after: Construction,
) -> Result<Document, Error> {
    let before = Construction::from_document(original).map_err(Error::Invalid)?;
    if let Scope::Definition(id) = scope {
        let replacement = after
            .definition(id)
            .map_err(|_| Error::OutsideScope)?
            .clone();
        if replace_definition(&before, replacement)? != after {
            return Err(Error::OutsideScope);
        }
    }
    after
        .validate_structure_detailed()
        .map_err(Error::Validation)?;
    protection::validate(&before, &after).map_err(Error::Locked)?;
    after
        .validate_semantics_detailed()
        .map_err(Error::Validation)?;
    if before == after {
        return Err(Error::NoChange);
    }
    after.replace_document(original).map_err(Error::Invalid)
}

fn replace_definition(
    before: &Construction,
    definition: Definition,
) -> Result<Construction, Error> {
    let old = before
        .definition(definition.id)
        .map_err(|_| Error::OutsideScope)?;
    let old_ids = reconcile::identities(old).map_err(Error::Invalid)?;
    let new_ids = reconcile::identities(&definition).map_err(Error::Invalid)?;
    let removed: std::collections::BTreeSet<_> = old_ids
        .keys()
        .filter(|id| !new_ids.contains_key(id))
        .copied()
        .collect();
    let mut after = before.clone();
    let target = after
        .definitions
        .iter_mut()
        .find(|d| d.id == definition.id)
        .ok_or(Error::OutsideScope)?;
    *target = definition;
    after
        .presentation
        .positions
        .retain(|id, _| !removed.contains(id));
    after
        .presentation
        .annotations
        .retain(|id, _| !removed.contains(id));
    for group in &mut after.presentation.groups {
        group.members.retain(|id| !removed.contains(id));
    }
    Ok(after)
}
/// Reconcile a previously parsed asset fragment using explicit UUID mappings.
/// Unrelated assets, project data and surviving presentation records are retained.
pub fn prepare_fragment(
    snapshot: &CommittedSnapshot,
    document: DocumentId,
    target: ObjectId,
    fragment: &source::Fragment,
    mapping: &reconcile::IdentityMap,
) -> Result<Candidate, Error> {
    let original = snapshot
        .state()
        .documents
        .get(&document)
        .ok_or(Error::MissingDocument)?;
    let before = Construction::from_document(original).map_err(Error::Invalid)?;
    let reconciled = reconcile::fragment(
        before.definition(target).map_err(|_| Error::OutsideScope)?,
        fragment,
        mapping,
    )
    .map_err(Error::Invalid)?;
    let after = replace_definition(&before, reconciled.definition)?;
    let mut candidate = prepare(snapshot, document, Scope::Definition(target), after)?;
    candidate.source_spans = reconciled.spans;
    Ok(candidate)
}
/// Transient editor state; invalid text never replaces the committed native document.
#[derive(Default)]
pub struct SourceBuffer {
    text: String,
    diagnostics: Vec<Diagnostic>,
    edit_error: Option<Error>,
}
impl SourceBuffer {
    pub fn text(&self) -> &str {
        &self.text
    }
    pub fn diagnostics(&self) -> &[Diagnostic] {
        &self.diagnostics
    }
    pub fn edit_error(&self) -> Option<&Error> {
        self.edit_error.as_ref()
    }
    pub fn propose(
        &mut self,
        text: String,
        snapshot: &CommittedSnapshot,
        document: DocumentId,
        scope: Scope,
    ) -> Result<Candidate, Error> {
        self.text = text;
        self.diagnostics.clear();
        self.edit_error = None;
        let parsed = match source::parse(&self.text) {
            Ok(parsed) => parsed,
            Err(diagnostic) => {
                let error = diagnostic
                    .validation
                    .clone()
                    .map(Error::Validation)
                    .unwrap_or_else(|| Error::Invalid(diagnostic.message.clone()));
                self.diagnostics.push(diagnostic);
                self.edit_error = Some(error.clone());
                return Err(error);
            }
        };
        prepare(snapshot, document, scope, parsed.construction)
            .map(|mut candidate| {
                candidate.source_spans = parsed.spans;
                candidate
            })
            .inspect_err(|error| self.edit_error = Some(error.clone()))
    }
    /// Mappings refer to UUIDs in this source. For an ID-free fragment, parse once
    /// and pass that retained Fragment to prepare_fragment after selecting mappings.
    pub fn propose_fragment(
        &mut self,
        text: String,
        snapshot: &CommittedSnapshot,
        document: DocumentId,
        target: ObjectId,
        mapping: &reconcile::IdentityMap,
    ) -> Result<Candidate, Error> {
        self.text = text;
        self.diagnostics.clear();
        self.edit_error = None;
        let result = (|| {
            let original = snapshot
                .state()
                .documents
                .get(&document)
                .ok_or(Error::MissingDocument)?;
            let before = Construction::from_document(original).map_err(Error::Invalid)?;
            let fragment = match source::parse_fragment(&self.text, &before, target) {
                Ok(fragment) => fragment,
                Err(diagnostic) => {
                    let error = diagnostic
                        .validation
                        .clone()
                        .map(Error::Validation)
                        .unwrap_or_else(|| Error::Invalid(diagnostic.message.clone()));
                    self.diagnostics.push(diagnostic);
                    return Err(error);
                }
            };
            prepare_fragment(snapshot, document, target, &fragment, mapping)
        })();
        result.inspect_err(|error| self.edit_error = Some(error.clone()))
    }
}
