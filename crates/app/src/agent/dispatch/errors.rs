use crate::agent::retry;
use fold_motion::{editing, patch};
use fold_project::proposals as candidates;
use serde_json::{Value, json};
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub code: &'static str,
    pub message: String,
    pub details: Value,
}
impl Failure {
    pub fn new(code: &'static str, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Value::Null,
        }
    }
    pub fn conflict(details: Value) -> Self {
        Self {
            code: "conflict",
            message: "Expected state differs".into(),
            details,
        }
    }
    pub fn invalid(message: String) -> Self {
        Self::new("type", message)
    }
}
impl From<retry::Error> for Failure {
    fn from(error: retry::Error) -> Self {
        let code = match error {
            retry::Error::Epoch => "conflict",
            retry::Error::Budget => "budget",
            retry::Error::Capacity => "capacity",
            retry::Error::IdempotencyConflict => "idempotency_conflict",
            retry::Error::Expired => "expired",
            retry::Error::InvalidTicket => "conflict",
        };
        Self::new(code, format!("{error:?}"))
    }
}
impl<E: std::fmt::Debug> From<fold_project::proposals::Error<E>> for Failure
where
    Failure: From<E>,
{
    fn from(error: fold_project::proposals::Error<E>) -> Self {
        let code = match error {
            candidates::Error::Edit(error) => return error.into(),
            candidates::Error::Capacity => "capacity",
            candidates::Error::Expired => "expired",
            candidates::Error::Cancelled => "cancelled",
            _ => "conflict",
        };
        Self::new(code, format!("{error:?}"))
    }
}
impl From<editing::Error> for Failure {
    fn from(error: editing::Error) -> Self {
        match error {
            editing::Error::Locked(value) => Self {
                code: "locked",
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
            editing::Error::MissingDocument => Self::new("missing_resource", "Missing document"),
            editing::Error::Conflict | editing::Error::OutsideScope => {
                Self::new("conflict", format!("{error:?}"))
            }
            editing::Error::Validation(diagnostic)
            | editing::Error::Patch(patch::Error::Validation(diagnostic)) => diagnostic.into(),
            editing::Error::Invalid(message) => Self::invalid(message),
            _ => Self::new("type", format!("{error:?}")),
        }
    }
}

impl From<fold_motion::evaluation::Failure> for Failure {
    fn from(error: fold_motion::evaluation::Failure) -> Self {
        use fold_motion::evaluation::FailureKind;
        Self {
            code: match error.kind {
                FailureKind::Invalid => "type",
                FailureKind::Budget => "budget",
                FailureKind::Cancelled => "cancelled",
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

impl From<fold_motion::validation::Diagnostic> for Failure {
    fn from(diagnostic: fold_motion::validation::Diagnostic) -> Self {
        use fold_motion::validation::Kind;
        Self {
            code: match diagnostic.kind {
                Kind::Type => "type",
                Kind::Cycle => "cycle",
                Kind::MissingReference => "missing_resource",
                Kind::Unsupported => "unsupported",
                Kind::Budget => "budget",
            },
            message: diagnostic.message,
            details: json!({"native_ids": diagnostic.native_ids}),
        }
    }
}

impl From<crate::agent::jobs::Error> for Failure {
    fn from(error: crate::agent::jobs::Error) -> Self {
        use crate::agent::jobs::Error;
        let code = match error {
            Error::Epoch | Error::InvalidTicket => "conflict",
            Error::Expired => "expired",
            Error::Capacity => "capacity",
            Error::Budget => "budget",
        };
        Self::new(code, format!("{error:?}"))
    }
}
