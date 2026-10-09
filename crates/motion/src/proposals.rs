//! Motion owns source parsing, scope, correspondence, diagnostics and protection.
use crate::{editing, model::Construction, scene::Scene, source};
use fold_foundation::{DocumentId, ObjectId};
use fold_media::Cancel;
use fold_platform::proposals::*;
use fold_project::{Document, Snapshot};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::BTreeMap;

pub struct Provider {
    scene: bool,
}
impl Provider {
    pub const CONSTRUCTION: Self = Self { scene: false };
    pub const SCENE: Self = Self { scene: true };
}
#[derive(Serialize, Deserialize, schemars::JsonSchema)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum GraphEdit {
    Source {
        expected: String,
        text: String,
        mapping: BTreeMap<ObjectId, ObjectId>,
    },
    Patches {
        patches: Vec<crate::patch::Patch>,
    },
}
#[derive(Deserialize, schemars::JsonSchema)]
#[serde(tag = "op", rename_all = "snake_case", deny_unknown_fields)]
pub enum SceneEdit {
    Author {
        document: DocumentId,
        expected: Value,
        edits: Vec<Value>,
    },
    Hierarchy {
        document: DocumentId,
        expected: Value,
        edits: Vec<fold_platform::hierarchy::Edit>,
    },
    ObjectSource {
        document: DocumentId,
        object: ObjectId,
        expected: String,
        source: String,
    },
}
pub fn projection(value: &Construction, scope: Scope) -> Result<String, Failure> {
    scope
        .definition
        .map_or_else(
            || source::canonical(value),
            |id| source::canonical_fragment(value, id),
        )
        .map_err(Failure::invalid)
}

fn graph_edit(before: &Construction, scope: Scope, body: Value) -> Result<Construction, Failure> {
    let edit: GraphEdit =
        serde_json::from_value(body).map_err(|e| Failure::invalid(e.to_string()))?;
    let after = match edit {
        GraphEdit::Patches { patches } => {
            crate::patch::construction(before, &patches).map_err(editing::Error::Patch)?
        }
        GraphEdit::Source {
            expected,
            text,
            mapping,
        } => {
            if expected.len() > 65536 || text.len() > 65536 {
                return Err(Failure::new(
                    FailureKind::Oversized,
                    "Source exceeds 64 KiB",
                ));
            }
            let actual = projection(before, scope)?;
            if expected != actual {
                return Err(Failure::conflict(
                    json!({"expected":expected,"actual":actual}),
                ));
            }
            if let Some(id) = scope.definition {
                let fragment = source::parse_fragment(&text, before, id).map_err(diagnostic)?;
                let mapped = crate::reconcile::fragment(
                    before.definition(id).map_err(Failure::invalid)?,
                    &fragment,
                    &mapping,
                )
                .map_err(Failure::invalid)?;
                editing::replace_definition(before, mapped.definition)?
            } else {
                if !mapping.is_empty() {
                    return Err(Failure::new(
                        FailureKind::Unsupported,
                        "Identity mapping requires an asset scope",
                    ));
                }
                source::parse(&text).map_err(diagnostic)?.construction
            }
        }
    };
    validate_graph(before, &after, scope)?;
    if before == &after {
        return Err(Failure::invalid("NoChange".into()));
    }
    Ok(after)
}
fn diagnostic(d: crate::expression_source::Diagnostic) -> Failure {
    let mut failure = d
        .validation
        .map(Failure::from)
        .unwrap_or_else(|| Failure::new(FailureKind::InvalidSource, d.message.clone()));
    failure.details["diagnostics"] = json!([{"message":d.message,"start":d.span.start,"end":d.span.end,"native_id":d.native_id}]);
    failure
}
fn validate_graph(
    before: &Construction,
    after: &Construction,
    scope: Scope,
) -> Result<(), Failure> {
    crate::proposal_budget::construction(after)?;
    after.validate_structure_detailed()?;
    crate::protection::validate(before, after).map_err(editing::Error::Locked)?;
    if let Some(id) = scope.definition
        && editing::replace_definition(
            before,
            after.definition(id).map_err(Failure::invalid)?.clone(),
        )? != *after
    {
        return Err(Failure::new(
            FailureKind::Conflict,
            "Edit exceeds definition scope",
        ));
    }
    after.validate_semantics_detailed()?;
    Ok(())
}
impl ScopedProposals for Provider {
    fn schema(&self) -> Value {
        let mut schema = json!({"version":1,"graph":schemars::schema_for!(GraphEdit),"catalog":crate::registry::catalog()});
        if self.scene {
            schema["scene"] = json!(schemars::schema_for!(SceneEdit));
        }
        schema
    }
    fn inspect(
        &self,
        document: &Document,
        scope: Scope,
        cancel: &Cancel,
    ) -> Result<DocumentInspection, Failure> {
        check(cancel)?;
        Motion::decode(document)?.source(scope)
    }
    fn prepare(
        &self,
        base: &Snapshot,
        request: &ScopedRequest,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure> {
        (|| -> Result<Box<dyn PreparedProposal>, Failure> {
        fold_platform::proposals::request(request, cancel)?;
        let scope = request.scope;
        let original =
            base.state().documents.get(&scope.document).ok_or_else(|| {
                Failure::new(FailureKind::MissingResource, "Missing Motion document")
            })?;
        let mut records = Motion::decode(original)?;
        match &mut records {
            Motion::Construction(graph) => {
                if scope.object.is_some() {
                    return Err(Failure::new(
                        FailureKind::InvalidRequest,
                        "Construction has no scene objects",
                    ));
                }
                **graph = graph_edit(graph, scope, request.body.clone())?;
            }
            Motion::Scene(scene) => {
                if let Some(id) = scope.object {
                    let object =
                        scene
                            .objects
                            .iter_mut()
                            .find(|o| o.id == id)
                            .ok_or_else(|| {
                                Failure::new(FailureKind::MissingResource, "Missing scene object")
                            })?;
                    object.graph = graph_edit(&object.graph, scope, request.body.clone())?;
                } else {
                    document_scope(scope)?;
                    let edit: SceneEdit = serde_json::from_value(request.body.clone())
                        .map_err(|e| Failure::invalid(e.to_string()))?;
                    use fold_platform::packages::{Authoring, Hierarchy};
                    let provider = crate::scene::package::Documents;
                    let document = match edit {
                        SceneEdit::Author {
                            document,
                            expected,
                            edits,
                        } => {
                            expected_scene(original, scope, document, &expected)?;
                            provider
                                .author_cancellable(original, &edits, cancel)
                                .map_err(Failure::invalid)?
                        }
                        SceneEdit::Hierarchy {
                            document,
                            expected,
                            edits,
                        } => {
                            expected_scene(original, scope, document, &expected)?;
                            if edits.iter().any(|e| {
                                matches!(
                                    e,
                                    fold_platform::hierarchy::Edit::Lock { value: false, .. }
                                )
                            }) {
                                return Err(Failure::new(
                                    FailureKind::ProtectedEdit,
                                    "Agent edits cannot unlock objects",
                                ));
                            }
                            provider
                                .edit_hierarchy(original, &edits)
                                .map_err(Failure::invalid)?
                        }
                        SceneEdit::ObjectSource {
                            document,
                            object,
                            expected,
                            source,
                        } => {
                            if document != scope.document {
                                return Err(Failure::new(
                                    FailureKind::Conflict,
                                    "Document scope differs",
                                ));
                            }
                            if expected.len() > 65536 || source.len() > 65536 {
                                return Err(Failure::new(
                                    FailureKind::Oversized,
                                    "Object source exceeds 64 KiB",
                                ));
                            }
                            let target = scene
                                .objects
                                .iter_mut()
                                .find(|o| o.id == object)
                                .ok_or_else(|| {
                                    Failure::new(
                                        FailureKind::MissingResource,
                                        "Missing scene object",
                                    )
                                })?;
                            target.graph = graph_edit(
                                &target.graph,
                                Scope {
                                    object: Some(object),
                                    ..scope
                                },
                                json!({"kind":"source","expected":expected,"text":source,"mapping":{}}),
                            )?;
                            scene
                                .replace(original)
                                .map_err(|e| Failure::new(FailureKind::ProtectedEdit, e))?
                        }
                    };
                    records = Motion::decode(&document)?;
                }
            }
        }
        let after = records.document(original)?;
        self.prepare_change(base, scope, &after, cancel)
        })().map_err(|error:Failure|error.context(crate::PACKAGE,request.scope))
    }
    fn prepare_change(
        &self,
        base: &Snapshot,
        scope: Scope,
        after: &Document,
        cancel: &Cancel,
    ) -> Result<Box<dyn PreparedProposal>, Failure> {
        (|| -> Result<Box<dyn PreparedProposal>, Failure> {
            Ok(Box::new(Prepared::<Motion>::new(
                base, scope, after, cancel,
            )?))
        })()
        .map_err(|error: Failure| error.context(crate::PACKAGE, scope))
    }
}
fn expected_scene(
    original: &Document,
    scope: Scope,
    id: DocumentId,
    expected: &Value,
) -> Result<(), Failure> {
    let native = serde_json::to_value(Scene::from_document(original).map_err(Failure::invalid)?)
        .map_err(|e| Failure::invalid(e.to_string()))?;
    if id != scope.document || expected["native"] != native {
        return Err(Failure::new(
            FailureKind::Conflict,
            "Expected scene differs",
        ));
    }
    Ok(())
}
enum Motion {
    Construction(Box<Construction>),
    Scene(Scene),
}
impl Records for Motion {
    fn undo_label(&self) -> &'static str {
        match self {
            Self::Construction(_) => "Edit Motion construction",
            Self::Scene(_) => "Edit Composition and scene graphs",
        }
    }
    fn decode(document: &Document) -> Result<Self, Failure> {
        match document.type_id.as_str() {
            crate::CONSTRUCTION => {
                let value = Construction::from_document(document).map_err(Failure::invalid)?;
                crate::proposal_budget::construction(&value)?;
                Ok(Self::Construction(Box::new(value)))
            }
            crate::scene::TYPE => {
                let value = Scene::from_document(document).map_err(Failure::invalid)?;
                for o in &value.objects {
                    crate::proposal_budget::construction(&o.graph)?;
                }
                Ok(Self::Scene(value))
            }
            _ => Err(Failure::new(
                FailureKind::Unsupported,
                "Unsupported Motion document",
            )),
        }
    }
    fn validate_change(before: &Self, after: &Self, scope: Scope) -> Result<(), Failure> {
        match (before, after) {
            (Self::Construction(a), Self::Construction(b)) => {
                if scope.object.is_some() {
                    return Err(Failure::new(
                        FailureKind::InvalidRequest,
                        "Construction has no objects",
                    ));
                }
                validate_graph(a, b, scope)
            }
            (Self::Scene(a), Self::Scene(b)) => {
                if let Some(id) = scope.object {
                    let mut allowed = a.clone();
                    let target = allowed
                        .objects
                        .iter_mut()
                        .find(|o| o.id == id)
                        .ok_or_else(|| Failure::new(FailureKind::Conflict, "Object removed"))?;
                    let next = b.object(id).map_err(Failure::invalid)?;
                    validate_graph(&target.graph, &next.graph, scope)?;
                    target.graph = next.graph.clone();
                    if allowed != *b {
                        return Err(Failure::new(
                            FailureKind::Conflict,
                            "Edit exceeds object scope",
                        ));
                    }
                } else {
                    document_scope(scope)?;
                }
                b.replace(&a.document(scope.document).map_err(Failure::invalid)?)
                    .map_err(|e| Failure::new(FailureKind::ProtectedEdit, e))?;
                Ok(())
            }
            _ => Err(Failure::new(
                FailureKind::Conflict,
                "Motion document kind changed",
            )),
        }
    }
    fn reconcile(
        before: &Self,
        after: &Self,
        current: &Self,
        scope: Scope,
    ) -> Result<Self, Failure> {
        match (before, after, current) {
            (Self::Construction(a), Self::Construction(b), Self::Construction(c)) => {
                Ok(Self::Construction(Box::new(
                    crate::merge::construction(a, b, c).map_err(editing::Error::RebaseConflict)?,
                )))
            }
            (Self::Scene(a), Self::Scene(b), Self::Scene(c)) => Ok(Self::Scene(
                crate::scene::reconcile::merge(a, b, c)
                    .map_err(|e| Failure::new(FailureKind::Conflict, e))?,
            )),
            _ => Err(Failure::conflict(json!({"scope":scope}))),
        }
    }
    fn document(&self, original: &Document) -> Result<Document, Failure> {
        match self {
            Self::Construction(v) => v.replace_document(original).map_err(Failure::invalid),
            Self::Scene(v) => v.replace(original).map_err(Failure::invalid),
        }
    }
    fn source(&self, scope: Scope) -> Result<DocumentInspection, Failure> {
        match self {
            Self::Construction(v) => projection(v, scope).map(DocumentInspection::Source),
            Self::Scene(v) => {
                if let Some(id) = scope.object {
                    return projection(&v.object(id).map_err(Failure::invalid)?.graph, scope)
                        .map(DocumentInspection::Source);
                }
                document_scope(scope)?;
                let sources = v
                    .objects
                    .iter()
                    .map(|o| {
                        Ok((
                            o.id.to_string(),
                            json!(source::canonical(&o.graph).map_err(Failure::invalid)?),
                        ))
                    })
                    .collect::<Result<serde_json::Map<_, _>, Failure>>()?;
                Ok(DocumentInspection::Native(
                    json!({"native":v,"object_sources":sources}),
                ))
            }
        }
    }
    fn diff(before: Option<&Self>, after: &Self, scope: Scope) -> Result<Vec<Value>, Failure> {
        match (before, after) {
            (Some(Self::Construction(a)), Self::Construction(b)) => crate::diff::construction(a, b)
                .map_err(Failure::invalid)?
                .into_iter()
                .map(|v| serde_json::to_value(v).map_err(|e| Failure::invalid(e.to_string())))
                .collect(),
            (Some(Self::Scene(a)), Self::Scene(b)) if scope.object.is_some() => Ok(vec![
                json!({"document":scope.document,"object":scope.object,"changes":crate::diff::construction(&a.object(scope.object.unwrap()).map_err(Failure::invalid)?.graph,&b.object(scope.object.unwrap()).map_err(Failure::invalid)?.graph).map_err(Failure::invalid)?}),
            ]),
            (Some(Self::Scene(a)), Self::Scene(b)) => json_diff(Some(a), b, scope),
            (None, Self::Scene(b)) => json_diff(None, b, scope),
            (None, Self::Construction(b)) => json_diff(None, b, scope),
            _ => Err(Failure::new(
                FailureKind::Conflict,
                "Motion document kind changed",
            )),
        }
    }
    fn affected(
        before: Option<&Self>,
        after: &Self,
        scope: Scope,
    ) -> Result<Vec<ObjectId>, Failure> {
        let ids = match (before, after) {
            (Some(Self::Construction(a)), Self::Construction(b)) => crate::diff::construction(a, b)
                .map_err(Failure::invalid)?
                .into_iter()
                .filter_map(|c| c.native_id)
                .collect(),
            (Some(Self::Scene(a)), Self::Scene(b)) => a
                .objects
                .iter()
                .chain(&b.objects)
                .filter(|o| {
                    a.objects.iter().find(|v| v.id == o.id)
                        != b.objects.iter().find(|v| v.id == o.id)
                })
                .map(|o| o.id)
                .collect(),
            (_, Self::Construction(v)) => v.definitions.iter().map(|d| d.id).collect(),
            (_, Self::Scene(v)) => v.objects.iter().map(|o| o.id).collect(),
        };
        let mut ids: std::collections::BTreeSet<ObjectId> = ids;
        ids.extend(scope.object);
        Ok(ids.into_iter().collect())
    }
}
