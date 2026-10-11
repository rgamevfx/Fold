//! Compositor component bindings use the shared expression IR and stable native
//! parameter identities. Display paths are resolved only when authoring a binding.
use crate::{PreparedComposite, PreparedNode, animation::path};
use fold_foundation::{ObjectId, Time};
use fold_graph::{
    binding::Binding,
    expression::Expr,
    model::Reference,
    types::{Literal, Type},
};
use std::collections::{BTreeMap, BTreeSet};

impl PreparedNode<'_> {
    fn property_owner(&self, property: &str) -> Result<ObjectId, String> {
        if property == "mix" {
            Ok(self.effect.id)
        } else {
            self.operation
                .parameters
                .get(self.config().parameter_for_property(property))
                .map(|p| p.id)
                .ok_or_else(|| "Missing parameter owner".into())
        }
    }
    pub fn property_reference(
        &self,
        property: &str,
        component: usize,
    ) -> Result<Reference, String> {
        let mut offsets = BTreeMap::new();
        for p in self.animated_properties() {
            let id = self.property_owner(&p.id)?;
            let offset = offsets.entry(id).or_insert(0usize);
            if p.id == property {
                if component >= p.values.len() {
                    return Err("Missing parameter component".into());
                }
                return Ok(Reference::Parameter {
                    parameter: id,
                    component: Some(
                        u8::try_from(*offset + component)
                            .map_err(|_| "Too many parameter components")?,
                    ),
                });
            }
            *offset += p.values.len();
        }
        Err("Missing parameter property".into())
    }
}
impl PreparedComposite<'_> {
    pub(crate) fn index_references(&mut self) -> Result<(), String> {
        let mut references = BTreeMap::new();
        for node in self.nodes() {
            let mut offsets = BTreeMap::new();
            for property in node.animated_properties() {
                let parameter = node.property_owner(&property.id)?;
                let offset = offsets.entry(parameter).or_insert(0usize);
                for (i, value) in property.values.iter().enumerate() {
                    let reference = Reference::Parameter {
                        parameter,
                        component: Some(
                            u8::try_from(*offset + i)
                                .map_err(|_| "Too many parameter components")?,
                        ),
                    };
                    if references
                        .insert(reference, (node.id, path(&property.id, i), *value))
                        .is_some()
                    {
                        return Err("Duplicate parameter reference address".into());
                    }
                }
                *offset += property.values.len();
            }
        }
        self.references = references;
        Ok(())
    }
    pub fn reference_owner(&self, reference: Reference) -> Result<(ObjectId, String, f64), String> {
        self.references
            .get(&reference)
            .cloned()
            .ok_or_else(|| "Parameter reference target no longer exists".into())
    }
    pub fn resolve_channel(&self, owner: ObjectId, name: &str) -> Result<Reference, String> {
        let node = self.node(owner)?;
        let name = name.strip_prefix("channel:").unwrap_or(name);
        let parts = fold_graph::channel::resolve_path(&[node.name.clone()], name)?;
        if !(2..=3).contains(&parts.len()) {
            return Err("Expected node/property/component".into());
        }
        let mut matches = self.nodes().iter().filter(|n| n.name == parts[0]);
        let source = matches.next().ok_or("Missing parameter owner")?;
        if matches.next().is_some() {
            return Err("Ambiguous node name; rename the source node".into());
        }
        let property = source
            .animated_properties()
            .into_iter()
            .find(|p| p.id == parts[1])
            .ok_or("Missing parameter")?;
        let component = if let Some(name) = parts.get(2) {
            property
                .channels
                .iter()
                .position(|c| c.eq_ignore_ascii_case(name))
                .or_else(|| name.parse::<usize>().ok())
                .ok_or("Missing component")?
        } else if property.values.len() == 1 {
            0
        } else {
            return Err("Choose a parameter component".into());
        };
        source.property_reference(&property.id, component)
    }
    pub fn channel_name(&self, owner: ObjectId, reference: Reference) -> Result<String, String> {
        let (id, channel, _) = self.reference_owner(reference)?;
        let (property, component) = channel.rsplit_once('.').ok_or("Missing component")?;
        let node = self.node(id)?;
        let p = node
            .animated_properties()
            .into_iter()
            .find(|p| p.id == property)
            .ok_or("Missing property")?;
        let index: usize = component.parse().map_err(|_| "Invalid component")?;
        let target = vec![
            node.name.clone(),
            property.into(),
            p.channels[index].to_lowercase(),
        ];
        Ok(fold_graph::channel::relative_path(
            &[self.node(owner)?.name.clone()],
            &target,
        ))
    }
    pub fn parse_expression(&self, owner: ObjectId, source: &str) -> Result<Expr, String> {
        let expr = fold_graph::expression_source::parse(source, &mut |name| {
            self.resolve_channel(owner, name)
        })
        .map_err(|e| e.message)?
        .expression;
        if expr.infer(&mut |r| {
            self.reference_owner(r)?;
            Ok(Type::Scalar)
        })? != Type::Scalar
        {
            return Err("A compositor component expression must produce a scalar".into());
        }
        Ok(expr)
    }
    pub(crate) fn validate_references(&self) -> Result<(), String> {
        let mut done = BTreeSet::new();
        let mut active = BTreeSet::new();
        for node in self.nodes() {
            for path in node.channels().paths() {
                self.visit_binding(node.id, path, &mut active, &mut done)?;
            }
        }
        Ok(())
    }
    fn visit_binding(
        &self,
        id: ObjectId,
        path: &str,
        active: &mut BTreeSet<(ObjectId, String)>,
        done: &mut BTreeSet<(ObjectId, String)>,
    ) -> Result<(), String> {
        let key = (id, path.to_owned());
        if done.contains(&key) {
            return Ok(());
        }
        if active.len() >= 64 {
            return Err("Parameter dependency depth exceeds 64".into());
        }
        if !active.insert(key.clone()) {
            return Err("Parameter reference cycle".into());
        }
        let binding = self.node(id)?.channels().binding(path);
        let references = match binding {
            Some(Binding::Link { reference }) => vec![reference.target],
            Some(Binding::Expression { expression }) => {
                if expression.infer(&mut |r| {
                    self.reference_owner(r)?;
                    Ok(Type::Scalar)
                })? != Type::Scalar
                {
                    return Err("Component expression must produce a scalar".into());
                }
                expression.references()?
            }
            _ => vec![],
        };
        for reference in references {
            let (node, path, _) = self.reference_owner(reference)?;
            self.visit_binding(node, &path, active, done)?;
        }
        active.remove(&key);
        done.insert(key);
        Ok(())
    }
    pub fn sample_reference(&self, reference: Reference, time: Time) -> Result<f64, String> {
        Sampler::new(self, time).reference(reference)
    }
    pub fn evaluated_parameters(
        &self,
        id: ObjectId,
        time: Time,
    ) -> Result<crate::OperationConfig, String> {
        let node = self.node(id)?;
        let mut config = node.config().clone();
        // Prepared static Roto controls are already validated. Avoid generating
        // thousands of point-property paths when there is nothing to sample.
        if matches!(config, crate::OperationConfig::Roto { .. }) && node.channels().len() == 0 {
            return Ok(config);
        }
        let mut sampler = Sampler::new(self, time);
        // Walk native component addresses once. Regenerating all properties for
        // each keyed component makes a fully keyed shape quadratic to sample.
        let mut offsets = BTreeMap::new();
        for mut property in config.animated_properties() {
            let parameter = node.property_owner(&property.id)?;
            let offset = offsets.entry(parameter).or_insert(0usize);
            let mut changed = false;
            for (i, value) in property.values.iter_mut().enumerate() {
                if node.channels().binding(&path(&property.id, i)).is_some() {
                    *value = sampler.reference(Reference::Parameter {
                        parameter,
                        component: Some(
                            u8::try_from(*offset + i)
                                .map_err(|_| "Too many parameter components")?,
                        ),
                    })?;
                    changed = true;
                }
            }
            *offset += property.values.len();
            if changed {
                config.set_property(&property.id, &property.values)?;
            }
        }
        config.validate()?;
        Ok(config)
    }
    pub fn evaluated_effect(&self, id: ObjectId, time: Time) -> Result<crate::Effect, String> {
        let node = self.node(id)?;
        let mut effect = node.effect.clone();
        if node.channels().binding("mix.0").is_some() {
            effect.mix = self.sample_reference(node.property_reference("mix", 0)?, time)? as f32;
        }
        effect.validate()?;
        Ok(effect)
    }
}
struct Sampler<'a, 'b> {
    graph: &'a PreparedComposite<'b>,
    time: Time,
    cache: BTreeMap<Reference, f64>,
    active: BTreeSet<Reference>,
    remaining: usize,
}
impl<'a, 'b> Sampler<'a, 'b> {
    fn new(graph: &'a PreparedComposite<'b>, time: Time) -> Self {
        Self {
            graph,
            time,
            cache: BTreeMap::new(),
            active: BTreeSet::new(),
            remaining: 65536,
        }
    }
    fn reference(&mut self, reference: Reference) -> Result<f64, String> {
        if let Some(value) = self.cache.get(&reference) {
            return Ok(*value);
        }
        if self.active.len() >= 64 || !self.active.insert(reference) {
            return Err("Parameter cycle or dependency depth exceeded".into());
        }
        let (id, path, base) = self.graph.reference_owner(reference)?;
        let value = match self.graph.node(id)?.channels().binding(&path) {
            None => base,
            Some(Binding::Track { track }) => match track.sample(self.time)? {
                Literal::Scalar { value } => value,
                _ => return Err("Expected scalar track".into()),
            },
            Some(Binding::Link { reference }) => self.reference(reference.target)?,
            Some(Binding::Expression { expression }) => {
                let references = expression.references()?;
                let mut inputs = BTreeMap::new();
                for reference in references {
                    inputs.insert(reference, self.reference(reference)?);
                }
                // Each term and dependency consumes one shared request allowance.
                let context = fold_graph::execution::Context {
                    scene_time: self.time,
                    asset_time: self.time,
                    seed: 0,
                    operation: id.to_bytes(),
                    element: None,
                };
                let steps = std::cell::Cell::new(self.remaining);
                let check = || {
                    let n = steps
                        .get()
                        .checked_sub(1)
                        .ok_or("Expression evaluation budget exhausted")?;
                    steps.set(n);
                    Ok(())
                };
                let value = expression.evaluate(
                    &context,
                    &mut fold_graph::execution::Budget::new(self.remaining, &check),
                    &mut |r| {
                        Ok(Literal::Scalar {
                            value: *inputs.get(&r).ok_or("Missing parameter input")?,
                        })
                    },
                )?;
                self.remaining = steps.get();
                match value {
                    Literal::Scalar { value } => value,
                    _ => return Err("Expected scalar expression".into()),
                }
            }
            _ => return Err("Unsupported component binding".into()),
        };
        if !value.is_finite() {
            return Err("Expression result is not finite".into());
        }
        self.active.remove(&reference);
        self.cache.insert(reference, value);
        Ok(value)
    }
}
