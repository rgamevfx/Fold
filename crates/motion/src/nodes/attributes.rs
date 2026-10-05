//! Explicit typed attribute access, keyed by element identity rather than order.
use super::*;
use crate::{
    evaluation::{Value, field, output},
    fields::{Field, Sample},
    geometry::Content,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, sync::Arc};
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Settings {
    pub geometry_kind: Kind,
    pub value_type: Kind,
}
impl Default for Settings {
    fn default() -> Self {
        Self {
            geometry_kind: Kind::Content,
            value_type: Kind::Scalar,
        }
    }
}
fn signature(n: &Node) -> Result<(Vec<Socket>, Vec<(String, Kind)>), String> {
    let settings = n.settings::<Settings>()?;
    if !matches!(settings.geometry_kind, Kind::Content | Kind::Points)
        || !matches!(
            settings.value_type,
            Kind::Scalar | Kind::Vector | Kind::Color | Kind::Bool
        )
    {
        return Err(
            "Attribute requires geometry and a scalar, vector, color or boolean type".into(),
        );
    }
    let default = match settings.value_type {
        Kind::Vector => Datum::Vector([0.; 2]),
        Kind::Color => Datum::Color([0.; 4]),
        Kind::Bool => Datum::Bool(false),
        _ => Datum::Scalar(0.),
    };
    let store = n.kind == "fold.motion.store_attribute";
    Ok((
        vec![
            Socket::required("content", settings.geometry_kind),
            Socket::value("name", Datum::Text("value".into()), ""),
            Socket::value(if store { "value" } else { "default" }, default, ""),
        ],
        vec![(
            if store { "content" } else { "value" }.into(),
            if store {
                settings.geometry_kind
            } else {
                settings.value_type
            },
        )],
    ))
}
fn name(e: &mut Evaluator<'_>, n: &Node) -> Result<String, String> {
    let name = e.uniform(n, "name")?.text()?.to_owned();
    if name.is_empty() || name.len() > 256 {
        return Err("Attribute name must contain 1..256 bytes".into());
    }
    Ok(name)
}
pub fn definitions() -> Vec<Definition> {
    let mut result = vec![
        definition(
            "fold.motion.read_attribute",
            "Read Attribute",
            "Attributes",
            vec![],
            vec![],
            |e, n| {
                let content = e.content(n, "content")?;
                let name = name(e, n)?;
                let fallback = e.field(n, "default")?;
                let mut values = BTreeMap::new();
                for item in content.iter() {
                    e.budget.spend()?;
                    if values
                        .insert(item.id, item.attributes.get(&name).cloned())
                        .is_some()
                    {
                        return Err("Attribute source has duplicate element IDs".into());
                    }
                }
                let kind = fallback.kind;
                Ok(field(Field::new(kind, true, move |s, b| {
                    match values.get(&s.id).and_then(Option::as_ref) {
                        Some(value) if value.kind() == kind => Ok(value.clone()),
                        Some(_) => Err("Attribute type mismatch".into()),
                        None => fallback.sample(s, b),
                    }
                })))
            },
        ),
        definition(
            "fold.motion.store_attribute",
            "Store Attribute",
            "Attributes",
            vec![],
            vec![],
            |e, n| {
                let source = e.content(n, "content")?;
                let name = name(e, n)?;
                let field = e.field(n, "value")?;
                let mut result = Vec::with_capacity(source.len());
                for (index, item) in source.iter().enumerate() {
                    e.budget.spend()?;
                    if item.attributes.len() >= 64 && !item.attributes.contains_key(&name) {
                        return Err("Element attribute limit is 64".into());
                    }
                    let value = field.sample(&sample(item, index, source.len()), &mut e.budget)?;
                    value.validate()?;
                    let mut item = item.clone();
                    item.attributes.insert(name.clone(), value);
                    result.push(item);
                }
                let content: Content = Arc::new(result);
                Ok(output(
                    "content",
                    if n.settings::<Settings>()?.geometry_kind == Kind::Points {
                        Value::Points(content)
                    } else {
                        Value::Content(content)
                    },
                ))
            },
        ),
    ];
    for d in &mut result {
        d.defaults = || serde_json::to_value(Settings::default()).unwrap();
        d.signature = Some(signature);
        d.validate = |n| signature(n).map(|_| ());
    }
    result
}
pub(crate) fn sample(item: &crate::geometry::Element, index: usize, count: usize) -> Sample {
    Sample {
        position: [item.transform[4], item.transform[5]],
        index,
        id: item.id,
        count,
        ..Default::default()
    }
}
