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
    tracks: BTreeMap<String, Track>,
    identities: Vec<fold_foundation::ObjectId>,
    parameters: BTreeMap<fold_foundation::ObjectId, String>,
}
impl Channels {
    pub(crate) fn prepare(&self) -> Result<PreparedChannels, String> {
        let mut tracks = BTreeMap::new();
        let mut identities = Vec::new();
        let mut parameters = BTreeMap::new();
        let mut unique = std::collections::BTreeSet::new();
        for (path, parameter) in &self.0 {
            fold_graph::model::extension_keys(
                &parameter.extensions,
                &["id", "value", "protected"],
            )?;
            let track = Self::track(parameter)?;
            for id in [parameter.id, track.id]
                .into_iter()
                .chain(track.keys.iter().map(|key| key.id))
            {
                if !unique.insert(id) {
                    return Err("Duplicate animation identity".into());
                }
                identities.push(id);
            }
            parameters.insert(parameter.id, path.clone());
            tracks.insert(path.clone(), track);
        }
        Ok(PreparedChannels {
            tracks,
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
        self.tracks.len()
    }
    pub fn paths(&self) -> impl Iterator<Item = &String> {
        self.tracks.keys()
    }
    pub fn sample(&self, path: &str, time: Time) -> Result<Option<f64>, String> {
        self.tracks
            .get(path)
            .map(|track| {
                let Literal::Scalar { value } = track.sample(time)? else {
                    return Err("Expected scalar animation result".into());
                };
                Ok(value)
            })
            .transpose()
    }
    pub fn curve(&self, path: &str) -> Result<Option<Curve>, String> {
        self.tracks
            .get(path)
            .map(|track| Ok(fold_graph::curves::curves(track)?.remove(0).1))
            .transpose()
    }
    pub fn shadow(&mut self, prefix: &str) {
        self.tracks.retain(|path, _| !path.starts_with(prefix));
    }
}
