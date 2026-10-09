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
        self.0
            .get(path)
            .map(|parameter| {
                let track = Self::track(parameter)?;
                Ok(fold_graph::curves::curves(&track)?.remove(0).1)
            })
            .transpose()
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
            .map(|path| Ok((path.clone(), self.curve(path)?.ok_or("Missing track")?)))
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
            .filter(|path| !curves.contains_key(*path))
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
    /// Instance values shadow definition tracks in a transient evaluation copy.
    /// Protection still applies to every authored mutation of the definition.
    pub(crate) fn for_evaluation_without(&self, prefix: &str) -> Self {
        Self(
            self.0
                .iter()
                .filter(|(path, _)| !path.starts_with(prefix))
                .map(|(path, parameter)| (path.clone(), parameter.clone()))
                .collect(),
        )
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
    pub fn duplicate(&self) -> Result<Self, String> {
        let mut copy = self.clone();
        for parameter in copy.0.values_mut() {
            parameter.id = fold_foundation::ObjectId::new();
            let mut track = Self::track(parameter)?;
            track.id = fold_foundation::ObjectId::new();
            for key in &mut track.keys {
                key.id = fold_foundation::ObjectId::new();
            }
            parameter.value =
                serde_json::to_value(Binding::Track { track }).map_err(|e| e.to_string())?;
        }
        Ok(copy)
    }
    pub(crate) fn identities(&self) -> Result<Vec<fold_foundation::ObjectId>, String> {
        let mut ids = Vec::new();
        for parameter in self.0.values() {
            let track = Self::track(parameter)?;
            ids.extend([parameter.id, track.id]);
            ids.extend(track.keys.iter().map(|key| key.id));
        }
        Ok(ids)
    }
    pub fn validate(&self) -> Result<(), String> {
        let mut ids = std::collections::BTreeSet::new();
        for parameter in self.0.values() {
            fold_graph::model::extension_keys(
                &parameter.extensions,
                &["id", "value", "protected"],
            )?;
            let track = Self::track(parameter)?;
            for id in [parameter.id, track.id]
                .into_iter()
                .chain(track.keys.iter().map(|key| key.id))
            {
                if !ids.insert(id) {
                    return Err("Duplicate animation identity".into());
                }
            }
        }
        Ok(())
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
