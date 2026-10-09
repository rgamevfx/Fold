//! Machine scheduling categories; presentation text never determines policy.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Cancelled,
    ResourcePressure,
    RequestTooLarge,
    InvalidInput,
    MissingResource,
    UnsupportedCapability,
    Execution,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
    pub context: Vec<String>,
}
impl Failure {
    pub fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
            context: vec![],
        }
    }
    pub fn context(mut self, id: impl ToString) -> Self {
        self.context.push(id.to_string());
        self
    }
    pub fn retryable(&self) -> bool {
        self.kind == FailureKind::ResourcePressure
    }
}
impl std::fmt::Display for Failure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Existing presentation surfaces receive explanatory text only. Context
        // remains structured internal data, not additional artist-facing feedback.
        f.write_str(&self.message)
    }
}
impl std::error::Error for Failure {}
// Untyped external libraries report execution failures, never retryable pressure.
impl From<String> for Failure {
    fn from(message: String) -> Self {
        Self::new(FailureKind::Execution, message)
    }
}
impl From<&str> for Failure {
    fn from(message: &str) -> Self {
        Self::from(message.to_owned())
    }
}
// Terminal presentation and non-scheduling contracts keep explanatory text.
impl From<Failure> for String {
    fn from(error: Failure) -> Self {
        error.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn only_category_controls_retry_and_context_is_retained() {
        let pressure = Failure::new(FailureKind::ResourcePressure, "busy").context("document:42");
        assert!(pressure.retryable());
        assert_eq!(pressure.context, ["document:42"]);
        assert_eq!(pressure.to_string(), "busy");
        for kind in [
            FailureKind::Cancelled,
            FailureKind::RequestTooLarge,
            FailureKind::InvalidInput,
            FailureKind::MissingResource,
            FailureKind::UnsupportedCapability,
            FailureKind::Execution,
        ] {
            assert!(
                !Failure::new(kind, "GPU working budget exhausted; capacity exhausted").retryable()
            );
        }
        assert!(!Failure::from("GPU working budget exhausted".to_owned()).retryable());
    }
}
