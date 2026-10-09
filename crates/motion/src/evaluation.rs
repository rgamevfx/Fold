//! Immutable CPU reference snapshots. Rendering consumes their geometry later.
mod animator;
pub mod cloner;
pub mod distribution;
pub mod extrude;
pub mod material;
pub mod mesh;
pub mod outlines;
pub mod placement;
mod profile;
mod sampling;
pub mod spatial;
pub mod style;
pub mod text;
mod text_animator;
mod transform;
use crate::{
    binding::Binding,
    execution::{Budget, Context},
    model::{Construction, Instance, Reference},
    registry::{self, Builtin},
    types::{Literal, Space, Unit},
};
use fold_foundation::{ObjectId, Time};
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Debug, PartialEq)]
pub struct Point {
    pub id: u64,
    pub position: [f64; 2],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Contour {
    pub id: u64,
    pub points: Vec<Point>,
    pub closed: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub enum GeometryData {
    Contours(Arc<Vec<Contour>>),
    Text(Arc<text::Text>),
    Outlines(Arc<outlines::TextOutlines>),
    Solids(Arc<extrude::TextSolids>),
    Mesh(Arc<mesh::Mesh>),
    Instances(Arc<cloner::Instances>),
}
#[derive(Clone, Debug, PartialEq)]
pub struct Geometry {
    /// Topology identity is shared by appearance-only branches.
    pub id: u64,
    pub space: Space,
    pub unit: Unit,
    pub data: GeometryData,
    /// Appearance does not change topology identity or duplicate retained contours.
    pub appearance: style::Appearance,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Placement {
    pub id: u64,
    pub geometry: Arc<Geometry>,
    pub transform: placement::Transform,
    pub opacity: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct SceneEntry {
    pub id: u64,
    pub placement: Arc<Placement>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Scene {
    pub entries: Vec<SceneEntry>,
    pub camera: Option<Arc<spatial::Camera>>,
    pub cameras: Vec<spatial::CameraEntry>,
    pub lights: Vec<spatial::LightEntry>,
}
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Primitive(Literal),
    Points(Arc<distribution::Points>),
    Geometry(Arc<Geometry>),
    Placement(Arc<Placement>),
    Scene(Arc<Scene>),
    Camera(Arc<spatial::Camera>),
    Light(Arc<spatial::Light>),
}
impl Value {
    pub fn primitive(&self) -> Result<&Literal, String> {
        match self {
            Self::Primitive(value) => Ok(value),
            _ => Err("Expected primitive value".into()),
        }
    }
}
#[derive(Clone, Copy, Debug)]
pub struct Limits {
    pub steps: usize,
    /// Generated geometry points, contours, placements and scene entries.
    pub elements: usize,
}
impl Default for Limits {
    fn default() -> Self {
        Self {
            steps: 1_000_000,
            elements: 100_000,
        }
    }
}
struct Operation {
    builtin: Builtin,
    bindings: BTreeMap<String, Binding>,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FailureKind {
    Invalid,
    Budget,
    Cancelled,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Failure {
    pub kind: FailureKind,
    pub message: String,
    pub context: Option<FailureContext>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FailureContext {
    pub operation: ObjectId,
    pub definition: ObjectId,
    pub instance_path: Vec<ObjectId>,
    pub time: Time,
}

/// Owns a validated immutable document and decoded operation arguments.
/// Preparing a new edit never changes snapshots already held by consumers.
pub struct Prepared {
    construction: Construction,
    operations: BTreeMap<ObjectId, Operation>,
}
impl Prepared {
    pub fn new(construction: Construction) -> Result<Self, String> {
        Self::new_detailed(construction).map_err(|error| error.message)
    }
    pub fn new_detailed(construction: Construction) -> Result<Self, crate::validation::Diagnostic> {
        construction.validate_semantics_detailed()?;
        let mut operations = BTreeMap::new();
        for definition in &construction.definitions {
            for node in &definition.nodes {
                let contract = registry::resolve(&node.operation, node.version, &node.mode)?;
                let bindings = node
                    .parameters
                    .iter()
                    .map(|(name, parameter)| Ok((name.clone(), parameter.binding()?)))
                    .collect::<Result<_, String>>()?;
                operations.insert(
                    node.id,
                    Operation {
                        builtin: contract.builtin,
                        bindings,
                    },
                );
            }
        }
        Ok(Self {
            construction,
            operations,
        })
    }
    pub fn construction(&self) -> &Construction {
        &self.construction
    }
    pub fn evaluate(
        &self,
        output: ObjectId,
        time: Time,
        limits: Limits,
        cancellation: &dyn Fn() -> Result<(), String>,
    ) -> Result<Value, String> {
        self.evaluate_detailed(output, time, limits, cancellation)
            .map_err(|error| error.message)
    }
    pub fn evaluate_detailed(
        &self,
        output: ObjectId,
        time: Time,
        limits: Limits,
        cancellation: &dyn Fn() -> Result<(), String>,
    ) -> Result<Value, Failure> {
        self.evaluate_scoped(output, time, None, limits, cancellation)
    }
    /// Scene occurrence identity is evaluation context, never a document rewrite.
    pub(crate) fn evaluate_in_object(
        &self,
        output: ObjectId,
        time: Time,
        object: ObjectId,
        limits: Limits,
        cancellation: &dyn Fn() -> Result<(), String>,
    ) -> Result<Value, String> {
        self.evaluate_scoped(output, time, Some(object), limits, cancellation)
            .map_err(|e| e.message)
    }
    fn evaluate_scoped(
        &self,
        output: ObjectId,
        time: Time,
        object: Option<ObjectId>,
        limits: Limits,
        cancellation: &dyn Fn() -> Result<(), String>,
    ) -> Result<Value, Failure> {
        let mut state = State {
            prepared: self,
            scene_time: time,
            budget: Budget::new(limits.steps, cancellation),
            remaining_elements: limits.elements,
            failure: None,
            failure_context: None,
            values: BTreeMap::new(),
            scopes: BTreeMap::new(),
            identities: BTreeMap::new(),
            depth: 0,
            text: None,
            element: None,
        };
        let result = (|| {
            let scene_scope = object.map(|object| Scope {
                definition: self.construction.root.definition,
                path: vec![object],
                time,
                inputs: Arc::new(BTreeMap::new()),
            });
            let root = state.instance(scene_scope.as_ref(), &self.construction.root)?;
            state.output(&root, output)
        })();
        result.map_err(|message| Failure {
            kind: match state.budget.stop() {
                Some(crate::execution::Stop::Cancelled) => FailureKind::Cancelled,
                Some(crate::execution::Stop::Exhausted) => FailureKind::Budget,
                None => state.failure.unwrap_or(FailureKind::Invalid),
            },
            message,
            context: state.failure_context,
        })
    }
}
#[derive(Clone)]
struct Scope {
    definition: ObjectId,
    path: Vec<ObjectId>,
    time: Time,
    inputs: Arc<BTreeMap<ObjectId, Literal>>,
}
struct State<'a> {
    prepared: &'a Prepared,
    scene_time: Time,
    budget: Budget<'a>,
    remaining_elements: usize,
    failure: Option<FailureKind>,
    failure_context: Option<FailureContext>,
    values: BTreeMap<(Vec<ObjectId>, ObjectId), Value>,
    scopes: BTreeMap<Vec<ObjectId>, Scope>,
    identities: BTreeMap<u64, Vec<u8>>,
    depth: usize,
    text: Option<fold_render::text::Service>,
    element: Option<crate::execution::ElementContext>,
}
impl State<'_> {
    fn reserve(&mut self, count: usize) -> Result<(), String> {
        self.remaining_elements = self.remaining_elements.checked_sub(count).ok_or_else(|| {
            self.failure = Some(FailureKind::Budget);
            "Generated element budget exhausted".to_string()
        })?;
        self.budget.step()
    }
    /// IDs derive from operation UUID, instance path, upstream identity and semantic slot.
    /// Full source keys are retained during a request to detect 64-bit collisions.
    fn identity(
        &mut self,
        scope: &Scope,
        operation: ObjectId,
        upstream: u64,
        slot: &[u8],
    ) -> Result<u64, String> {
        let mut key = operation.to_bytes().to_vec();
        key.extend_from_slice(&(scope.path.len() as u64).to_le_bytes());
        for id in &scope.path {
            key.extend_from_slice(&id.to_bytes());
        }
        key.extend_from_slice(&upstream.to_le_bytes());
        key.extend_from_slice(slot);
        let id = key.iter().fold(0xcbf29ce484222325u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x100000001b3)
        });
        if let Some(previous) = self.identities.get(&id) {
            if previous != &key {
                return Err("Generated element identity collision".into());
            }
        } else {
            self.identities.insert(id, key);
        }
        Ok(id)
    }
    fn instance(&mut self, parent: Option<&Scope>, instance: &Instance) -> Result<Scope, String> {
        self.budget.step()?;
        let mut path = parent.map(|scope| scope.path.clone()).unwrap_or_default();
        path.push(instance.id);
        if let Some(scope) = self.scopes.get(&path) {
            return Ok(scope.clone());
        }
        if path.len() > 256 {
            self.failure = Some(FailureKind::Budget);
            return Err("Asset instance depth exceeds 256".into());
        }
        let parent_time = parent.map(|scope| scope.time).unwrap_or(self.scene_time);
        let root_parent = Scope {
            definition: instance.definition,
            path: vec![],
            time: parent_time,
            inputs: Arc::new(BTreeMap::new()),
        };
        let binding_scope = parent.unwrap_or(&root_parent);
        let definition = self.prepared.construction.definition(instance.definition)?;
        let mut inputs = BTreeMap::new();
        for input in &definition.inputs {
            self.budget.step()?;
            let value = if let Some(parameter) = instance.overrides.get(&input.id) {
                match parameter.binding()? {
                    Binding::Track { track } => {
                        for _ in &track.keys {
                            self.budget.step()?;
                        }
                        track.sample(parent_time)?
                    }
                    binding => self
                        .binding(binding_scope, instance.id, &binding)?
                        .primitive()?
                        .clone(),
                }
            } else {
                input.default.clone()
            };
            inputs.insert(input.id, value);
        }
        let scope = Scope {
            definition: instance.definition,
            path: path.clone(),
            time: instance.clock.at(parent_time)?,
            inputs: Arc::new(inputs),
        };
        self.scopes.insert(path, scope.clone());
        Ok(scope)
    }
    fn output(&mut self, scope: &Scope, output: ObjectId) -> Result<Value, String> {
        self.budget.step()?;
        let definition = self.prepared.construction.definition(scope.definition)?;
        let output = definition
            .outputs
            .iter()
            .find(|o| o.id == output)
            .ok_or("Output does not belong to definition")?;
        self.reference(scope, output.value.target)
    }
    fn reference(&mut self, scope: &Scope, reference: Reference) -> Result<Value, String> {
        self.budget.step()?;
        if self.depth >= 256 {
            self.failure = Some(FailureKind::Budget);
            return Err("Evaluation dependency depth exceeds 256".into());
        }
        self.depth += 1;
        let result = self.reference_inner(scope, reference);
        self.depth -= 1;
        result
    }
    fn reference_inner(&mut self, scope: &Scope, reference: Reference) -> Result<Value, String> {
        match reference {
            Reference::Input { input } => scope
                .inputs
                .get(&input)
                .cloned()
                .map(Value::Primitive)
                .ok_or("Input outside evaluation scope".into()),
            Reference::Node { node } => self.node(scope, node),
            Reference::Instance { instance, output } => {
                let definition = self.prepared.construction.definition(scope.definition)?;
                let instance = definition
                    .instances
                    .iter()
                    .find(|i| i.id == instance)
                    .ok_or("Instance outside evaluation scope")?;
                let child = self.instance(Some(scope), instance)?;
                self.output(&child, output)
            }
        }
    }
    fn binding(
        &mut self,
        scope: &Scope,
        owner: ObjectId,
        binding: &Binding,
    ) -> Result<Value, String> {
        self.budget.step()?;
        match binding {
            Binding::Literal { value } => Ok(Value::Primitive(value.clone())),
            Binding::Link { reference } => self.reference(scope, reference.target),
            Binding::Expression { expression } => {
                // Resolve dependencies before borrowing the shared expression budget.
                let mut values = BTreeMap::new();
                for reference in expression.references()? {
                    if let std::collections::btree_map::Entry::Vacant(entry) =
                        values.entry(reference)
                    {
                        entry.insert(self.reference(scope, reference)?.primitive()?.clone());
                    }
                }
                let context = Context {
                    scene_time: self.scene_time,
                    asset_time: scope.time,
                    seed: self.prepared.construction.seed,
                    operation: owner.to_bytes(),
                    element: self.element.clone(),
                };
                Ok(Value::Primitive(expression.evaluate(
                    &context,
                    &mut self.budget,
                    &mut |reference| {
                        values
                            .get(&reference)
                            .cloned()
                            .ok_or("Unresolved expression reference".into())
                    },
                )?))
            }
            _ => Err("Track/list requires its owning operation".into()),
        }
    }
    fn node(&mut self, scope: &Scope, id: ObjectId) -> Result<Value, String> {
        self.node_inner(scope, id).inspect_err(|_| {
            // Record the innermost failing operation once; consumers unwinding
            // above it must not replace its identity or local instance clock.
            if self.failure_context.is_none() {
                self.failure_context = Some(FailureContext {
                    operation: id,
                    definition: scope.definition,
                    instance_path: scope.path.clone(),
                    time: scope.time,
                });
            }
        })
    }
    fn node_inner(&mut self, scope: &Scope, id: ObjectId) -> Result<Value, String> {
        let key = (scope.path.clone(), id);
        if let Some(value) = self.values.get(&key) {
            return Ok(value.clone());
        }
        self.budget.step()?;
        let operation = self
            .prepared
            .operations
            .get(&id)
            .ok_or("Unknown operation")?;
        let mut args = BTreeMap::new();
        for (name, binding) in &operation.bindings {
            if (operation.builtin == Builtin::TextAnimatorWords && name == "position")
                || (operation.builtin == Builtin::InstanceScale && name == "factor")
                || (operation.builtin == Builtin::InstanceColor && name == "color")
                || (operation.builtin == Builtin::InstanceRotationZ && name == "angle")
            {
                continue;
            }
            if !matches!(
                binding,
                Binding::Track { .. } | Binding::Ramp { .. } | Binding::List { .. }
            ) {
                args.insert(name.as_str(), self.binding(scope, id, binding)?);
            }
        }
        let primitive = |name: &str| {
            args.get(name)
                .ok_or_else(|| format!("Missing {name}"))?
                .primitive()
        };
        let duration = |name: &str| match primitive(name)? {
            Literal::Duration { value } => Ok(*value),
            _ => Err("Expected duration".to_string()),
        };
        let value = match operation.builtin {
            Builtin::Ramp => {
                let Binding::Ramp { ramp } = &operation.bindings["stops"] else {
                    return Err("Ramp requires stops".into());
                };
                for _ in &ramp.stops {
                    self.budget.step()?;
                }
                let Literal::Scalar { value } = primitive("position")? else {
                    return Err("Ramp input must be scalar".into());
                };
                Value::Primitive(ramp.sample(*value)?)
            }
            Builtin::Expression => args.get("value").ok_or("Missing expression")?.clone(),
            Builtin::SceneTime => Value::Primitive(Literal::Duration {
                value: self.scene_time,
            }),
            Builtin::AssetTime => Value::Primitive(Literal::Duration { value: scope.time }),
            Builtin::Rectangle => {
                let distance = |name| match primitive(name)? {
                    Literal::Distance {
                        value,
                        unit: Unit::Canvas,
                    } => Ok(*value),
                    _ => Err("Expected canvas distance".to_string()),
                };
                let width = distance("width")?;
                let height = distance("height")?;
                if width <= 0.0 || height <= 0.0 {
                    return Err("Rectangle dimensions must be positive".into());
                }
                self.reserve(6)?;
                let geometry_id = self.identity(scope, id, 0, b"geometry")?;
                let contour_id = self.identity(scope, id, geometry_id, b"contour:0")?;
                let mut points = Vec::with_capacity(4);
                for (index, position) in [[0., 0.], [width, 0.], [width, height], [0., height]]
                    .into_iter()
                    .enumerate()
                {
                    points.push(Point {
                        id: self.identity(
                            scope,
                            id,
                            contour_id,
                            format!("point:{index}").as_bytes(),
                        )?,
                        position,
                    });
                }
                Value::Geometry(Arc::new(Geometry {
                    id: geometry_id,
                    space: Space::Screen,
                    unit: Unit::Canvas,
                    data: GeometryData::Contours(Arc::new(vec![Contour {
                        id: contour_id,
                        points,
                        closed: true,
                    }])),
                    appearance: style::Appearance::default(),
                }))
            }
            Builtin::InstanceScale | Builtin::InstanceColor | Builtin::InstanceRotationZ => self
                .animate_instances(
                    scope,
                    id,
                    &args,
                    operation.builtin,
                    &operation.bindings[match operation.builtin {
                        Builtin::InstanceScale => "factor",
                        Builtin::InstanceColor => "color",
                        _ => "angle",
                    }],
                )?,
            Builtin::RadialField => self.radial_field(scope, id, &args)?,
            Builtin::LinearField => self.linear_field(scope, id, &args)?,
            Builtin::InstanceRandom => self.instance_random(scope, id, &args)?,
            Builtin::Cloner => self.cloner(scope, id, &args)?,
            Builtin::Grid => self.grid(scope, id, &args)?,
            Builtin::BoxMesh => self.box_mesh(scope, id, &args)?,
            Builtin::SphereMesh => self.sphere_mesh(scope, id, &args)?,
            Builtin::TransformScreen | Builtin::TransformMesh | Builtin::TransformInstances => {
                self.transform_geometry(scope, id, &args, operation.builtin)?
            }
            Builtin::Tessellate => self.profile_mesh(scope, id, &args, false)?,
            Builtin::ExtrudeProfile => self.profile_mesh(scope, id, &args, true)?,
            Builtin::Extrude => self.extrude_text(scope, id, &args)?,
            Builtin::OutlineText => self.outline_text(scope, id, &args)?,
            Builtin::Text => self.text_geometry(scope, id, &args)?,
            Builtin::TextAnimatorWords => {
                self.animate_words(scope, id, &args, &operation.bindings["position"])?
            }
            Builtin::LitMaterial | Builtin::UnlitMaterial => {
                let Some(Value::Geometry(geometry)) = args.get("geometry") else {
                    return Err("Expected spatial geometry".into());
                };
                self.reserve(1)?;
                Value::Geometry(Arc::new(material::apply(
                    geometry,
                    operation.builtin,
                    &args,
                )?))
            }
            Builtin::Fill
            | Builtin::Stroke
            | Builtin::StrokeButtRound
            | Builtin::StrokeRoundMiter
            | Builtin::StrokeRoundRound => {
                let Some(Value::Geometry(geometry)) = args.get("geometry") else {
                    return Err("Expected geometry to style".into());
                };
                self.reserve(1)?;
                Value::Geometry(Arc::new(style::apply(geometry, operation.builtin, &args)?))
            }
            Builtin::TweenLinear | Builtin::TweenCubicOut => {
                let length = duration("duration")?;
                if length <= Time::ZERO {
                    return Err("Tween duration must be positive".into());
                }
                let time = duration("time")?;
                let start = duration("start")?;
                let end = start.checked_add(length).map_err(|e| e.to_string())?;
                let mut amount = if time <= start {
                    0.0
                } else if time >= end {
                    1.0
                } else {
                    let elapsed = time.checked_sub(start).map_err(|e| e.to_string())?;
                    (elapsed.numerator() as f64 * f64::from(length.denominator()))
                        / (length.numerator() as f64 * f64::from(elapsed.denominator()))
                };
                if operation.builtin == Builtin::TweenCubicOut {
                    amount = 1.0 - (1.0 - amount).powi(3);
                }
                Value::Primitive(primitive("from")?.interpolate(primitive("to")?, amount)?)
            }
            Builtin::Keyframes => {
                let Binding::Track { track } =
                    operation.bindings.get("track").ok_or("Missing keys")?
                else {
                    return Err("Expected track".into());
                };
                // Track validation/sampling is bounded by 4096 keys; charge that work.
                for _ in &track.keys {
                    self.budget.step()?;
                }
                Value::Primitive(track.sample(duration("time")?)?)
            }
            Builtin::CameraPlaneViewport | Builtin::CameraPlanePhysical => {
                let Some(Value::Geometry(geometry)) = args.get("geometry") else {
                    return Err("Expected planar geometry".into());
                };
                let (transform, opacity) = placement::camera_plane(
                    geometry,
                    &args,
                    operation.builtin == Builtin::CameraPlanePhysical,
                )?;
                self.reserve(1)?;
                Value::Placement(Arc::new(Placement {
                    id: self.identity(scope, id, geometry.id, b"placement")?,
                    geometry: geometry.clone(),
                    transform: placement::Transform::CameraPlane(transform),
                    opacity,
                }))
            }
            Builtin::World => {
                let Some(Value::Geometry(geometry)) = args.get("geometry") else {
                    return Err("Expected world geometry".into());
                };
                let (transform, opacity) = placement::world(geometry, &args)?;
                self.reserve(1)?;
                Value::Placement(Arc::new(Placement {
                    id: self.identity(scope, id, geometry.id, b"placement")?,
                    geometry: geometry.clone(),
                    transform: placement::Transform::World(transform),
                    opacity,
                }))
            }
            Builtin::Screen => {
                let Value::Geometry(geometry) = args.get("geometry").ok_or("Missing geometry")?
                else {
                    return Err("Expected geometry".into());
                };
                if geometry.space != Space::Screen || geometry.unit != Unit::Canvas {
                    return Err("Screen placement requires screen/canvas geometry".into());
                }
                let Literal::Vector {
                    value: position,
                    unit: Unit::Canvas,
                    space: Space::Screen,
                } = primitive("position")?
                else {
                    return Err("Expected screen position".into());
                };
                let Literal::Scalar { value: opacity } = primitive("opacity")? else {
                    return Err("Expected opacity".into());
                };
                if !(0.0..=1.0).contains(opacity) {
                    return Err("Opacity must be in [0,1]".into());
                }
                self.reserve(1)?;
                Value::Placement(Arc::new(Placement {
                    id: self.identity(scope, id, geometry.id, b"placement")?,
                    geometry: geometry.clone(),
                    transform: placement::Transform::Screen {
                        position: [position[0], position[1]],
                    },
                    opacity: *opacity,
                }))
            }
            Builtin::PerspectiveCamera | Builtin::OrthographicCamera => {
                self.reserve(1)?;
                Value::Camera(Arc::new(spatial::camera(
                    self.identity(scope, id, 0, b"camera")?,
                    &args,
                    operation.builtin == Builtin::OrthographicCamera,
                )?))
            }
            Builtin::DirectionalLight => {
                self.reserve(1)?;
                Value::Light(Arc::new(spatial::light(
                    self.identity(scope, id, 0, b"light")?,
                    &args,
                )?))
            }
            Builtin::Scene | Builtin::SpatialScene => {
                let Binding::List { values } = operation
                    .bindings
                    .get("children")
                    .ok_or("Missing scene contents")?
                else {
                    return Err("Expected scene list".into());
                };
                self.reserve(values.len())?;
                let mut entries = Vec::with_capacity(values.len());
                let mut cameras = vec![];
                let mut lights = vec![];
                let camera = if operation.builtin == Builtin::SpatialScene {
                    let Some(Value::Camera(camera)) = args.get("camera") else {
                        return Err("Expected active camera".into());
                    };
                    Some(camera.clone())
                } else {
                    None
                };
                for binding in values {
                    let part = self.binding(scope, id, binding)?;
                    let Binding::Link { reference } = binding else {
                        return Err("Scene occurrence requires a persistent connection".into());
                    };
                    match part {
                        Value::Placement(placement) => entries.push(SceneEntry {
                            id: self.identity(scope, id, placement.id, &reference.id.to_bytes())?,
                            placement,
                        }),
                        Value::Camera(camera) => cameras.push(spatial::CameraEntry {
                            id: self.identity(scope, id, camera.id, &reference.id.to_bytes())?,
                            camera,
                        }),
                        Value::Light(light) => lights.push(spatial::LightEntry {
                            id: self.identity(scope, id, light.id, &reference.id.to_bytes())?,
                            light,
                        }),
                        _ => return Err("Expected scene placement, camera or light".into()),
                    }
                }
                Value::Scene(Arc::new(Scene {
                    entries,
                    camera,
                    cameras,
                    lights,
                }))
            }
            Builtin::Input | Builtin::Output | Builtin::Asset => {
                return Err("Native declaration cannot execute as an operation".into());
            }
        };
        self.values.insert(key, value.clone());
        Ok(value)
    }
}
