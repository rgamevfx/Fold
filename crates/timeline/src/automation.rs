use crate::{Sequence, SequenceEdit};
use fold_platform::authoring::{decode, encode};
use fold_project::Document;
use serde_json::{Value, json};
pub fn catalog() -> Value {
    json!({
        "operation_schemas": [schemars::schema_for!(SequenceEdit)],
        "notes": "Uses the same SequenceEdit as the UI, including linked edits and locked-track rules. Identities are literal UUIDs; time is rational seconds."
    })
}
pub fn inspect(d: &Document) -> Result<Value, String> {
    encode(Sequence::from_document(d)?)
}
pub fn author(d: &Document, edits: &[Value]) -> Result<Document, String> {
    let mut s = Sequence::from_document(d)?;
    for v in edits {
        s = s.edited(&decode::<SequenceEdit>(v)?)?;
    }
    let mut result = s.document(d.id)?;
    result.extensions = d.extensions.clone();
    Ok(result)
}
