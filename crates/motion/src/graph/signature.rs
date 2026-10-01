use super::{
    Node,
    definition::{Definition, Socket},
};
use crate::fields::Kind;
impl Definition {
    pub fn signature(&self, node: &Node) -> Result<(Vec<Socket>, Vec<(String, Kind)>), String> {
        if let Some(signature) = self.signature {
            return signature(node);
        }
        let kind: Kind = if self.id == "fold.motion.keyframes" {
            node.settings::<crate::animation::Track>()?
                .keys
                .first()
                .ok_or("empty keyframe track")?
                .value
                .kind()
        } else {
            node.settings
                .get("value_type")
                .map(|v| serde_json::from_value(v.clone()))
                .transpose()
                .map_err(|e| e.to_string())?
                .unwrap_or(Kind::Scalar)
        };
        if kind == Kind::Generic {
            return Err("Generic is not a concrete value type".into());
        }
        let mut inputs = self.inputs.clone();
        for s in &mut inputs {
            if s.kind == Kind::Generic {
                s.kind = kind;
                if s.default.as_ref().is_some_and(|v| v.kind() != kind) {
                    let value = s
                        .default
                        .as_ref()
                        .and_then(|v| v.scalar().ok())
                        .unwrap_or(0.);
                    s.default = Some(match kind {
                        Kind::Vector => crate::fields::Datum::Vector([value; 2]),
                        Kind::Color => crate::fields::Datum::Color([value; 4]),
                        Kind::Bool => crate::fields::Datum::Bool(false),
                        Kind::Text => crate::fields::Datum::Text(String::new()),
                        _ => return Err("unsupported default value type".into()),
                    });
                }
            }
        }
        let outputs = self
            .outputs
            .iter()
            .map(|(name, k)| {
                (
                    name.to_string(),
                    if *k == Kind::Generic { kind } else { *k },
                )
            })
            .collect();
        Ok((inputs, outputs))
    }
}
