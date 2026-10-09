use crate::agent::retry;
use fold_project::proposals as candidates;
use serde_json::Value;
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
impl From<fold_platform::proposals::Failure> for Failure {
    fn from(e: fold_platform::proposals::Failure) -> Self {
        Self {
            code: e.kind.code(),
            message: e.message,
            details: e.details,
        }
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
