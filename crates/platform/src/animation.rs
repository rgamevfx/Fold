//! Provider-owned keyed parameters exposed to Composition without graph models.
use fold_animation::Curve;
use fold_foundation::{ObjectId, Time};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Target {
    pub group: Option<ObjectId>,
    pub object: ObjectId,
    pub path: String,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Channel {
    pub target: Target,
    pub node_label: String,
    pub property: String,
    pub property_label: String,
    pub component: String,
    pub curve: Curve,
}
impl Channel {
    /// A translation only: slopes and normalized tangent times are unchanged.
    pub fn shift(&mut self, delta: Time) -> Result<(), String> {
        for key in &mut self.curve.keys {
            key.time = key.time.checked_add(delta).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}
