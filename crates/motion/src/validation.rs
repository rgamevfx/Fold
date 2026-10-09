//! Semantic validation is separate from opaque, lossless document persistence.
use crate::{
    binding::Binding,
    model::{Construction, Definition, Instance, MAX_OBJECTS, Reference},
    registry,
    types::Type,
};
use fold_foundation::ObjectId;
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Native categories survive registry callbacks and transport boundaries.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Kind {
    Type,
    Cycle,
    MissingReference,
    Unsupported,
    Budget,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub kind: Kind,
    pub message: String,
    pub native_ids: Vec<ObjectId>,
}
impl From<&str> for Diagnostic {
    fn from(message: &str) -> Self {
        Self::from(message.to_string())
    }
}
impl From<String> for Diagnostic {
    fn from(message: String) -> Self {
        Self {
            kind: Kind::Type,
            message,
            native_ids: vec![],
        }
    }
}

impl Construction {
    /// Resolve every operation, reference and override, including disconnected work.
    /// An unresolved operation can be saved but cannot pass this execution gate.
    pub fn validate_semantics(&self) -> Result<(), String> {
        self.validate_semantics_detailed()
            .map_err(|error| error.message)
    }
    pub fn validate_semantics_detailed(&self) -> Result<(), Diagnostic> {
        let mut ids = self.structural_ids()?;
        for definition in &self.definitions {
            for node in &definition.nodes {
                registry::resolve(&node.operation, node.version, &node.mode).map_err(
                    |message| Diagnostic {
                        kind: Kind::Unsupported,
                        message,
                        native_ids: vec![node.id],
                    },
                )?;
                for parameter in node.parameters.values() {
                    binding_ids(&parameter.binding()?, &mut ids, 0)?;
                }
            }
            for instance in &definition.instances {
                for parameter in instance.overrides.values() {
                    binding_ids(&parameter.binding()?, &mut ids, 0)?;
                }
            }
        }
        for parameter in self.root.overrides.values() {
            binding_ids(&parameter.binding()?, &mut ids, 0)?;
        }
        let mut checker = Checker {
            construction: self,
            types: BTreeMap::new(),
            instances: BTreeSet::new(),
            active: Vec::new(),
            diagnostic: None,
        };
        let result = (|| {
            for definition in &self.definitions {
                for node in &definition.nodes {
                    checker.reference(definition, Reference::Node { node: node.id })?;
                }
                for instance in &definition.instances {
                    checker.instance(Some(definition), instance)?;
                }
                for output in &definition.outputs {
                    let actual = checker.reference(definition, output.value.target)?;
                    if actual != output.ty {
                        return Err(format!(
                            "Output {} declares {:?}, resolves to {actual:?}",
                            output.name, output.ty
                        ));
                    }
                }
            }
            checker.instance(None, &self.root)
        })();
        result.map_err(|message| {
            checker
                .diagnostic
                .unwrap_or_else(|| Diagnostic::from(message))
        })
    }
}
fn binding_ids(
    binding: &Binding,
    ids: &mut BTreeSet<ObjectId>,
    depth: usize,
) -> Result<(), String> {
    if depth > 64 {
        return Err("Binding nesting exceeds 64".into());
    }
    let edges = match binding {
        Binding::Link { reference } => vec![reference],
        Binding::Expression { expression } => expression.edges()?,
        _ => vec![],
    };
    for edge in edges {
        edge.validate()?;
        if !ids.insert(edge.id) {
            return Err(format!("Duplicate native identity {:?}", edge.id));
        }
        if ids.len() > MAX_OBJECTS {
            return Err("Native object budget exceeded".into());
        }
    }
    match binding {
        Binding::Ramp { ramp } => {
            ramp.validate()?;
            for id in std::iter::once(ramp.id).chain(ramp.stops.iter().map(|stop| stop.id)) {
                if !ids.insert(id) {
                    return Err(format!("Duplicate native identity {id:?}"));
                }
                if ids.len() > MAX_OBJECTS {
                    return Err("Native object budget exceeded".into());
                }
            }
        }
        Binding::Track { track } => {
            track.validate()?;
            for id in std::iter::once(track.id).chain(track.keys.iter().map(|key| key.id)) {
                if !ids.insert(id) {
                    return Err(format!("Duplicate native identity {id:?}"));
                }
                if ids.len() > MAX_OBJECTS {
                    return Err("Native object budget exceeded".into());
                }
            }
        }
        Binding::List { values } => {
            if values.len() > 4096 {
                return Err("Binding list exceeds 4096 values".into());
            }
            for value in values {
                binding_ids(value, ids, depth + 1)?;
            }
        }
        _ => {}
    }
    Ok(())
}
struct Checker<'a> {
    construction: &'a Construction,
    types: BTreeMap<ObjectId, Type>,
    instances: BTreeSet<ObjectId>,
    active: Vec<ObjectId>,
    diagnostic: Option<Diagnostic>,
}
impl Checker<'_> {
    // Registry inference accepts string failures. Retain the originating native
    // diagnostic separately while that callback unwinds, without text matching.
    fn fail(&mut self, kind: Kind, message: String, native_ids: Vec<ObjectId>) -> String {
        if self.diagnostic.is_none() {
            self.diagnostic = Some(Diagnostic {
                kind,
                message: message.clone(),
                native_ids,
            });
        }
        message
    }
    fn enter(&mut self, id: ObjectId) -> Result<(), String> {
        if let Some(start) = self.active.iter().position(|current| *current == id) {
            let mut cycle = self.active[start..].to_vec();
            cycle.push(id);
            return Err(self.fail(
                Kind::Cycle,
                format!("Cyclic native dependency: {cycle:?}"),
                cycle,
            ));
        }
        if self.active.len() >= 256 {
            return Err(self.fail(
                Kind::Budget,
                "Native dependency depth exceeds 256".into(),
                vec![id],
            ));
        }
        self.active.push(id);
        Ok(())
    }
    fn reference(&mut self, scope: &Definition, reference: Reference) -> Result<Type, String> {
        match reference {
            Reference::Input { input } => scope
                .inputs
                .iter()
                .find(|i| i.id == input)
                .map(|i| i.ty.clone())
                .ok_or_else(|| {
                    self.fail(
                        Kind::MissingReference,
                        format!("Input {input:?} is outside definition {}", scope.name),
                        vec![input, scope.id],
                    )
                }),
            Reference::Node { node } => {
                // Scope membership is checked even when the type was previously cached.
                let node = scope.nodes.iter().find(|n| n.id == node).ok_or_else(|| {
                    self.fail(
                        Kind::MissingReference,
                        format!("Node {node:?} is outside definition {}", scope.name),
                        vec![node, scope.id],
                    )
                })?;
                if let Some(ty) = self.types.get(&node.id) {
                    return Ok(ty.clone());
                }
                self.enter(node.id)?;
                let result = registry::resolve(&node.operation, node.version, &node.mode)?
                    .infer(node, &mut |reference| self.reference(scope, reference));
                self.active.pop();
                let ty = result.map_err(|message| self.fail(Kind::Type, message, vec![node.id]))?;
                self.types.insert(node.id, ty.clone());
                Ok(ty)
            }
            Reference::Instance { instance, output } => {
                let instance = scope
                    .instances
                    .iter()
                    .find(|i| i.id == instance)
                    .ok_or_else(|| {
                        self.fail(
                            Kind::MissingReference,
                            format!("Instance {instance:?} is outside definition {}", scope.name),
                            vec![instance, scope.id],
                        )
                    })?;
                self.instance(Some(scope), instance)?;
                let definition = self.construction.definition(instance.definition)?;
                definition
                    .outputs
                    .iter()
                    .find(|o| o.id == output)
                    .map(|o| o.ty.clone())
                    .ok_or_else(|| {
                        self.fail(
                            Kind::MissingReference,
                            format!(
                                "Output {output:?} does not belong to instance {}",
                                instance.name
                            ),
                            vec![output, instance.id],
                        )
                    })
            }
        }
    }
    fn instance(&mut self, parent: Option<&Definition>, instance: &Instance) -> Result<(), String> {
        if self.instances.contains(&instance.id) {
            return Ok(());
        }
        self.enter(instance.id)?;
        let definition = self.construction.definition(instance.definition)?;
        for (input, parameter) in &instance.overrides {
            let input = definition
                .inputs
                .iter()
                .find(|i| i.id == *input)
                .ok_or("Unknown override input")?;
            let actual = match parameter.binding()? {
                Binding::Track { track } => {
                    track.validate()?;
                    track.ty
                }
                binding => registry::binding_type(&binding, &mut |reference| {
                    let parent =
                        parent.ok_or("Root overrides cannot reference a parent definition")?;
                    self.reference(parent, reference)
                })?,
            };
            if actual != input.ty {
                return Err(format!(
                    "Override {} declares {:?}, resolves to {actual:?}",
                    input.name, input.ty
                ));
            }
        }
        self.active.pop();
        self.instances.insert(instance.id);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        model::{Canvas, Input, Output, Parameter, Presentation},
        types::{Clock, Literal},
    };
    use fold_foundation::Time;
    use fold_project::Metadata;
    fn construction() -> Construction {
        let id = ObjectId::new();
        Construction {
            seed: 1,
            canvas: Canvas {
                width: 320,
                height: 180,
                rate: [24, 1],
                start: Time::ZERO,
                end: Time::new(1, 1).unwrap(),
                extensions: Metadata::new(),
            },
            definitions: vec![Definition {
                id,
                name: "main".into(),
                inputs: vec![],
                nodes: vec![],
                instances: vec![],
                outputs: vec![],
                extensions: Metadata::new(),
            }],
            root: Instance {
                id: ObjectId::new(),
                name: "root".into(),
                definition: id,
                clock: Clock::default(),
                overrides: BTreeMap::new(),
                extensions: Metadata::new(),
            },
            presentation: Presentation::default(),
            extensions: Metadata::new(),
        }
    }
    #[test]
    fn outputs_require_correct_scope_and_exact_declared_type() {
        let mut c = construction();
        let node = registry::resolve("fold.motion.time", 1, "asset")
            .unwrap()
            .node("clock", BTreeMap::new())
            .unwrap();
        let id = node.id;
        c.definitions[0].nodes.push(node);
        c.definitions[0].outputs.push(Output {
            id: ObjectId::new(),
            name: "time".into(),
            ty: Type::Signal {
                value: Box::new(Type::Duration),
            },
            value: Reference::Node { node: id }.into(),
            extensions: Metadata::new(),
        });
        c.validate_semantics().unwrap();
        c.definitions[0].outputs[0].ty = Type::Scalar;
        assert!(c.validate_semantics().unwrap_err().contains("declares"));
        c.definitions[0].outputs[0].value.target = Reference::Node {
            node: ObjectId::new(),
        };
        assert!(
            c.validate_semantics()
                .unwrap_err()
                .contains("outside definition")
        );
    }
    #[test]
    fn node_cycles_and_root_reference_overrides_fail() {
        let mut c = construction();
        let mut node = registry::resolve("fold.motion.expression", 1, "value")
            .unwrap()
            .node(
                "recursive",
                BTreeMap::from([(
                    "value".into(),
                    Binding::Literal {
                        value: Literal::Scalar { value: 1.0 },
                    },
                )]),
            )
            .unwrap();
        node.parameters.get_mut("value").unwrap().value = serde_json::to_value(Binding::Link {
            reference: Reference::Node { node: node.id }.into(),
        })
        .unwrap();
        let cyclic_id = node.id;
        c.definitions[0].nodes.push(node);
        let error = c.validate_semantics().unwrap_err();
        assert!(error.contains("Cyclic") && error.contains(&format!("{cyclic_id:?}")));
        c.definitions[0].nodes.clear();
        let input = ObjectId::new();
        c.definitions[0].inputs.push(Input {
            id: input,
            name: "size".into(),
            ty: Type::Scalar,
            default: Literal::Scalar { value: 1.0 },
            protected: false,
            extensions: Metadata::new(),
        });
        c.root.overrides.insert(
            input,
            Parameter::new(Binding::Link {
                reference: Reference::Input { input }.into(),
            })
            .unwrap(),
        );
        assert!(
            c.validate_semantics()
                .unwrap_err()
                .contains("Root overrides")
        );
        c.root.overrides.insert(
            input,
            Parameter::new(Binding::Literal {
                value: Literal::Bool { value: false },
            })
            .unwrap(),
        );
        assert!(
            c.validate_semantics()
                .unwrap_err()
                .contains("Override size")
        );
    }
    #[test]
    fn asset_override_dependency_cycles_are_rejected() {
        let mut c = construction();
        let input = ObjectId::new();
        let output = ObjectId::new();
        let child = Definition {
            id: ObjectId::new(),
            name: "child".into(),
            inputs: vec![Input {
                id: input,
                name: "value".into(),
                ty: Type::Scalar,
                default: Literal::Scalar { value: 1.0 },
                protected: false,
                extensions: Metadata::new(),
            }],
            nodes: vec![],
            instances: vec![],
            outputs: vec![Output {
                id: output,
                name: "result".into(),
                ty: Type::Scalar,
                value: Reference::Input { input }.into(),
                extensions: Metadata::new(),
            }],
            extensions: Metadata::new(),
        };
        let mut instance = Instance {
            id: ObjectId::new(),
            name: "child".into(),
            definition: child.id,
            clock: Clock::default(),
            overrides: BTreeMap::new(),
            extensions: Metadata::new(),
        };
        let node = registry::resolve("fold.motion.expression", 1, "value")
            .unwrap()
            .node(
                "loop",
                BTreeMap::from([(
                    "value".into(),
                    Binding::Link {
                        reference: Reference::Instance {
                            instance: instance.id,
                            output,
                        }
                        .into(),
                    },
                )]),
            )
            .unwrap();
        instance.overrides.insert(
            input,
            Parameter::new(Binding::Link {
                reference: Reference::Node { node: node.id }.into(),
            })
            .unwrap(),
        );
        c.definitions[0].nodes.push(node);
        c.definitions[0].instances.push(instance);
        c.definitions.push(child);
        assert!(c.validate_semantics().unwrap_err().contains("Cyclic"));
        c.definitions[0].instances[0].overrides.clear();
        c.validate_semantics().unwrap();
    }
}
