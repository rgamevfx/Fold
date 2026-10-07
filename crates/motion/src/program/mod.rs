//! Live Rust programs construct reusable native groups. Instance edits stay native.
pub mod bundle;
pub mod compiler;
mod migration;
use crate::{
    Motion,
    fields::Kind,
    graph::{Graph, Input, Link, Node},
    nodes::interface::{GroupDefinition, Port},
};
use fold_foundation::ObjectId;
use fold_media::Cancel;
pub use migration::Migration;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::{BTreeMap, BTreeSet};

pub const METADATA: &str = "fold.motion.rust_assets";
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Blueprint {
    version: u32,
    name: String,
    inputs: Vec<Port>,
    nodes: Vec<NodeSpec>,
    output: Output,
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct NodeSpec {
    key: String,
    kind: String,
    settings: Value,
    inputs: BTreeMap<String, Argument>,
}
#[derive(Clone, Deserialize)]
#[serde(untagged)]
enum Argument {
    Expression {
        expression: crate::fields::expression::Expression,
    },
    Link {
        link: Output,
    },
}
#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct Output {
    key: String,
    socket: String,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Record {
    pub source: String,
    pub version: u32,
    ids: BTreeMap<String, ObjectId>,
    baseline: GroupDefinition,
    #[serde(flatten)]
    extensions: BTreeMap<String, Value>,
}
impl Blueprint {
    pub fn input_default(&self, id: &str) -> Option<crate::fields::Datum> {
        self.inputs
            .iter()
            .find(|p| p.id == id)
            .and_then(|p| p.default.clone())
    }
    fn validate(&self) -> Result<(), String> {
        if self.version != 1
            || self.name.trim().is_empty()
            || self.name.len() > 160
            || self.inputs.len() > 64
            || self.nodes.is_empty()
            || self.nodes.len() > 1024
        {
            return Err(
                "asset requires version 1, name, 1..1024 nodes and at most 64 inputs".into(),
            );
        }
        let mut keys = BTreeSet::new();
        for p in &self.inputs {
            key(&p.id)?;
            if !keys.insert(p.id.clone()) || p.kind == Kind::Generic {
                return Err("duplicate or untyped asset input".into());
            }
            if let Some(v) = &p.default {
                v.validate()?;
                if v.kind() != p.kind {
                    return Err("asset input default type mismatch".into());
                }
            }
        }
        keys.clear();
        for n in &self.nodes {
            key(&n.key)?;
            if !keys.insert(n.key.clone()) {
                return Err(format!("duplicate asset key: {}", n.key));
            }
        }
        Ok(())
    }
}
fn key(key: &str) -> Result<(), String> {
    if key.is_empty()
        || key.len() > 80
        || !key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "_-./".contains(c))
    {
        Err("asset keys require 1..80 ASCII letters, digits, underscore, dash, dot or slash".into())
    } else {
        Ok(())
    }
}
fn record_key(id: ObjectId) -> String {
    serde_json::to_value(id)
        .expect("identity")
        .as_str()
        .expect("UUID")
        .to_owned()
}
pub fn record(m: &Motion, definition: ObjectId) -> Result<Option<Record>, String> {
    m.extensions
        .get(METADATA)
        .and_then(|v| v.get(record_key(definition)))
        .cloned()
        .map(serde_json::from_value)
        .transpose()
        .map_err(|e| e.to_string())
}
fn normalized(mut group: GroupDefinition) -> GroupDefinition {
    for node in &mut group.graph.nodes {
        node.position = [0.; 2];
    }
    group
}
fn identity(ids: &mut BTreeMap<String, ObjectId>, key: &str) -> ObjectId {
    *ids.entry(key.into()).or_insert_with(ObjectId::new)
}
fn resolve(ids: &BTreeMap<String, ObjectId>, output: &Output) -> Result<Link, String> {
    Ok(Link {
        node: *ids
            .get(&output.key)
            .ok_or_else(|| format!("missing generated source {}", output.key))?,
        socket: output.socket.clone(),
    })
}

/// Compile and revise a definition atomically. This does not create an instance.
pub fn define(
    m: &mut Motion,
    definition: ObjectId,
    source: &str,
    cancel: &Cancel,
) -> Result<(), String> {
    define_with_migrations(m, definition, source, &[], cancel)
}

pub fn define_with_migrations(
    m: &mut Motion,
    definition: ObjectId,
    source: &str,
    migrations: &[Migration],
    cancel: &Cancel,
) -> Result<(), String> {
    let previous = record(m, definition)?;
    if previous.as_ref().is_some_and(|r| r.version != 1) {
        return Err("unsupported Rust asset version".into());
    }
    if let Some(old) = m.groups.get(&definition) {
        let record = previous
            .as_ref()
            .ok_or("definition is manually authored; choose a new asset definition")?;
        if normalized(old.clone()) != normalized(record.baseline.clone()) {
            return Err("generated internals changed; preserve them in an independent group before replacing source".into());
        }
    } else if previous.is_some() {
        return Err("asset definition was deleted; use a new definition identity".into());
    }
    let blueprint = compiler::compile(source, cancel)?;
    migration::validate(m.groups.get(&definition), &blueprint.inputs, migrations)?;
    let mut candidate = m.clone();
    migration::instances(&mut candidate, definition, migrations)?;
    let mut ids = previous.as_ref().map(|r| r.ids.clone()).unwrap_or_default();
    migration::identities(&mut ids, migrations);
    // Keep removed keys as tombstones; surviving/reintroduced declarations keep identity.
    let mut nodes = vec![];
    for p in &blueprint.inputs {
        let mut n = Node::new("fold.motion.group_input")?;
        n.id = identity(&mut ids, &format!("@{}", p.id));
        n.settings = json!({"key":p.id,"value_type":p.kind});
        nodes.push(n);
    }
    for spec in &blueprint.nodes {
        identity(&mut ids, &spec.key);
    }
    let defaults: BTreeMap<_, _> = blueprint
        .inputs
        .iter()
        .filter_map(|p| p.default.clone().map(|v| (p.id.clone(), v)))
        .collect();
    for spec in &blueprint.nodes {
        cancel.check()?;
        let mut n = Node::new(&spec.kind).map_err(|e| format!("{}: {e}", spec.key))?;
        n.id = ids[&spec.key];
        n.patch_settings(&spec.settings)?;
        if n.kind == "fold.motion.text" && n.settings.get("font").is_none_or(Value::is_null) {
            let font_key = format!("!font/{}", spec.key);
            let font = if let Some(font) = ids.get(&font_key) {
                if !candidate.fonts.contains_key(font) {
                    return Err(format!("{}: embedded font was removed", spec.key));
                }
                *font
            } else {
                let font = candidate
                    .fonts
                    .keys()
                    .next()
                    .copied()
                    .unwrap_or_else(ObjectId::new);
                ids.insert(font_key, font);
                font
            };
            candidate
                .fonts
                .entry(font)
                .or_insert_with(crate::document::Font::builtin);
            n.settings["font"] = json!(font);
        }
        if n.kind == "fold.motion.wrangle" {
            crate::wrangle::sync_controls(&mut n)?;
            let settings = n.settings::<crate::wrangle::Settings>()?;
            crate::wrangle::compiler::compile(&settings.source, &settings.channels, cancel)?;
        }
        // Specialization may change default types.
        let signature = candidate.signature(&n)?;
        n.inputs = signature
            .0
            .into_iter()
            .filter_map(|p| p.default.map(|v| (p.id, Input::Value(v))))
            .collect();
        for (socket, arg) in &spec.inputs {
            let input = match arg {
                Argument::Link { link } => Input::Link(resolve(&ids, link)?),
                Argument::Expression { expression } => {
                    use crate::fields::expression::Expression;
                    match expression {
                        Expression::Literal { value } => Input::Value(value.clone()),
                        Expression::Input { name } => Input::Link(resolve(
                            &ids,
                            &Output {
                                key: format!("@{name}"),
                                socket: "value".into(),
                            },
                        )?),
                        _ => {
                            let mut e = Node::new("fold.motion.expression")?;
                            e.id = identity(&mut ids, &format!("${}/{socket}", spec.key));
                            e.settings = json!({"expression":expression,"inputs":defaults});
                            for name in defaults.keys() {
                                e.connect(name, ids[&format!("@{name}")], "value");
                            }
                            let link = Input::Link(Link {
                                node: e.id,
                                socket: "value".into(),
                            });
                            nodes.push(e);
                            link
                        }
                    }
                }
            };
            n.inputs.insert(socket.clone(), input);
        }
        nodes.push(n);
    }
    if ids.len() > 4096 {
        return Err("asset identity history exceeds 4096 keys; create a new definition".into());
    }
    let output_source = resolve(&ids, &blueprint.output)?;
    let mut output_node = Node::new("fold.motion.group_output")?;
    output_node.id = identity(&mut ids, "!output");
    output_node.settings = json!({"value_type":"Content"});
    output_node
        .inputs
        .insert("value".into(), Input::Link(output_source));
    let output = Link {
        node: output_node.id,
        socket: "value".into(),
    };
    nodes.push(output_node);
    let old = candidate.groups.get(&definition);
    for (i, n) in nodes.iter_mut().enumerate() {
        n.position = old
            .and_then(|g| g.graph.node(n.id).ok())
            .map(|n| n.position)
            .unwrap_or([(i % 5) as f32 * 240., (i / 5) as f32 * 150.]);
    }
    let group = GroupDefinition {
        name: blueprint.name,
        version: 1,
        inputs: blueprint.inputs,
        outputs: BTreeMap::from([("content".into(), output.clone())]),
        graph: Graph { nodes, output },
    };
    // Existing instance values, keys, connections, names and placement are untouched.
    for n in candidate.graph.nodes.iter_mut().chain(
        candidate
            .groups
            .values_mut()
            .flat_map(|g| g.graph.nodes.iter_mut()),
    ) {
        if n.kind == "fold.motion.group_instance"
            && n.settings.get("group") == Some(&json!(definition))
        {
            for port in &group.inputs {
                if let Some(v) = &port.default {
                    n.inputs
                        .entry(port.id.clone())
                        .or_insert_with(|| Input::Value(v.clone()));
                }
            }
        }
    }
    candidate.groups.insert(definition, group.clone());
    let record = Record {
        source: source.into(),
        version: 1,
        ids,
        baseline: group,
        extensions: previous
            .as_ref()
            .map(|r| r.extensions.clone())
            .unwrap_or_default(),
    };
    let metadata = candidate
        .extensions
        .entry(METADATA.into())
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .ok_or("invalid asset metadata")?;
    if metadata.len() >= 128 && !metadata.contains_key(&record_key(definition)) {
        return Err("at most 128 Rust asset definitions".into());
    }
    metadata.insert(
        record_key(definition),
        serde_json::to_value(record).map_err(|e| e.to_string())?,
    );
    candidate.validate()?;
    // Asset content must be a real content output, not an accidentally scalar graph.
    let g = &candidate.groups[&definition];
    let node = g.graph.node(g.graph.output.node)?;
    if !candidate
        .signature(node)?
        .1
        .iter()
        .any(|(s, k)| s == &g.graph.output.socket && *k == Kind::Content)
    {
        return Err("Rust asset output must be Content".into());
    }
    cancel.check()?;
    *m = candidate;
    Ok(())
}

pub fn instantiate(m: &mut Motion, definition: ObjectId, id: ObjectId) -> Result<(), String> {
    if m.graph.nodes.iter().any(|n| n.id == id) {
        return Err("asset instance identity already exists".into());
    }
    let group = m
        .groups
        .get(&definition)
        .ok_or("missing asset definition")?;
    let mut node = Node::new("fold.motion.group_instance")?;
    node.id = id;
    node.settings = json!({"group":definition});
    node.inputs = group
        .inputs
        .iter()
        .filter_map(|p| p.default.clone().map(|v| (p.id.clone(), Input::Value(v))))
        .collect();
    m.graph.nodes.push(node);
    Ok(())
}

/// A visible procedural source whose transform/timing are owned by Composition.
pub fn scene_instance(m: &mut Motion, definition: ObjectId, id: ObjectId) -> Result<(), String> {
    instantiate(m, definition, id)?;
    let link = Link {
        node: id,
        socket: "content".into(),
    };
    let name = m.groups[&definition].name.clone();
    let state = fold_composition::layer::LayerState::new(name, m.info.time(m.info.frames)?);
    let mut state = state;
    state.id = id;
    m.scene.get_or_insert_with(Default::default).objects.insert(
        0,
        crate::scene::Object {
            state,
            source: link.clone(),
            output: link,
            transform: None,
            appearance: None,
            modifiers: vec![],
            isolated: false,
            masks: vec![],
            constraints: vec![],
            extensions: Default::default(),
        },
    );
    Ok(())
}
