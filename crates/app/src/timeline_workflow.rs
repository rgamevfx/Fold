//! Compatibility exports for application callers. Authoring belongs entirely
//! to the timeline package; new UI commands dispatch through its registration.
pub use fold_timeline::{active, has_sequence, import};
