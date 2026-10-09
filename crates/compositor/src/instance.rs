//! Compositor instance overrides are native parameters. The shared host effect
//! service receives a disposable invocation projection, never authored storage.
use fold_foundation::{ObjectId, Time};
use fold_graph::{
    binding::{Binding, Track},
    model::Parameter,
    types::Literal,
};
use fold_project::{DocumentRef, Metadata};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Instance {
    pub id: ObjectId,
    pub definition: DocumentRef,
    pub version: u32,
    pub overrides: BTreeMap<String, Parameter>,
    pub enabled: bool,
    pub extensions: Metadata,
}
impl Instance {
    pub fn capture(host: &fold_platform::effects::Instance) -> Result<Self, String> {
        host.validate()?;
        let mut overrides = BTreeMap::new();
        let names: std::collections::BTreeSet<_> =
            host.values.keys().chain(host.animation.keys()).collect();
        for name in names {
            let values = if let Some(curves) = host.animation.get(name) {
                curves
                    .iter()
                    .map(|curve| {
                        let key = curve.keys.first().ok_or("Empty effect animation")?;
                        let track =
                            Track::constant(Literal::Scalar { value: key.value }, key.time)?;
                        let track =
                            fold_graph::curves::replace(&track, &[(String::new(), curve.clone())])?;
                        Ok(Binding::Track { track })
                    })
                    .collect::<Result<Vec<_>, String>>()?
            } else {
                host.values[name]
                    .iter()
                    .map(|value| Binding::Literal {
                        value: Literal::Scalar { value: *value },
                    })
                    .collect()
            };
            overrides.insert(name.clone(), Parameter::new(Binding::List { values })?);
        }
        Ok(Self {
            id: host.id,
            definition: host.definition.clone(),
            version: host.version,
            overrides,
            enabled: host.enabled,
            extensions: host.extensions.clone(),
        })
    }
    pub fn host(&self) -> Result<fold_platform::effects::Instance, String> {
        let mut host = fold_platform::effects::Instance {
            id: self.id,
            definition: self.definition.clone(),
            version: self.version,
            values: BTreeMap::new(),
            animation: BTreeMap::new(),
            enabled: self.enabled,
            extensions: self.extensions.clone(),
        };
        for (name, parameter) in &self.overrides {
            let Binding::List { values } = parameter.binding()? else {
                return Err("Effect override requires component bindings".into());
            };
            let mut defaults = Vec::new();
            let mut curves = Vec::new();
            let mut animated = false;
            for (component, binding) in values.into_iter().enumerate() {
                match binding {
                    Binding::Literal {
                        value: Literal::Scalar { value },
                    } => {
                        defaults.push(value);
                        let mut curve = fold_animation::Curve::default();
                        curve.insert(Time::ZERO, value);
                        curve.keys[0].id = literal_key(parameter.id, component);
                        curves.push(curve);
                    }
                    Binding::Track { track } => {
                        track.validate()?;
                        let Literal::Scalar { value } = track.sample(Time::ZERO)? else {
                            return Err("Effect override track requires scalar components".into());
                        };
                        defaults.push(value);
                        curves.push(fold_graph::curves::curves(&track)?.remove(0).1);
                        animated = true;
                    }
                    _ => return Err("Effect override requires scalar literals or tracks".into()),
                }
            }
            host.values.insert(name.clone(), defaults);
            if animated {
                host.animation.insert(name.clone(), curves);
            }
        }
        host.validate()?;
        Ok(host)
    }
    pub fn reconcile(&mut self, before: &Self) -> Result<(), String> {
        for (name, previous) in &before.overrides {
            let replacement = self.overrides.get_mut(name);
            if let Some(replacement) = replacement {
                let Binding::List { values: old } = previous.binding()? else {
                    return Err("Invalid previous effect override".into());
                };
                let Binding::List { mut values } = replacement.binding()? else {
                    return Err("Invalid effect override".into());
                };
                for (component, (next, old)) in values.iter_mut().zip(old).enumerate() {
                    match (&mut *next, old) {
                        (Binding::Track { track: next }, Binding::Track { track: old }) => {
                            let curves = fold_graph::curves::curves(next)?;
                            *next = fold_graph::curves::replace(&old, &curves)?;
                        }
                        (Binding::Track { track }, Binding::Literal { value })
                            if track.keys.len() == 1
                                && track.keys[0].id == literal_key(previous.id, component)
                                && track.keys[0].time == Time::ZERO
                                && track.keys[0].value == value =>
                        {
                            // Host controls require a curve for each component.
                            // An unchanged synthetic curve is still a native literal.
                            *next = Binding::Literal { value };
                        }
                        _ => {}
                    }
                }
                replacement.value =
                    serde_json::to_value(Binding::List { values }).map_err(|e| e.to_string())?;
                replacement.id = previous.id;
                replacement.protected = previous.protected;
                replacement.extensions = previous.extensions.clone();
                if previous.protected && replacement != previous {
                    return Err(format!("Protected instance control {name}"));
                }
            } else if previous.protected {
                return Err(format!("Protected instance control {name}"));
            }
        }
        Ok(())
    }
    pub fn check_protection(&self, next: Option<&Self>) -> Result<(), String> {
        for (name, parameter) in &self.overrides {
            if parameter.protected
                && (next
                    .is_none_or(|n| n.definition != self.definition || n.version != self.version)
                    || next.and_then(|n| n.overrides.get(name)) != Some(parameter))
            {
                return Err(format!("Protected instance control {name}"));
            }
        }
        Ok(())
    }
}
impl Instance {
    pub fn identities(&self) -> Result<Vec<ObjectId>, String> {
        let mut ids = vec![self.id];
        for parameter in self.overrides.values() {
            fold_graph::model::extension_keys(
                &parameter.extensions,
                &["id", "value", "protected"],
            )?;
            ids.push(parameter.id);
            let Binding::List { values } = parameter.binding()? else {
                return Err("Invalid effect override".into());
            };
            for value in values {
                if let Binding::Track { track } = value {
                    ids.push(track.id);
                    ids.extend(track.keys.iter().map(|key| key.id));
                }
            }
        }
        Ok(ids)
    }
    pub fn duplicate(&mut self) -> Result<(), String> {
        self.id = ObjectId::new();
        for parameter in self.overrides.values_mut() {
            parameter.id = ObjectId::new();
            let Binding::List { mut values } = parameter.binding()? else {
                return Err("Invalid effect override".into());
            };
            for value in &mut values {
                if let Binding::Track { track } = value {
                    track.id = ObjectId::new();
                    for key in &mut track.keys {
                        key.id = ObjectId::new();
                    }
                }
            }
            parameter.value =
                serde_json::to_value(Binding::List { values }).map_err(|e| e.to_string())?;
        }
        Ok(())
    }
}

// Projection-only key: stable across paints and distinct from its parameter ID.
// Reconciliation recognizes it without manufacturing authored animation.
fn literal_key(parameter: ObjectId, component: usize) -> ObjectId {
    let mut bytes = parameter.to_bytes();
    bytes[0] ^= 0x80;
    bytes[15] ^= component as u8;
    ObjectId::from_bytes(bytes)
}
