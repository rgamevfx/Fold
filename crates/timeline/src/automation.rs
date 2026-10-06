use crate::{Sequence, SequenceEdit};
use fold_platform::authoring::{decode, encode};
use fold_project::Document;
use serde_json::{Value, json};
pub fn catalog() -> Value {
    fold_platform::authoring::examples(
        json!({"operation_schemas":[fold_platform::authoring::reference_schema(serde_json::to_value(schemars::schema_for!(SequenceEdit)).expect("operation schema"))],"operations":[
    {"AddTrack":{"kind":"Video"}},
    {"Track":{"id":"$track","enabled":true,"solo":false,"locked":false}},
    {"RemoveTrack":{"id":"$track"}},
    {"Clip":{"id":"$clip","linked":true,"edit":{"Move":{"start":{"numerator":1,"denominator":1}}}}},
    {"Clip":{"id":"$clip","linked":true,"edit":{"Trim":{"start":{"numerator":0,"denominator":1},"end":{"numerator":2,"denominator":1}}}}},
    {"Clip":{"id":"$clip","linked":true,"edit":{"Split":{"at":{"numerator":1,"denominator":1},"right_id":"$right"}}}},
    {"Clip":{"id":"$clip","linked":true,"edit":"Lift"}},
    {"Clip":{"id":"$clip","linked":false,"edit":"Unlink"}},
    {"Clip":{"id":"$clip","linked":false,"edit":{"Level":0.5}}}
],"notes":"Uses the same SequenceEdit as the UI, including linked edits and locked-track rules. Insert assets/documents through placement. Time is rational seconds."}),
    )
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
