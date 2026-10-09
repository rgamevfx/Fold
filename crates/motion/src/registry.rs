//! Single contract authority for native operations. A declaration is not execution qualification.
mod animator;
mod distribution;
mod spatial;
mod style;
mod text;
use crate::{
    binding::Binding,
    expression::{ClockScope, Expr},
    model::{Node, Parameter, Reference},
    types::{Domain, Literal, Space, Type, Unit},
};
use fold_foundation::{ObjectId, Time};
use fold_project::Metadata;
use serde::Serialize;
use serde_json::{Value, json};
use std::{collections::BTreeMap, sync::OnceLock};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Palette {
    Artist,
    Foundation,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Cost {
    Uniform,
    Element,
    Geometry,
    Collection,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    Operation,
    Input,
    Output,
    Asset,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Pattern {
    Exact { ty: Type },
    Primitive,
    Interpolatable,
    SameAs { parameter: String },
    ScenePart,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Form {
    Value,
    List,
    Track,
    Ramp,
}
#[derive(Clone, Debug, Serialize)]
pub struct Port {
    pub title: String,
    pub form: Form,
    pub pattern: Pattern,
    pub default: Option<Binding>,
}
#[derive(Clone, Debug, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ResultType {
    Exact { ty: Type },
    Parameter { parameter: String, signal: bool },
    DefinitionOutput,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Builtin {
    Rectangle,
    BoxMesh,
    Grid,
    Cloner,
    InstanceScale,
    InstanceColor,
    InstanceRotationZ,
    RadialField,
    LinearField,
    InstanceRandom,
    Ramp,
    Expression,
    SceneTime,
    AssetTime,
    TweenLinear,
    TweenCubicOut,
    Keyframes,
    Screen,
    World,
    CameraPlaneViewport,
    CameraPlanePhysical,
    Scene,
    SpatialScene,
    PerspectiveCamera,
    OrthographicCamera,
    DirectionalLight,
    Fill,
    Stroke,
    Text,
    TextAnimatorWords,
    OutlineText,
    Extrude,
    LitMaterial,
    UnlitMaterial,
    Input,
    Output,
    Asset,
}
#[derive(Clone, Debug, Serialize)]
pub struct Operation {
    pub key: String,
    pub version: u32,
    pub mode: String,
    pub title: String,
    pub palette: Palette,
    pub role: Role,
    pub parameters: BTreeMap<String, Port>,
    pub output: ResultType,
    pub domains: Vec<Domain>,
    pub cost: Cost,
    /// Qualified CPU operation execution; this is independent of rendering support.
    pub cpu_reference: bool,
    pub gpu: bool,
    #[serde(skip)]
    pub builtin: Builtin,
}
fn exact(ty: Type) -> Pattern {
    Pattern::Exact { ty }
}
fn value(title: &str, pattern: Pattern, default: Option<Literal>) -> Port {
    Port {
        title: title.into(),
        form: Form::Value,
        pattern,
        default: default.map(|value| Binding::Literal { value }),
    }
}
fn time() -> Port {
    Port {
        title: "Time".into(),
        form: Form::Value,
        pattern: exact(Type::Duration),
        default: Some(Binding::Expression {
            expression: Expr::Time {
                scope: ClockScope::Asset,
            },
        }),
    }
}
fn operation(
    (key, mode, title, builtin): (&str, &str, &str, Builtin),
    palette: Palette,
    parameters: Vec<(&str, Port)>,
    output: ResultType,
    cost: Cost,
    domains: Vec<Domain>,
) -> Operation {
    Operation {
        key: format!("fold.motion.{key}"),
        version: 1,
        mode: mode.into(),
        title: title.into(),
        palette,
        role: Role::Operation,
        parameters: parameters.into_iter().map(|(k, v)| (k.into(), v)).collect(),
        output,
        domains,
        cost,
        cpu_reference: true,
        gpu: false,
        builtin,
    }
}
pub fn operations() -> &'static [Operation] {
    static REGISTRY: OnceLock<Vec<Operation>> = OnceLock::new();
    REGISTRY.get_or_init(|| {
        let length = Type::Distance { unit: Unit::Canvas };
        let scalar = |title, default| {
            value(
                title,
                exact(Type::Scalar),
                Some(Literal::Scalar { value: default }),
            )
        };
        let mut entries = vec![
            operation(
                ("shape", "rectangle", "Rectangle", Builtin::Rectangle),
                Palette::Artist,
                vec![
                    (
                        "width",
                        value(
                            "Width",
                            exact(length.clone()),
                            Some(Literal::Distance {
                                value: 100.,
                                unit: Unit::Canvas,
                            }),
                        ),
                    ),
                    (
                        "height",
                        value(
                            "Height",
                            exact(length),
                            Some(Literal::Distance {
                                value: 100.,
                                unit: Unit::Canvas,
                            }),
                        ),
                    ),
                ],
                ResultType::Exact { ty: Type::Geometry },
                Cost::Geometry,
                vec![Domain::Collection, Domain::Curve],
            ),
            operation(
                ("expression", "value", "Expression", Builtin::Expression),
                Palette::Foundation,
                vec![("value", value("Value", Pattern::Primitive, None))],
                ResultType::Parameter {
                    parameter: "value".into(),
                    signal: true,
                },
                Cost::Uniform,
                vec![],
            ),
            operation(
                ("time", "scene", "Scene Time", Builtin::SceneTime),
                Palette::Foundation,
                vec![],
                ResultType::Exact {
                    ty: Type::Signal {
                        value: Box::new(Type::Duration),
                    },
                },
                Cost::Uniform,
                vec![],
            ),
            operation(
                ("time", "asset", "Asset Time", Builtin::AssetTime),
                Palette::Foundation,
                vec![],
                ResultType::Exact {
                    ty: Type::Signal {
                        value: Box::new(Type::Duration),
                    },
                },
                Cost::Uniform,
                vec![],
            ),
            operation(
                ("keyframes", "value", "Keyframes", Builtin::Keyframes),
                Palette::Foundation,
                vec![
                    (
                        "track",
                        Port {
                            title: "Keys".into(),
                            form: Form::Track,
                            pattern: Pattern::Interpolatable,
                            default: None,
                        },
                    ),
                    ("time", time()),
                ],
                ResultType::Parameter {
                    parameter: "track".into(),
                    signal: true,
                },
                Cost::Uniform,
                vec![],
            ),
            operation(
                ("place", "screen", "Screen Placement", Builtin::Screen),
                Palette::Artist,
                vec![
                    ("geometry", value("Geometry", exact(Type::Geometry), None)),
                    (
                        "position",
                        value(
                            "Position",
                            exact(Type::Vector {
                                dimensions: 2,
                                unit: Unit::Canvas,
                                space: Space::Screen,
                            }),
                            Some(Literal::Vector {
                                value: vec![0., 0.],
                                unit: Unit::Canvas,
                                space: Space::Screen,
                            }),
                        ),
                    ),
                    ("opacity", scalar("Opacity", 1.)),
                ],
                ResultType::Exact {
                    ty: Type::Placement,
                },
                Cost::Collection,
                vec![Domain::Collection],
            ),
            operation(
                ("scene", "ordered", "Scene", Builtin::Scene),
                Palette::Artist,
                vec![(
                    "children",
                    Port {
                        title: "Ordered contents".into(),
                        form: Form::List,
                        pattern: Pattern::ScenePart,
                        default: Some(Binding::List { values: vec![] }),
                    },
                )],
                ResultType::Exact { ty: Type::Scene },
                Cost::Collection,
                vec![Domain::Collection],
            ),
        ];
        entries.extend(animator::operations());
        entries.extend(distribution::operations());
        entries.extend(spatial::operations());
        entries.extend(style::operations());
        entries.extend(text::operations());
        for (mode, title, builtin) in [
            ("linear", "Linear Tween", Builtin::TweenLinear),
            ("cubic_out", "Cubic-out Tween", Builtin::TweenCubicOut),
        ] {
            entries.push(operation(
                ("tween", mode, title, builtin),
                Palette::Foundation,
                vec![
                    ("from", value("From", Pattern::Interpolatable, None)),
                    (
                        "to",
                        value(
                            "To",
                            Pattern::SameAs {
                                parameter: "from".into(),
                            },
                            None,
                        ),
                    ),
                    (
                        "duration",
                        value(
                            "Duration",
                            exact(Type::Duration),
                            Some(Literal::Duration {
                                value: Time::new(1, 1).expect("constant duration"),
                            }),
                        ),
                    ),
                    (
                        "start",
                        value(
                            "Start",
                            exact(Type::Duration),
                            Some(Literal::Duration { value: Time::ZERO }),
                        ),
                    ),
                    ("time", time()),
                ],
                ResultType::Parameter {
                    parameter: "from".into(),
                    signal: true,
                },
                Cost::Uniform,
                vec![],
            ));
        }
        for (key, title, builtin, role) in [
            ("input", "Input", Builtin::Input, Role::Input),
            ("output", "Output", Builtin::Output, Role::Output),
            ("asset", "Asset Instance", Builtin::Asset, Role::Asset),
        ] {
            let mut spec = operation(
                (key, "native", title, builtin),
                Palette::Foundation,
                vec![],
                ResultType::DefinitionOutput,
                Cost::Uniform,
                vec![],
            );
            spec.role = role;
            spec.cpu_reference = false;
            if role == Role::Asset {
                spec.palette = Palette::Artist;
            }
            entries.push(spec);
        }
        entries
    })
}
pub fn resolve(key: &str, version: u32, mode: &str) -> Result<&'static Operation, String> {
    operations()
        .iter()
        .find(|op| op.key == key && op.version == version && op.mode == mode)
        .ok_or_else(|| format!("Unresolved operation {key} version {version} mode {mode}"))
}
impl Operation {
    /// Authored defaults acquire persistent parameter IDs exactly once, at construction.
    pub fn node(
        &self,
        name: impl Into<String>,
        mut bindings: BTreeMap<String, Binding>,
    ) -> Result<Node, String> {
        if self.role != Role::Operation {
            return Err("Native declarations are not operation nodes".into());
        }
        if bindings
            .keys()
            .any(|key| !self.parameters.contains_key(key))
        {
            return Err("Unknown parameter".into());
        }
        let mut parameters = BTreeMap::new();
        for (key, port) in &self.parameters {
            let binding = bindings
                .remove(key)
                .or_else(|| port.default.clone())
                .ok_or_else(|| format!("Missing parameter {key}"))?;
            parameters.insert(key.clone(), Parameter::new(binding)?);
        }
        Ok(Node {
            id: ObjectId::new(),
            name: name.into(),
            operation: self.key.clone(),
            version: self.version,
            mode: self.mode.clone(),
            parameters,
            extensions: Metadata::default(),
        })
    }
    pub fn infer(
        &self,
        node: &Node,
        resolve: &mut impl FnMut(Reference) -> Result<Type, String>,
    ) -> Result<Type, String> {
        if self.role != Role::Operation
            || node.operation != self.key
            || node.version != self.version
            || node.mode != self.mode
        {
            return Err("Operation contract mismatch".into());
        }
        if node.parameters.len() != self.parameters.len()
            || node
                .parameters
                .keys()
                .any(|key| !self.parameters.contains_key(key))
        {
            return Err(
                "Operation parameters must match its contract, including authored defaults".into(),
            );
        }
        let mut types = BTreeMap::new();
        for (name, port) in &self.parameters {
            let binding = node
                .parameters
                .get(name)
                .ok_or("Missing parameter")?
                .binding()?;
            let values = match (&port.form, &binding) {
                (
                    Form::Value,
                    Binding::Literal { .. } | Binding::Link { .. } | Binding::Expression { .. },
                ) => vec![binding_type(&binding, resolve)?],
                (Form::List, Binding::List { values }) if values.len() <= 4096 => values
                    .iter()
                    .map(|v| binding_type(v, resolve))
                    .collect::<Result<Vec<_>, _>>()?,
                (Form::Ramp, Binding::Ramp { ramp }) => {
                    ramp.validate()?;
                    vec![ramp.ty.clone()]
                }
                (Form::Track, Binding::Track { track }) => {
                    track.validate()?;
                    vec![track.ty.clone()]
                }
                _ => return Err(format!("Parameter {name} has the wrong binding form")),
            };
            types.insert(name.clone(), values);
        }
        for (name, port) in &self.parameters {
            for ty in &types[name] {
                let matches = match &port.pattern {
                    Pattern::Exact { ty: expected } => ty == expected,
                    Pattern::Primitive => ty.is_primitive(),
                    Pattern::Interpolatable => ty.is_interpolatable(),
                    Pattern::SameAs { parameter } => types
                        .get(parameter)
                        .is_some_and(|v| v.len() == 1 && &v[0] == ty),
                    Pattern::ScenePart => {
                        matches!(ty, Type::Placement | Type::Camera | Type::Light)
                    }
                };
                if !matches {
                    return Err(format!(
                        "Parameter {name}: {ty:?} violates {:?}",
                        port.pattern
                    ));
                }
            }
        }
        let output = match &self.output {
            ResultType::Exact { ty } => ty.clone(),
            ResultType::Parameter { parameter, signal } => {
                let values = types.get(parameter).ok_or("Invalid result contract")?;
                let [value] = values.as_slice() else {
                    return Err("Result requires one value".into());
                };
                if *signal {
                    Type::Signal {
                        value: Box::new(value.clone()),
                    }
                } else {
                    value.clone()
                }
            }
            ResultType::DefinitionOutput => {
                return Err("Declaration needs its definition context".into());
            }
        };
        output.validate()?;
        Ok(output)
    }
    /// Grammar signature generated from this operation contract. Angle-bracket
    /// arguments are placeholders; literal/link/expression type rules remain in ports.
    pub fn declaration(&self) -> String {
        match self.role {
            Role::Input => {
                return "input <quoted-name> @<metadata> : <type-json> = <literal>;".into();
            }
            Role::Output => {
                return "output <quoted-name> @<metadata> : <type-json> = <reference>;".into();
            }
            Role::Asset => return "asset <quoted-name> @<metadata> { <declarations> }".into(),
            Role::Operation => {}
        }
        let arguments = self
            .parameters
            .iter()
            .map(|(name, port)| {
                let form = match port.form {
                    Form::Value => "<value(...) | link(...) | expr(...)>",
                    Form::List => "list[<bindings>]",
                    Form::Track => "track(<native-track-json>)",
                    Form::Ramp => "ramp(<native-ramp-json>)",
                };
                format!(
                    "{} @<metadata> = {form};",
                    serde_json::to_string(name).expect("registry parameter name")
                )
            })
            .collect::<Vec<_>>()
            .join(" ");
        format!(
            "let <quoted-name> @<metadata> = {}.{} @{} ({arguments});",
            serde_json::to_string(&self.key).expect("registry operation key"),
            serde_json::to_string(&self.mode).expect("registry mode"),
            self.version
        )
    }
}
pub(crate) fn binding_type(
    binding: &Binding,
    resolve: &mut impl FnMut(Reference) -> Result<Type, String>,
) -> Result<Type, String> {
    match binding {
        Binding::Literal { value } => {
            value.validate()?;
            Ok(value.ty())
        }
        Binding::Link { reference } => Ok(resolve(reference.target)?.sampled().clone()),
        Binding::Expression { expression } => expression.infer(resolve),
        _ => Err("Expected a value binding".into()),
    }
}
/// One schema root for all supported node variants. Semantic link types are checked
/// against the same contracts by `infer`; schema validation cannot resolve a document.
pub fn node_schema() -> Value {
    let mut binding =
        serde_json::to_value(schemars::schema_for!(Binding)).expect("serializable binding schema");
    let mut definitions = binding
        .as_object_mut()
        .unwrap()
        .remove("$defs")
        .unwrap_or(json!({}));
    binding.as_object_mut().unwrap().remove("$schema");
    definitions
        .as_object_mut()
        .unwrap()
        .insert("MotionBinding".into(), binding);
    let uuid = json!({"type":"string","format":"uuid"});
    let variants:Vec<_>=operations().iter().filter(|op|op.role==Role::Operation).map(|op| {
        let properties:serde_json::Map<String,Value>=op.parameters.iter().map(|(name,port)| {
            let forms: &[&str] = match port.form {
                Form::Value => &["literal", "link", "expression"],
                Form::List => &["list"],
                Form::Track => &["track"],
                Form::Ramp => &["ramp"],
            };
            let mut value = json!({"allOf":[
                {"$ref":"#/$defs/MotionBinding"},
                {"properties":{"kind":{"enum":forms}}}
            ]});
            if let Some(default) = &port.default { value["default"] = serde_json::to_value(default).expect("serializable default"); }
            (name.clone(),json!({"type":"object","required":["id","value"],"properties":{"id":uuid,"value":value,"protected":{"type":"boolean"}},"x-fold-contract":port}))
        }).collect();
        json!({"type":"object","required":["id","name","operation","version","mode","parameters"],"properties":{
            "id":uuid,"name":{"type":"string","minLength":1,"maxLength":256},"operation":{"const":op.key},"version":{"const":op.version},"mode":{"const":op.mode},
            "parameters":{"type":"object","properties":properties,"required":op.parameters.keys().collect::<Vec<_>>(),"additionalProperties":false}
        }})
    }).collect();
    json!({"$schema":"https://json-schema.org/draft/2020-12/schema","$defs":definitions,"oneOf":variants})
}
/// The same records supply editor labels/defaults, discoverable references and declarations.
pub fn catalog() -> Value {
    json!({"operations":operations(),"language":operations().iter().map(Operation::declaration).collect::<Vec<_>>(),"node_schema":node_schema()})
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn registry_keys_modes_defaults_and_schema_references_are_consistent() {
        let mut keys = std::collections::BTreeSet::new();
        for op in operations() {
            assert!(keys.insert((&op.key, op.version, &op.mode)));
            assert_eq!(op.cpu_reference, op.role == Role::Operation);
            assert!(!op.gpu, "GPU execution is not yet qualified");
            for port in op.parameters.values() {
                if let Some(default) = &port.default {
                    match default {
                        Binding::List { values } => assert!(values.is_empty()),
                        _ => {
                            let ty = binding_type(default, &mut |_| {
                                Err("default references another object".into())
                            })
                            .unwrap();
                            if let Pattern::Exact { ty: expected } = &port.pattern {
                                assert_eq!(&ty, expected);
                            }
                        }
                    }
                }
            }
        }
        let schema = node_schema();
        let mut stack = vec![&schema];
        while let Some(value) = stack.pop() {
            if let Some(reference) = value.get("$ref").and_then(Value::as_str) {
                assert!(
                    schema
                        .pointer(reference.strip_prefix('#').unwrap())
                        .is_some(),
                    "{reference}"
                );
            }
            match value {
                Value::Object(o) => stack.extend(o.values()),
                Value::Array(a) => stack.extend(a),
                _ => {}
            }
        }
    }
    #[test]
    fn defaults_have_stable_identity_and_mismatched_units_are_rejected() {
        let op = resolve("fold.motion.shape", 1, "rectangle").unwrap();
        let mut node = op.node("profile", BTreeMap::new()).unwrap();
        assert_eq!(
            op.infer(&node, &mut |_| unreachable!()).unwrap(),
            Type::Geometry
        );
        let saved = serde_json::to_value(&node).unwrap();
        assert_eq!(serde_json::from_value::<Node>(saved).unwrap(), node);
        node.parameters.get_mut("width").unwrap().value = serde_json::to_value(Binding::Literal {
            value: Literal::Distance {
                value: 1.,
                unit: Unit::Meters,
            },
        })
        .unwrap();
        assert!(op.infer(&node, &mut |_| unreachable!()).is_err());
        assert!(resolve("fold.motion.shape", 2, "rectangle").is_err());
    }
}
