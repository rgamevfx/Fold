//! Versioned native operation records. Kernel configurations are disposable projections;
//! authoring, persistence and protection operate on the identity-bearing record.
use crate::{Node, OPERATORS, OperationConfig};
use fold_foundation::ObjectId;
use fold_graph::model::{Node as Record, Parameter};
use fold_graph::{binding::Binding, types::Literal};
use fold_project::Metadata;
use serde_json::{Value, json};
use std::collections::BTreeMap;

impl std::ops::Deref for Node {
    type Target = Record;
    fn deref(&self) -> &Record {
        &self.operation
    }
}
impl std::ops::DerefMut for Node {
    fn deref_mut(&mut self) -> &mut Record {
        &mut self.operation
    }
}

fn fields(config: &OperationConfig) -> (String, BTreeMap<String, Value>) {
    match serde_json::to_value(config).expect("finite operation configuration") {
        Value::String(tag) => (tag, BTreeMap::new()),
        Value::Object(object) => {
            let (tag, value) = object.into_iter().next().expect("operation variant");
            (
                tag,
                serde_json::from_value(value).expect("configuration fields"),
            )
        }
        _ => unreachable!("operation configurations have named fields"),
    }
}
pub(crate) fn parameter_values(config: &OperationConfig) -> BTreeMap<String, Value> {
    fields(config)
        .1
        .into_iter()
        .map(|(name, value)| (name, encode_value(value)))
        .collect()
}
pub(crate) fn record(config: &OperationConfig) -> Record {
    let (_, mut fields) = fields(config);
    if let OperationConfig::Instance { instance, .. } = config {
        fields.insert(
            "instance".into(),
            serde_json::to_value(
                crate::instance::Instance::capture(instance).expect("valid effect invocation"),
            )
            .expect("instance record"),
        );
    }
    let op = config.operator();
    Record {
        id: ObjectId::new(),
        name: op.label().into(),
        operation: op.id.into(),
        version: op.version,
        mode: String::new(),
        parameters: fields
            .into_iter()
            .map(|(name, value)| {
                (
                    name,
                    Parameter {
                        id: ObjectId::new(),
                        value: encode_value(value),
                        protected: false,
                        extensions: Metadata::new(),
                    },
                )
            })
            .collect(),
        extensions: Metadata::new(),
    }
}
fn decode(record: &Record) -> Result<OperationConfig, String> {
    let index = OPERATORS
        .iter()
        .position(|op| op.id == record.operation && op.version == record.version)
        .ok_or_else(|| {
            format!(
                "Unresolved compositor operation {} version {}",
                record.operation, record.version
            )
        })?;
    if !record.mode.is_empty() {
        return Err("Unsupported compositor operation mode".into());
    }
    let mut fields: serde_json::Map<String, Value> = record
        .parameters
        .iter()
        .map(|(name, p)| Ok((name.clone(), decode_value(&p.value)?)))
        .collect::<Result<_, String>>()?;
    if OPERATORS[index].kernel == "Instance" {
        let instance: crate::instance::Instance =
            serde_json::from_value(fields.remove("instance").ok_or("Missing effect instance")?)
                .map_err(|e| e.to_string())?;
        fields.insert(
            "instance".into(),
            serde_json::to_value(instance.host()?).map_err(|e| e.to_string())?,
        );
    }
    let kernel = OPERATORS[index].kernel;
    let value = if matches!(kernel, "ApplyMask" | "Output") {
        if !fields.is_empty() {
            return Err("Operation has no parameters".into());
        }
        json!(kernel)
    } else {
        json!({kernel: fields})
    };
    let config: OperationConfig = serde_json::from_value(value).map_err(|e| e.to_string())?;
    if config.operator().id != record.operation || config.operator().version != record.version {
        return Err("Configuration disagrees with its versioned operation contract".into());
    }
    config.validate()?;
    Ok(config)
}
impl Node {
    pub(crate) fn set_native_instance(
        &mut self,
        instance: crate::instance::Instance,
    ) -> Result<(), String> {
        instance.host()?;
        let before = self.native_instance()?.ok_or("Not an effect instance")?;
        before.check_protection(Some(&instance))?;
        let value = encode_value(serde_json::to_value(instance).map_err(|e| e.to_string())?);
        let parameter = self
            .operation
            .parameters
            .get_mut("instance")
            .ok_or("Missing instance parameter")?;
        if parameter.protected && parameter.value != value {
            return Err("Instance is protected".into());
        }
        parameter.value = value;
        Ok(())
    }
    pub(crate) fn duplicate_instance(&mut self) -> Result<(), String> {
        if let Some(mut instance) = self.native_instance()? {
            instance.duplicate()?;
            self.operation.parameters.get_mut("instance").unwrap().value =
                encode_value(serde_json::to_value(instance).map_err(|e| e.to_string())?);
        }
        Ok(())
    }
    pub(crate) fn native_instance(&self) -> Result<Option<crate::instance::Instance>, String> {
        native_instance(&self.operation)
    }
    pub fn from_record(record: Record) -> Result<Self, String> {
        let config = decode(&record)?;
        let mut node = Self::disconnected(config);
        node.operation = record;
        Ok(node)
    }
    pub fn try_configuration(&self) -> Result<OperationConfig, String> {
        decode(&self.operation)
    }
    /// Internal consumers only receive graphs admitted by Composite::validate.
    pub fn configuration(&self) -> OperationConfig {
        self.try_configuration()
            .expect("validated compositor operation")
    }
    pub fn edit_configuration(
        &mut self,
        edit: impl FnOnce(&mut OperationConfig),
    ) -> Result<(), String> {
        let mut config = self.try_configuration()?;
        edit(&mut config);
        self.set_configuration(config)
    }
    pub fn set_configuration(&mut self, config: OperationConfig) -> Result<(), String> {
        config.validate()?;
        let mut next = record(&config);
        next.id = self.id;
        next.name = self.name.clone();
        next.extensions = self.operation.extensions.clone();
        if let (Some(before), Some(mut after)) = (self.native_instance()?, native_instance(&next)?)
        {
            after.reconcile(&before)?;
            next.parameters.get_mut("instance").unwrap().value =
                encode_value(serde_json::to_value(after).map_err(|e| e.to_string())?);
        }
        for (key, previous) in &self.operation.parameters {
            let replacement = next.parameters.get_mut(key);
            if previous.protected && replacement.as_ref().map(|p| &p.value) != Some(&previous.value)
            {
                return Err(format!("Protected compositor parameter {key}"));
            }
            if let Some(replacement) = replacement {
                replacement.id = previous.id;
                replacement.protected = previous.protected;
                replacement.extensions = previous.extensions.clone();
            }
        }
        self.operation = next;
        Ok(())
    }
}

// Domain structures (media descriptors, transforms and channel mappings) are
// validated by the registered operation contract. Primitive controls use the
// shared typed binding representation directly; component animation owns tracks.
fn encode_value(value: Value) -> Value {
    let literal = match value {
        Value::Bool(value) => Literal::Bool { value },
        Value::Number(value) => match value.as_i64() {
            Some(value) => Literal::Integer { value },
            None => Literal::Scalar {
                value: value.as_f64().expect("JSON number"),
            },
        },
        Value::String(value) => Literal::Text { value },
        Value::Array(values) => {
            return json!({"kind":"list", "values":values.into_iter().map(encode_value).collect::<Vec<_>>()});
        }
        value => return json!({"kind":"configuration", "value":value}),
    };
    serde_json::to_value(Binding::Literal { value: literal }).expect("literal binding")
}
fn decode_value(value: &Value) -> Result<Value, String> {
    if value.get("kind").and_then(Value::as_str) == Some("configuration") {
        if value
            .as_object()
            .is_none_or(|v| v.len() != 2 || !v.contains_key("value"))
        {
            return Err("Invalid domain configuration".into());
        }
        return Ok(value["value"].clone());
    }
    if value.get("kind").and_then(Value::as_str) == Some("list") {
        if value.as_object().is_none_or(|v| v.len() != 2) {
            return Err("Invalid binding list".into());
        }
        let values = value["values"].as_array().ok_or("Invalid binding list")?;
        return values
            .iter()
            .map(decode_value)
            .collect::<Result<Vec<_>, _>>()
            .map(Value::Array);
    }
    let binding: Binding = serde_json::from_value(value.clone()).map_err(|e| e.to_string())?;
    let literal = match binding {
        Binding::Literal { value } => value,

        _ => {
            return Err(
                "Compositor configuration requires a literal, list or domain configuration".into(),
            );
        }
    };
    literal.validate()?;
    match literal {
        Literal::Bool { value } => Ok(json!(value)),
        Literal::Integer { value } => Ok(json!(value)),
        Literal::Scalar { value } => Ok(json!(value)),
        Literal::Text { value } => Ok(json!(value)),
        Literal::Duration { value } => Ok(json!(value)),
        Literal::Distance { value, .. } => Ok(json!(value)),
        Literal::Vector { value, .. } => Ok(json!(value)),
        Literal::Color { value, .. } => Ok(json!(value)),
        _ => Err("Literal type is not a compositor configuration value".into()),
    }
}

fn native_instance(record: &Record) -> Result<Option<crate::instance::Instance>, String> {
    if record.operation != "Effect" || record.version != 1 {
        return Ok(None);
    }
    let parameter = record
        .parameters
        .get("instance")
        .ok_or("Missing instance record")?;
    serde_json::from_value(decode_value(&parameter.value)?)
        .map(Some)
        .map_err(|e| e.to_string())
}
