//! Versioned component identity embedded in independent document instances.
use fold_foundation::ObjectId;
use serde::{Deserialize, Serialize};
pub const PROVENANCE: &str = "fold.gallery.instance";
#[derive(
    Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Reference {
    pub id: ObjectId,
    pub version: u32,
}
#[derive(
    Clone,
    Copy,
    Debug,
    Default,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Serialize,
    Deserialize,
    schemars::JsonSchema,
)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    #[default]
    Graphic,
    Motion,
    Effect,
    Transition,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Origin {
    component: Reference,
    #[serde(default)]
    name: String,
    kind: Kind,
}
/// Preserve opaque extensions while allowing an explicit version update to a known origin.
pub fn preserves_extensions(before: &crate::Metadata, after: &crate::Metadata) -> bool {
    if before
        .iter()
        .filter(|(k, _)| k.as_str() != PROVENANCE)
        .ne(after.iter().filter(|(k, _)| k.as_str() != PROVENANCE))
    {
        return false;
    }
    let read = |m: &crate::Metadata| -> Result<Option<Origin>, ()> {
        m.get(PROVENANCE)
            .map(|v| serde_json::from_value::<Origin>(v.clone()).map_err(|_| ()))
            .transpose()
    };
    match (read(before), read(after)) {
        (Ok(None), Ok(None)) => true,
        (Ok(old), Ok(Some(new))) => {
            new.component.version > 0
                && new.name.len() <= 128
                && old
                    .is_none_or(|old| old.component.id == new.component.id && old.kind == new.kind)
        }
        _ => false,
    }
}
