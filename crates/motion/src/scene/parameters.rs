//! Scene placement controls and embedded graph occurrences share parameter paths.
use super::{Object, Scene};
use crate::types::Literal;
use fold_foundation::Time;
use fold_media::Cancel;
use fold_platform::parameters::{self, Descriptor, Target};
use fold_project::Document;

fn properties(object: &Object, time: Time) -> Result<Vec<(&'static str, Literal)>, String> {
    Ok(vec![
        (
            "name",
            Literal::Text {
                value: object.name.clone(),
            },
        ),
        (
            "visible",
            Literal::Bool {
                value: object.visible,
            },
        ),
        (
            "locked",
            Literal::Bool {
                value: object.locked,
            },
        ),
        ("position", super::sample(&object.position, time)?),
        ("rotation", super::sample(&object.rotation, time)?),
        ("scale", super::sample(&object.scale, time)?),
        (
            "clock.offset",
            Literal::Duration {
                value: object.clock.offset,
            },
        ),
        (
            "clock.rate.numerator",
            Literal::Integer {
                value: object.clock.rate.numerator(),
            },
        ),
        (
            "clock.rate.denominator",
            Literal::Integer {
                value: i64::from(object.clock.rate.denominator()),
            },
        ),
    ])
}

pub(super) fn catalog(document: &Document) -> Result<Vec<Descriptor>, String> {
    let scene = Scene::from_document(document)?;
    let mut result = vec![];
    for object in &scene.objects {
        let prefix: Vec<_> = scene
            .ancestors(object.id)?
            .iter()
            .map(|o| o.name.clone())
            .collect();
        for (key, value) in properties(object, Time::ZERO)? {
            let mut path = prefix.clone();
            path.extend(key.split('.').map(String::from));
            result.extend(parameters::property(
                document.id,
                Some(object.id),
                key,
                path,
                &value,
                !scene.locked(object.id),
            ));
        }
        for mut entry in crate::parameters::catalog(&object.graph, document.id, Some(object.id))? {
            entry.path.splice(0..0, prefix.clone());
            entry.editable &= !scene.locked(object.id);
            result.push(entry);
            if result.len() > 131072 {
                return Err("Scene parameter catalog budget exceeded".into());
            }
        }
    }
    Ok(result)
}

pub(super) fn sample(
    document: &Document,
    target: &Target,
    time: Time,
    cancel: &Cancel,
) -> Result<Literal, String> {
    sample_with_images(document, target, time, cancel, None)
}
pub(super) fn sample_with_images(
    document: &Document,
    target: &Target,
    time: Time,
    cancel: &Cancel,
    images: Option<&dyn fold_platform::image_sampling::Resolver>,
) -> Result<Literal, String> {
    cancel.check()?;
    let scene = Scene::from_document(document)?;
    let id = target.object.ok_or("Select a scene object")?;
    let mut local_time = time;
    for ancestor in scene.ancestors(id)? {
        local_time = ancestor.clock.at(local_time)?;
    }
    let object = scene.object(id)?;
    if let Some(key) = target.parameter.property() {
        if !target.occurrence.is_empty() {
            return Err("Scene placement has no graph occurrence".into());
        }
        let value = properties(object, local_time)?
            .into_iter()
            .find(|(name, _)| *name == key)
            .map(|(_, value)| value)
            .ok_or("Unknown scene property")?;
        return parameters::select_component(value, target);
    }
    crate::parameters::sample_with_images(object.graph.clone(), target, local_time, cancel, images)
}
