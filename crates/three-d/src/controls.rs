//! Shared controls and animation channels are projections of native parameters.
//! Their targets and coupled key IDs are not independently stored UI state.
use crate::{
    editing::{self, Edit},
    evaluation,
    model::Scene,
    package::Provider,
};
use fold_foundation::{ObjectId, Time};
use fold_graph::{
    binding::{Binding, Combination, Interpolation, Key, Track},
    curves,
    types::Literal,
};
use fold_media::Cancel;
use fold_platform::{
    animation,
    packages::{Animation, Parameters},
    parameters,
};
use fold_project::Document;
use std::collections::{BTreeMap, BTreeSet};

impl Parameters for Provider {
    fn parameters(&self, document: &Document) -> Result<Vec<parameters::Descriptor>, String> {
        let scene = Scene::from_document(document)?;
        let editable = editing::editable_ids(&scene);
        let mut result = vec![];
        for (&id, object) in &scene.objects {
            for (label, parameter) in object.named_parameters() {
                let value = evaluation::sample(parameter, Time::ZERO)?;
                result.extend(parameters::with_components(parameters::Descriptor {
                    target: parameters::Target {
                        document: document.id,
                        placement: None,
                        object: Some(id),
                        occurrence: vec![],
                        parameter: parameter.id.into(),
                        component: None,
                    },
                    path: vec![object.name.clone(), label.into()],
                    value_type: Some(value.ty()),
                    editable: editable.contains(&id) && !parameter.protected,
                }));
            }
        }
        Ok(result)
    }
    fn sample_parameter(
        &self,
        document: &Document,
        target: &parameters::Target,
        time: Time,
        cancel: &Cancel,
    ) -> Result<Literal, String> {
        cancel.check().map_err(|e| e.to_string())?;
        if target.document != document.id
            || target.placement.is_some()
            || !target.occurrence.is_empty()
        {
            return Err("Parameter target must be local to this 3D document".into());
        }
        let scene = Scene::from_document(document)?;
        let object = scene
            .objects
            .get(&target.object.ok_or("Missing 3D object target")?)
            .ok_or("Missing 3D object")?;
        let parameter = object.parameter(
            target
                .parameter
                .native()
                .ok_or("3D transform parameters require stable IDs")?,
        )?;
        let value = parameters::select_component(evaluation::sample(parameter, time)?, target)?;
        cancel.check().map_err(|e| e.to_string())?;
        Ok(value)
    }
    fn parameter_time(&self, document: &Document, output_time: Time) -> Result<Time, String> {
        Scene::from_document(document)?;
        Ok(output_time)
    }
}
impl Animation for Provider {
    fn animation_channels(
        &self,
        document: &Document,
        selected: Option<ObjectId>,
    ) -> Result<Vec<animation::Channel>, String> {
        let scene = Scene::from_document(document)?;
        if selected.is_some_and(|id| !scene.objects.contains_key(&id)) {
            return Err("Missing 3D animation object".into());
        }
        let mut result = vec![];
        for (&id, object) in &scene.objects {
            if selected.is_some_and(|selected| selected != id) {
                continue;
            }
            for (label, parameter) in object.named_parameters() {
                if let Binding::Track { track } = parameter.binding()? {
                    for (component, curve) in curves::curves(&track)? {
                        result.push(animation::Channel {
                            target: animation::Target {
                                group: None,
                                object: id,
                                path: format!("{}/{component}", parameter.id),
                            },
                            node_label: object.name.clone(),
                            property: parameter.id.to_string(),
                            property_label: label.into(),
                            component,
                            curve,
                        });
                    }
                }
            }
        }
        Ok(result)
    }
    fn edit_animation(
        &self,
        document: &Document,
        channels: &[animation::Channel],
        time: Time,
    ) -> Result<Document, String> {
        let scene = Scene::from_document(document)?;
        let available = self
            .animation_channels(document, None)?
            .into_iter()
            .map(|channel| (channel.target.clone(), channel))
            .collect::<BTreeMap<_, _>>();
        let mut seen = BTreeSet::new();
        let mut grouped =
            BTreeMap::<(ObjectId, ObjectId), Vec<(String, fold_animation::Curve)>>::new();
        for channel in channels {
            let original = available
                .get(&channel.target)
                .ok_or("Missing 3D animation target")?;
            if !seen.insert(channel.target.clone())
                || channel.property != original.property
                || channel.component != original.component
            {
                return Err("Animation edit has an inconsistent or repeated target".into());
            }
            let id: ObjectId =
                serde_json::from_value(serde_json::Value::String(original.property.clone()))
                    .map_err(|_| "Invalid 3D parameter ID")?;
            grouped
                .entry((channel.target.object, id))
                .or_default()
                .push((channel.component.clone(), channel.curve.clone()));
        }
        let mut edits = vec![];
        for ((object, id), components) in grouped {
            let expected = scene
                .objects
                .get(&object)
                .ok_or("Missing 3D object")?
                .parameter(id)?
                .clone();
            let Binding::Track { track } = expected.binding()? else {
                return Err("3D parameter has no animation track".into());
            };
            let binding = if components.iter().all(|(_, curve)| curve.keys.is_empty()) {
                // Typed keys are coupled. Clearing their projected component
                // curves captures the current pose as a native static value.
                Binding::Literal {
                    value: track.sample(time)?,
                }
            } else {
                Binding::Track {
                    track: curves::replace(&track, &components)?,
                }
            };
            let mut value = expected.clone();
            value.value = serde_json::to_value(binding).map_err(|e| e.to_string())?;
            edits.push(Edit::Parameter {
                object,
                expected,
                value,
            });
        }
        let after = editing::apply(&scene, &edits, &Cancel::default())?;
        let mut result = after.document(document.id)?;
        result.revision = document.revision;
        result.extensions = document.extensions.clone();
        Ok(result)
    }
}

pub(crate) fn edited_binding(
    binding: Binding,
    value: Literal,
    time: Time,
    key: bool,
) -> Result<Binding, String> {
    let mut track = match binding {
        Binding::Literal { value: original } if key => Track::constant(original, Time::ZERO)?,
        Binding::Literal { .. } => return Ok(Binding::Literal { value }),
        Binding::Track { track } if track.combination == Combination::Replace => track,
        _ => return Err("3D transform edits require literal or replacement animation".into()),
    };
    if let Some(key) = track.keys.iter_mut().find(|key| key.time == time) {
        key.value = value;
    } else {
        track.keys.push(Key {
            id: ObjectId::new(),
            time,
            value,
            interpolation: Interpolation::Linear,
            handles: vec![],
            extensions: Default::default(),
        });
        track.keys.sort_by_key(|key| key.time);
    }
    track.validate()?;
    Ok(Binding::Track { track })
}
