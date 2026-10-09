//! Feature-owned parameter discovery. Paths are a projection of stable targets;
//! sampling stays in the provider's evaluator and uses document time.
use fold_foundation::{DocumentId, ObjectId};
use fold_graph::channel;
pub use fold_graph::types::{Literal, Space, Type, Unit};
use serde::{Deserialize, Serialize};

/// Identity of a native graph parameter or a fixed provider-owned property.
/// Property names are contract keys, never user labels or JSON pointers.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum Key {
    Native { id: ObjectId },
    Property { name: String },
}
impl From<ObjectId> for Key {
    fn from(id: ObjectId) -> Self {
        Self::Native { id }
    }
}
impl Key {
    pub fn native(&self) -> Option<ObjectId> {
        match self {
            Self::Native { id } => Some(*id),
            Self::Property { .. } => None,
        }
    }
    pub fn property(&self) -> Option<&str> {
        match self {
            Self::Property { name } => Some(name),
            Self::Native { .. } => None,
        }
    }
}

#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Target {
    pub document: DocumentId,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub placement: Option<Placement>,
    /// Feature-owned object, such as a Motion scene object.
    pub object: Option<ObjectId>,
    /// Ordered native instance IDs below the document's root instance.
    pub occurrence: Vec<ObjectId>,
    pub parameter: Key,
    pub component: Option<u8>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Descriptor {
    pub target: Target,
    /// Unescaped segments relative to the document. Never used as identity.
    pub path: Vec<String>,
    /// None for structured controls (lists and ramps), which cannot be sampled
    /// as one primitive. They remain discoverable by their native identity.
    pub value_type: Option<Type>,
    pub editable: bool,
}

/// Resolves a readable path without choosing an arbitrary duplicate name.
/// `owner` is the path of the owning node/container, not its parameter.
pub fn resolve<'a>(
    catalog: &'a [Descriptor],
    owner: &[String],
    path: &str,
) -> Result<&'a Descriptor, String> {
    let path = channel::resolve_path(owner, path)?;
    let mut matches = catalog.iter().filter(|entry| entry.path == path);
    let result = matches.next().ok_or("Missing parameter path")?;
    if matches.next().is_some() {
        return Err("Ambiguous parameter path; use its stable target".into());
    }
    Ok(result)
}

pub fn absolute_path(entry: &Descriptor) -> String {
    format!(
        "/{}",
        entry
            .path
            .iter()
            .map(|s| channel::escape(s))
            .collect::<Vec<_>>()
            .join("/")
    )
}

/// Components use the same units as expressions. Color component names are
/// r/g/b/a; vector and rotation names are x/y/z/w.
pub fn with_components(entry: Descriptor) -> Vec<Descriptor> {
    let count = match entry.value_type.as_ref().map(Type::sampled) {
        Some(Type::Vector { dimensions, .. }) => *dimensions,
        Some(Type::Color { .. } | Type::Rotation { .. }) => 4,
        _ => 0,
    };
    let names = if matches!(
        entry.value_type.as_ref().map(Type::sampled),
        Some(Type::Color { .. })
    ) {
        ["r", "g", "b", "a"]
    } else {
        ["x", "y", "z", "w"]
    };
    let mut result = vec![];
    for component in 0..count {
        let mut value = entry.clone();
        value.target.component = Some(component);
        value.path.push(names[usize::from(component)].into());
        value.value_type = entry
            .value_type
            .as_ref()
            .and_then(|ty| channel::component_type(ty, Some(component)).ok());
        result.push(value);
    }
    result.insert(0, entry);
    result
}

pub fn select_component(value: Literal, target: &Target) -> Result<Literal, String> {
    channel::component_value(value, target.component)
}

/// Build a descriptor for a fixed property exposed by an owning provider.
pub fn property(
    document: DocumentId,
    object: Option<ObjectId>,
    name: &str,
    path: Vec<String>,
    value: &Literal,
    editable: bool,
) -> Vec<Descriptor> {
    with_components(Descriptor {
        target: Target {
            document,
            placement: None,
            object,
            occurrence: vec![],
            parameter: Key::Property { name: name.into() },
            component: None,
        },
        path,
        value_type: Some(value.ty()),
        editable,
    })
}

/// Placement of a source document below a root document. The ordered owner IDs
/// distinguish repeated uses of the same source. Names are never stored here.
#[derive(
    Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, schemars::JsonSchema,
)]
#[serde(deny_unknown_fields)]
pub struct Placement {
    pub root: DocumentId,
    pub owners: Vec<ObjectId>,
}

/// A provider-declared nested source and its exact output-time mapping.
#[derive(Clone, Debug)]
pub struct Source {
    pub owner: ObjectId,
    pub path: Vec<String>,
    pub reference: fold_project::DocumentRef,
    pub origin: fold_foundation::Time,
    pub source_start: fold_foundation::Time,
    pub editable: bool,
}
impl Source {
    pub fn at(&self, time: fold_foundation::Time) -> Result<fold_foundation::Time, String> {
        time.checked_sub(self.origin)
            .and_then(|t| t.checked_add(self.source_start))
            .map_err(|e| e.to_string())
    }
}
