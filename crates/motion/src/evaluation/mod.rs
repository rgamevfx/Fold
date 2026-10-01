//! Headless demand evaluation. Node dispatch is registry-driven; fields remain
//! expressions until a geometry operation selects an explicit element domain.
use crate::{
    document::Motion,
    fields::{Budget, Datum, Field, Kind},
    geometry::Content,
    graph::{Graph, Input, Link, Node, registry},
};
use fold_foundation::{ObjectId, Time};
use fold_render::{ImageOp, RenderGraph};
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone, Debug)]
pub enum Value {
    Field(Field),
    Content(Content),
    Points(Content),
}
impl Value {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Field(f) => f.kind,
            Self::Content(_) => Kind::Content,
            Self::Points(_) => Kind::Points,
        }
    }
}
pub type Outputs = BTreeMap<String, Value>;
pub fn output(name: &str, value: Value) -> Outputs {
    BTreeMap::from([(name.into(), value)])
}
pub fn field(value: Field) -> Outputs {
    output("value", Value::Field(value))
}
pub fn content(value: Content) -> Outputs {
    output("content", Value::Content(value))
}
pub struct Evaluator<'a> {
    pub motion: &'a Motion,
    pub time: Time,
    graph: &'a Graph,
    pub(crate) bindings: BTreeMap<String, Value>,
    pub(crate) group_depth: usize,
    pub budget: Budget,
    cache: BTreeMap<ObjectId, Outputs>,
    active: BTreeSet<ObjectId>,
}
impl<'a> Evaluator<'a> {
    pub fn new(motion: &'a Motion, time: Time) -> Result<Self, String> {
        motion.validate()?;
        Self::for_graph(motion, &motion.graph, time)
    }
    pub(crate) fn for_graph(
        motion: &'a Motion,
        graph: &'a Graph,
        time: Time,
    ) -> Result<Self, String> {
        Ok(Self {
            motion,
            time,
            graph,
            bindings: BTreeMap::new(),
            group_depth: 0,
            budget: Budget::default(),
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
        })
    }
    pub fn resolve(&mut self, link: &Link) -> Result<Value, String> {
        if !self.cache.contains_key(&link.node) {
            if self.active.len() >= 128 || !self.active.insert(link.node) {
                return Err("recursive graph evaluation".into());
            }
            self.budget.spend()?;
            let node = self.graph.node(link.node)?.clone();
            let def = registry::find(&node.kind)?;
            if node.version != 1 {
                return Err(format!(
                    "unsupported {} node version {}",
                    node.kind, node.version
                ));
            }
            let result = (def.evaluate)(self, &node)
                .map_err(|e| format!("{} {:?}: {e}", def.name, node.id))?;
            for (name, value) in &result {
                if !self
                    .motion
                    .signature(&node)?
                    .1
                    .iter()
                    .any(|(key, kind)| key == name && *kind == value.kind())
                {
                    return Err(format!("invalid output {name} from {}", node.kind));
                }
            }
            self.active.remove(&link.node);
            self.cache.insert(link.node, result);
        }
        self.cache[&link.node]
            .get(&link.socket)
            .cloned()
            .ok_or_else(|| format!("missing output {} at {:?}", link.socket, link.node))
    }
    pub fn input(&mut self, node: &Node, socket: &str) -> Result<Value, String> {
        let signature = self.motion.signature(node)?;
        let slot = signature
            .0
            .iter()
            .find(|s| s.id == socket)
            .ok_or_else(|| format!("unknown input {socket}"))?;
        let result = match node.inputs.get(socket) {
            Some(Input::Value(value)) => Value::Field(Field::constant(value.clone())),
            Some(Input::Link(link)) => self.resolve(link)?,
            None => {
                if let Some(value) = &slot.default {
                    Value::Field(Field::constant(value.clone()))
                } else {
                    return Err(format!("connect required input {socket}"));
                }
            }
        };
        if result.kind() != slot.kind {
            return Err(format!(
                "{socket}: expected {:?}, got {:?}",
                slot.kind,
                result.kind()
            ));
        }
        Ok(result)
    }
    pub fn field(&mut self, node: &Node, socket: &str) -> Result<Field, String> {
        if let Value::Field(f) = self.input(node, socket)? {
            Ok(f)
        } else {
            Err(format!("{socket} requires a field"))
        }
    }
    pub fn uniform(&mut self, node: &Node, socket: &str) -> Result<Datum, String> {
        self.field(node, socket)?.uniform(&mut self.budget)
    }
    pub fn scalar(&mut self, node: &Node, socket: &str) -> Result<f64, String> {
        self.uniform(node, socket)?.scalar()
    }
    pub fn vector(&mut self, node: &Node, socket: &str) -> Result<[f64; 2], String> {
        self.uniform(node, socket)?.vector()
    }
    pub fn content(&mut self, node: &Node, socket: &str) -> Result<Content, String> {
        match self.input(node, socket)? {
            Value::Content(c) | Value::Points(c) => Ok(c),
            _ => Err(format!("{socket} requires geometry")),
        }
    }
}
pub fn compile(
    motion: &Motion,
    time: Time,
    width: u32,
    height: u32,
) -> Result<RenderGraph, String> {
    motion.info.frame_at(time)?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 4_194_304 {
        return Err("invalid motion output resolution".into());
    }
    let mut evaluator = Evaluator::new(motion, time)?;
    let value = evaluator.resolve(&motion.graph.output)?;
    let Value::Content(content) = value else {
        return Err("motion output must be scene content".into());
    };
    let drawings = crate::geometry::drawings(
        &content,
        [
            width as f64 / motion.info.width as f64,
            height as f64 / motion.info.height as f64,
        ],
    )?;
    Ok(RenderGraph {
        width,
        height,
        nodes: vec![ImageOp::Vector(Arc::new(drawings))],
        output: 0,
    })
}
