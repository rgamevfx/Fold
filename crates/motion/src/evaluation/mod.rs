//! Headless demand evaluation. Node dispatch is registry-driven; fields remain
//! expressions until a geometry operation selects an explicit element domain.
pub(crate) mod layers;
mod relationships;
mod temporal;
use crate::{
    document::Motion,
    fields::{Budget, Datum, Field, Kind},
    geometry::Content,
    graph::{Graph, Input, Link, Node, registry},
};
use fold_foundation::{ObjectId, Time};
use fold_render::RenderGraph;
use std::{
    collections::{BTreeMap, BTreeSet},
    sync::Arc,
};
#[derive(Clone, Debug)]
pub enum Value {
    Image(Arc<Vec<fold_platform::image_source::Request>>),
    ImageField(Arc<crate::nodes::image::Samples>),
    Field(Field),
    Content(Content),
    Points(Content),
}
impl Value {
    pub fn kind(&self) -> Kind {
        match self {
            Self::Field(f) => f.kind,
            Self::Image(_) => Kind::Image,
            Self::ImageField(_) => Kind::ImageField,
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
    pub(crate) scene_output: Option<ObjectId>,
    pub(crate) layer_times: Arc<BTreeMap<ObjectId, Time>>,
    graph: &'a Graph,
    pub(crate) bindings: BTreeMap<String, Value>,
    pub(crate) group_depth: usize,
    pub(crate) object_path: BTreeSet<ObjectId>,
    pub(crate) current_object: Option<ObjectId>,
    pub budget: Budget,
    pub(crate) runtime: Arc<crate::simulation::Runtime>,
    pub(crate) identity: Option<Arc<Vec<u8>>>,
    pub(crate) scope: Vec<ObjectId>,
    pub(crate) step_context: Option<crate::nodes::solver::StepContext>,
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
            scene_output: None,
            layer_times: Default::default(),
            graph,
            bindings: BTreeMap::new(),
            group_depth: 0,
            object_path: BTreeSet::new(),
            current_object: None,
            budget: Budget::default(),
            runtime: Arc::new(crate::simulation::Runtime::default()),
            identity: None,
            scope: Vec::new(),
            step_context: None,
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
        })
    }
    fn object_time(&self, id: ObjectId) -> Result<Time, String> {
        let current = self
            .current_object
            .and_then(|id| self.layer_times.get(&id))
            .copied()
            .unwrap_or(Time::ZERO);
        let target = self.layer_times.get(&id).copied().unwrap_or(Time::ZERO);
        self.time
            .checked_sub(current)
            .and_then(|t| t.checked_add(target))
            .map_err(|e| e.to_string())
    }
    /// References use source objects regardless of their direct-render visibility.
    pub fn object(&mut self, id: ObjectId) -> Result<Value, String> {
        if self.object_path.len() >= 64 || self.object_path.contains(&id) {
            return Err("cyclic scene object reference".into());
        }
        let object = self
            .motion
            .scene
            .as_ref()
            .and_then(|s| s.objects.iter().find(|o| o.id == id))
            .ok_or("missing referenced scene object")?;
        let mut nested = Self::for_graph(self.motion, &self.motion.graph, self.object_time(id)?)?;
        nested.layer_times = self.layer_times.clone();
        nested.object_path = self.object_path.clone();
        nested.object_path.insert(id);
        nested.current_object = Some(id);
        nested.runtime = self.runtime.clone();
        nested.identity = self.identity.clone();
        nested.step_context = self.step_context.clone();
        nested.group_depth = self.group_depth;
        nested.budget = std::mem::take(&mut self.budget);
        let result = nested.resolve(&object.output).and_then(|value| {
            let Value::Content(content) = value else {
                return Err("object output requires content".into());
            };
            nested.scene_effects(id, content).map(Value::Content)
        });
        self.budget = nested.budget;
        result
    }
    pub fn children(&mut self, id: ObjectId) -> Result<Value, String> {
        let scene = self
            .motion
            .scene
            .as_ref()
            .ok_or("scene group requires a scene")?;
        let mut elements = vec![];
        for child in scene.objects.iter().rev().filter(|o| o.parent == Some(id)) {
            if child.active(self.object_time(child.id)?)
                && let Value::Content(content) = self.object(child.id)?
            {
                elements.extend(content.iter().cloned());
            }
        }
        Ok(Value::Content(Arc::new(elements)))
    }
    pub fn scene(&mut self) -> Result<Value, String> {
        let Some(scene) = &self.motion.scene else {
            return self.resolve(&self.motion.graph.output);
        };
        if let Some(id) = self.scene_output.or(scene.output) {
            let object = scene
                .objects
                .iter()
                .find(|o| o.id == id)
                .ok_or("missing output object")?;
            return if self.time >= object.start && self.time < object.end {
                self.object(id)
            } else {
                Ok(Value::Content(Arc::new(vec![])))
            };
        }
        let mut elements = Vec::new();
        for object in scene.objects.iter().rev() {
            if object.parent.is_none() && object.active(self.time) {
                let Value::Content(content) = self.object(object.id)? else {
                    return Err("object output must be content".into());
                };
                elements.extend(content.iter().cloned());
            }
        }
        Ok(Value::Content(Arc::new(elements)))
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
            Some(Input::Value(value)) => {
                if node.animation.is_empty() {
                    Value::Field(Field::constant(value.clone()))
                } else {
                    let node = node.clone();
                    let socket = socket.to_owned();
                    let value = value.clone();
                    let time = self.time;
                    Value::Field(Field::new(value.kind(), false, move |s, _| {
                        let time = time
                            .checked_sub(s.time_offset.unwrap_or(Time::ZERO))
                            .map_err(|e| e.to_string())?;
                        Ok(node.sample_input(&socket, &value, time))
                    }))
                }
            }
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
    compile_with(motion, time, width, height, &fold_media::Cancel::default())
}

pub fn compile_with(
    motion: &Motion,
    time: Time,
    width: u32,
    height: u32,
    cancel: &fold_media::Cancel,
) -> Result<RenderGraph, String> {
    compile_runtime(
        motion,
        time,
        width,
        height,
        cancel,
        Arc::new(crate::simulation::Runtime::default()),
    )
}

pub fn compile_runtime(
    motion: &Motion,
    time: Time,
    width: u32,
    height: u32,
    cancel: &fold_media::Cancel,
    runtime: Arc<crate::simulation::Runtime>,
) -> Result<RenderGraph, String> {
    cancel.check()?;
    let evaluator = Evaluator::with_runtime(motion, time, runtime)?;
    compile_evaluator(evaluator, width, height, cancel)
}

/// The provider supplies a validated immutable root and its cached identity.
pub(crate) fn compile_evaluator(
    evaluator: Evaluator<'_>,
    width: u32,
    height: u32,
    cancel: &fold_media::Cancel,
) -> Result<RenderGraph, String> {
    compile_evaluator_images(evaluator, width, height, cancel, &|_| {
        Err("image inputs require a project render context".into())
    })
}
pub(crate) fn compile_evaluator_images(
    mut evaluator: Evaluator<'_>,
    width: u32,
    height: u32,
    cancel: &fold_media::Cancel,
    resolve: &dyn Fn(&fold_platform::image_source::Request) -> Result<RenderGraph, String>,
) -> Result<RenderGraph, String> {
    cancel.check()?;
    let motion = evaluator.motion;
    motion.info.frame_at(evaluator.time)?;
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > 4_194_304 {
        return Err("invalid motion output resolution".into());
    }
    evaluator.budget = Budget::cancellable(cancel);
    let value = evaluator.scene()?;
    let Value::Content(content) = value else {
        return Err("motion output must be scene content".into());
    };
    crate::geometry::render::compile_images(
        &content,
        width,
        height,
        [
            width as f64 / motion.info.width as f64,
            height as f64 / motion.info.height as f64,
        ],
        cancel,
        resolve,
    )
}
