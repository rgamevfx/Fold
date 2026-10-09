use crate::{editing, patch};
use fold_platform::proposals::Failure;
use fold_platform::proposals::FailureKind;
use serde_json::{Value, json};
impl From<editing::Error> for Failure {
    fn from(error: editing::Error) -> Self {
        match error {
            editing::Error::Locked(value) => Self {
                kind: FailureKind::ProtectedEdit,
                message: value.reason,
                details: json!({"native_id":value.id}),
            },
            editing::Error::RebaseConflict(conflicts) => {
                Self::conflict(json!({"conflicts":conflicts}))
            }
            editing::Error::Patch(patch::Error::Conflict {
                index,
                path,
                expected,
                actual,
            }) => Self::conflict(
                json!({"index":index,"path":path,"expected":expected,"actual":actual}),
            ),
            editing::Error::MissingDocument => {
                Self::new(FailureKind::MissingResource, "Missing document")
            }
            editing::Error::Conflict | editing::Error::OutsideScope => {
                Self::new(FailureKind::Conflict, format!("{error:?}"))
            }
            editing::Error::Validation(diagnostic)
            | editing::Error::Patch(patch::Error::Validation(diagnostic)) => diagnostic.into(),
            editing::Error::Invalid(message) => Self::invalid(message),
            _ => Self::new(FailureKind::InvalidRequest, format!("{error:?}")),
        }
    }
}

impl From<crate::evaluation::Failure> for Failure {
    fn from(error: crate::evaluation::Failure) -> Self {
        use crate::evaluation::FailureKind as EvaluationFailure;
        Self {
            kind: match error.kind {
                EvaluationFailure::Invalid => FailureKind::InvalidRequest,
                EvaluationFailure::Budget => FailureKind::Oversized,
                EvaluationFailure::Cancelled => FailureKind::Cancelled,
            },
            message: error.message,
            details: error.context.map_or(Value::Null, |context| {
                json!({
                    "native_ids": [context.operation], "definition": context.definition,
                    "instance_path": context.instance_path, "asset_time": context.time,
                })
            }),
        }
    }
}
