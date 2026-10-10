//! Native scalar bindings. Curves are transient views for the shared editor.
use fold_animation::Curve;
use fold_foundation::Time;
use fold_graph::{
    binding::{Binding, Track},
    model::Parameter,
    types::{Literal, Type},
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Channels(BTreeMap<String, Parameter>);

impl Channels {
    pub(crate) fn reconcile(base: &Self, proposed: &Self, current: &Self) -> Result<Self, String> {
        let result = Self(fold_graph::reconcile::parameters(
            &base.0,
            &proposed.0,
            &current.0,
        )?);
        result.validate()?;
        Ok(result)
    }
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
    pub fn binding(&self, path: &str) -> Result<Option<Binding>, String> {
        self.0.get(path).map(Parameter::binding).transpose()
    }
    pub fn set_binding(&mut self, path: String, mut binding: Binding) -> Result<(), String> {
        // Inspector expression text omits metadata; retain existing edge identities
        // and extension data for references that still target the same parameter.
        if let Some(previous) = self.binding(&path)? {
            let mut edges = match previous {
                Binding::Link { reference } => vec![reference],
                Binding::Expression { expression } => {
                    expression.edges()?.into_iter().cloned().collect()
                }
                _ => vec![],
            };
            let mut retain = |edge: &mut fold_graph::model::Edge| {
                if let Some(index) = edges.iter().position(|old| old.target == edge.target) {
                    *edge = edges.remove(index);
                }
                Ok(())
            };
            match &mut binding {
                Binding::Link { reference } => retain(reference)?,
                Binding::Expression { expression } => {
                    edit_expression_edges(expression, &mut retain)?
                }
                _ => {}
            }
        }
        validate_binding(&binding)?;
        let mut next = self
            .0
            .get(&path)
            .cloned()
            .unwrap_or(Parameter::new(binding.clone())?);
        let value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
        if next.protected && next.value != value {
            return Err("Animation parameter is protected".into());
        }
        next.value = value;
        self.0.insert(path, next);
        Ok(())
    }
    pub fn driven(&self, path: &str) -> bool {
        matches!(
            self.binding(path),
            Ok(Some(Binding::Expression { .. } | Binding::Link { .. }))
        )
    }
    pub fn bindings(&self) -> &BTreeMap<String, Parameter> {
        &self.0
    }
    fn track(parameter: &Parameter) -> Result<Track, String> {
        let Binding::Track { track } = parameter.binding()? else {
            return Err("Compositor animation requires a typed track".into());
        };
        track.validate()?;
        if track.ty != Type::Scalar {
            return Err("Compositor component channels require scalar tracks".into());
        }
        Ok(track)
    }
    pub fn curve(&self, path: &str) -> Result<Option<Curve>, String> {
        match self.binding(path)? {
            Some(Binding::Track { track }) => {
                Ok(Some(fold_graph::curves::curves(&track)?.remove(0).1))
            }
            _ => Ok(None),
        }
    }
    pub fn sample(&self, path: &str, time: Time) -> Result<Option<f64>, String> {
        self.0
            .get(path)
            .map(|parameter| {
                let Literal::Scalar { value } = Self::track(parameter)?.sample(time)? else {
                    return Err("Expected scalar animation result".into());
                };
                Ok(value)
            })
            .transpose()
    }
    pub fn project(&self) -> Result<BTreeMap<String, Curve>, String> {
        self.0
            .keys()
            .filter_map(|path| match self.curve(path) {
                Ok(Some(curve)) => Some(Ok((path.clone(), curve))),
                Ok(None) => None,
                Err(e) => Some(Err(e)),
            })
            .collect()
    }
    /// Edit key data while retaining parameter, track, key and extension identities.
    pub fn insert(&mut self, path: String, curve: Curve) -> Result<(), String> {
        curve.validate()?;
        let first = curve.keys.first().ok_or("Empty animation channel")?;
        let previous = self.0.get(&path);
        if previous.is_some_and(|parameter| parameter.protected) {
            if self.curve(&path)?.as_ref() == Some(&curve) {
                return Ok(());
            }
            return Err("Animation parameter is protected".into());
        }
        let track = match previous {
            Some(parameter) => Self::track(parameter)?,
            None => Track::constant(Literal::Scalar { value: first.value }, first.time)?,
        };
        let track = fold_graph::curves::replace(&track, &[(String::new(), curve)])?;
        let parameter = match previous {
            Some(previous) => {
                let mut parameter = previous.clone();
                parameter.value =
                    serde_json::to_value(Binding::Track { track }).map_err(|e| e.to_string())?;
                parameter
            }
            None => Parameter::new(Binding::Track { track })?,
        };
        self.0.insert(path, parameter);
        Ok(())
    }
    /// Apply an editor projection atomically; an empty curve removes its binding.
    pub fn replace(&mut self, curves: &BTreeMap<String, Curve>) -> Result<(), String> {
        let mut next = self.clone();
        let removed: Vec<_> = self
            .0
            .keys()
            .filter(|path| !curves.contains_key(*path) && !self.driven(path))
            .cloned()
            .collect();
        for path in removed {
            next.remove(&path)?;
        }
        for (path, curve) in curves {
            if curve.keys.is_empty() {
                next.remove(path)?;
            } else {
                next.insert(path.clone(), curve.clone())?;
            }
        }
        *self = next;
        Ok(())
    }
    pub fn remove(&mut self, path: &str) -> Result<(), String> {
        if self
            .0
            .get(path)
            .is_some_and(|parameter| parameter.protected)
        {
            return Err("Animation parameter is protected".into());
        }
        self.0.remove(path);
        Ok(())
    }
    /// A shader interface rename moves the native record, not a reconstructed curve.
    pub fn remap(&self, names: &BTreeMap<String, Option<String>>) -> Result<Self, String> {
        let mut result = BTreeMap::new();
        for (path, parameter) in &self.0 {
            let next = names
                .get(path)
                .cloned()
                .unwrap_or_else(|| Some(path.clone()));
            if parameter.protected && next.as_ref() != Some(path) {
                return Err("Animation parameter is protected".into());
            }
            if let Some(next) = next
                && result.insert(next, parameter.clone()).is_some()
            {
                return Err("Animation identity mapping collides".into());
            }
        }
        Ok(Self(result))
    }
    pub fn duplicate(
        &self,
        remap: &BTreeMap<fold_foundation::ObjectId, fold_foundation::ObjectId>,
    ) -> Result<Self, String> {
        let mut copy = self.clone();
        for parameter in copy.0.values_mut() {
            parameter.id = fold_foundation::ObjectId::new();
            let mut binding = parameter.binding()?;
            match &mut binding {
                Binding::Track { track } => {
                    track.id = fold_foundation::ObjectId::new();
                    for key in &mut track.keys {
                        key.id = fold_foundation::ObjectId::new();
                    }
                }
                Binding::Link { reference } => remap_edge(reference, remap),
                Binding::Expression { expression } => remap_expression(expression, remap),
                _ => {}
            }
            parameter.value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
        }
        Ok(copy)
    }
    pub(crate) fn remap_references(
        &mut self,
        remap: &BTreeMap<fold_graph::model::Reference, Option<fold_graph::model::Reference>>,
    ) -> Result<(), String> {
        let mut next = self.clone();
        for parameter in next.0.values_mut() {
            let mut binding = parameter.binding()?;
            let mut edit = |edge: &mut fold_graph::model::Edge| {
                if let Some(target) = remap.get(&edge.target) {
                    edge.target = target.ok_or(
                        "Remove parameter references before deleting their shader control",
                    )?;
                }
                Ok(())
            };
            match &mut binding {
                Binding::Link { reference } => edit(reference)?,
                Binding::Expression { expression } => edit_expression_edges(expression, &mut edit)?,
                _ => {}
            }
            parameter.value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
        }
        self.check_protection(&next)?;
        *self = next;
        Ok(())
    }
    pub fn validate(&self) -> Result<(), String> {
        self.prepare().map(|_| ())
    }
    pub fn check_protection(&self, next: &Self) -> Result<(), String> {
        for (path, parameter) in &self.0 {
            if parameter.protected && next.0.get(path) != Some(parameter) {
                return Err("Animation parameter is protected".into());
            }
        }
        Ok(())
    }
}

/// Decoded scalar contracts shared by validation, sampling and channel inspection.
pub(crate) struct PreparedChannels {
    bindings: BTreeMap<String, Binding>,
    identities: Vec<fold_foundation::ObjectId>,
    parameters: BTreeMap<fold_foundation::ObjectId, String>,
}
impl Channels {
    pub(crate) fn prepare(&self) -> Result<PreparedChannels, String> {
        let mut bindings = BTreeMap::new();
        let mut identities = Vec::new();
        let mut parameters = BTreeMap::new();
        let mut unique = std::collections::BTreeSet::new();
        for (path, parameter) in &self.0 {
            fold_graph::model::extension_keys(
                &parameter.extensions,
                &["id", "value", "protected"],
            )?;
            let binding = parameter.binding()?;
            validate_binding(&binding)?;
            let mut ids = vec![parameter.id];
            match &binding {
                Binding::Track { track } => {
                    ids.push(track.id);
                    ids.extend(track.keys.iter().map(|k| k.id));
                }
                Binding::Link { reference } => ids.push(reference.id),
                Binding::Expression { expression } => {
                    ids.extend(expression.edges()?.into_iter().map(|e| e.id))
                }
                _ => {}
            }
            for id in ids {
                if !unique.insert(id) {
                    return Err("Duplicate animation identity".into());
                }
                identities.push(id);
            }
            parameters.insert(parameter.id, path.clone());
            bindings.insert(path.clone(), binding);
        }
        Ok(PreparedChannels {
            bindings,
            identities,
            parameters,
        })
    }
}
impl PreparedChannels {
    pub fn parameter_path(&self, id: fold_foundation::ObjectId) -> Option<&str> {
        self.parameters.get(&id).map(String::as_str)
    }
    pub fn identities(&self) -> &[fold_foundation::ObjectId] {
        &self.identities
    }
    pub fn len(&self) -> usize {
        self.bindings.len()
    }
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.bindings.keys()
    }
    pub fn binding(&self, path: &str) -> Option<&Binding> {
        self.bindings.get(path)
    }
    pub fn curve(&self, path: &str) -> Result<Option<Curve>, String> {
        match self.bindings.get(path) {
            Some(Binding::Track { track }) => {
                Ok(Some(fold_graph::curves::curves(track)?.remove(0).1))
            }
            _ => Ok(None),
        }
    }
    pub fn shadow(&mut self, prefix: &str) {
        self.bindings.retain(|path, _| !path.starts_with(prefix));
    }
}

fn validate_binding(binding: &Binding) -> Result<(), String> {
    match binding {
        Binding::Track { track } => {
            track.validate()?;
            if track.ty != Type::Scalar {
                return Err("Compositor channels require scalar tracks".into());
            }
        }
        Binding::Link { reference } => reference.validate()?,
        Binding::Expression { expression } => {
            expression.references()?;
        }
        _ => return Err("Compositor channels require a track, reference or expression".into()),
    }
    Ok(())
}
fn remap_edge(
    edge: &mut fold_graph::model::Edge,
    remap: &BTreeMap<fold_foundation::ObjectId, fold_foundation::ObjectId>,
) {
    edge.id = fold_foundation::ObjectId::new();
    if let fold_graph::model::Reference::Parameter { parameter, .. } = &mut edge.target {
        if let Some(id) = remap.get(parameter) {
            *parameter = *id;
        }
    }
}
fn remap_expression(
    expr: &mut fold_graph::expression::Expr,
    remap: &BTreeMap<fold_foundation::ObjectId, fold_foundation::ObjectId>,
) {
    edit_expression_edges(expr, &mut |edge| {
        remap_edge(edge, remap);
        Ok(())
    })
    .expect("identity remapping cannot fail");
}

fn edit_expression_edges(
    expr: &mut fold_graph::expression::Expr,
    edit: &mut impl FnMut(&mut fold_graph::model::Edge) -> Result<(), String>,
) -> Result<(), String> {
    use fold_graph::expression::Expr;
    match expr {
        Expr::Reference { reference } => edit(reference)?,
        Expr::Unary { value, .. } => edit_expression_edges(value, edit)?,
        Expr::Binary { left, right, .. } => {
            edit_expression_edges(left, edit)?;
            edit_expression_edges(right, edit)?;
        }
        Expr::Call { arguments, .. } => {
            for expr in arguments {
                edit_expression_edges(expr, edit)?;
            }
        }
        Expr::Vector { components, .. } => {
            for expr in components {
                edit_expression_edges(expr, edit)?;
            }
        }
        _ => {}
    }
    Ok(())
}
