//! Fixed layer properties exposed through the shared parameter service.
use crate::{Composition, Layer};
use fold_foundation::Time;
use fold_media::Cancel;
use fold_platform::parameters::{self, Descriptor, Literal, Target, Unit};
use fold_project::Document;

fn values(layer: &Layer, time: Time) -> Vec<(&'static str, Literal)> {
    let mut values = vec![
        (
            "name",
            Literal::Text {
                value: layer.name.clone(),
            },
        ),
        (
            "visible",
            Literal::Bool {
                value: layer.visible,
            },
        ),
        (
            "locked",
            Literal::Bool {
                value: layer.locked,
            },
        ),
        ("start", Literal::Duration { value: layer.start }),
        ("end", Literal::Duration { value: layer.end }),
        (
            "origin",
            Literal::Duration {
                value: layer.origin,
            },
        ),
        (
            "source_start",
            Literal::Duration {
                value: layer.source_start,
            },
        ),
        (
            "opacity",
            Literal::Scalar {
                value: layer
                    .sample("scene.opacity.0", layer.opacity, time)
                    .clamp(0., 1.),
            },
        ),
        (
            "adjustment",
            Literal::Bool {
                value: layer.adjustment,
            },
        ),
    ];
    // This is the same local affine matrix used by the compositor. Parent
    // placement is separate; inspecting a local control never bakes its parent.
    for (index, (name, value)) in [
        "transform.a",
        "transform.b",
        "transform.c",
        "transform.d",
        "transform.tx",
        "transform.ty",
    ]
    .into_iter()
    .zip(layer.matrix(time))
    .enumerate()
    {
        values.push((
            name,
            if index < 4 {
                Literal::Scalar { value }
            } else {
                Literal::Distance {
                    value,
                    unit: Unit::Canvas,
                }
            },
        ));
    }
    values
}

fn layer_path(composition: &Composition, layer: &Layer) -> Result<Vec<String>, String> {
    let mut path = vec![layer.name.clone()];
    let mut parent = layer.parent;
    while let Some(id) = parent {
        let ancestor = composition.layer(id)?;
        path.push(ancestor.name.clone());
        parent = ancestor.parent;
    }
    path.reverse();
    Ok(path)
}

pub(crate) fn sources(document: &Document) -> Result<Vec<parameters::Source>, String> {
    let composition = Composition::from_document(document)?;
    composition
        .layers
        .iter()
        .filter_map(|layer| match &layer.source {
            Some(fold_platform::image_source::Source::Output(reference)) => {
                Some((layer, reference))
            }
            _ => None,
        })
        .map(|(layer, reference)| {
            Ok(parameters::Source {
                owner: layer.id,
                path: layer_path(&composition, layer)?,
                reference: reference.clone(),
                origin: layer.origin,
                source_start: layer.source_start,
                editable: !composition.locked(layer.id),
            })
        })
        .collect()
}

pub(crate) fn catalog(document: &Document) -> Result<Vec<Descriptor>, String> {
    let composition = Composition::from_document(document)?;
    let mut result = vec![];
    for layer in &composition.layers {
        let path = layer_path(&composition, layer)?;
        for (key, value) in values(layer, Time::ZERO) {
            let mut property_path = path.clone();
            property_path.extend(key.split('.').map(String::from));
            result.extend(parameters::property(
                document.id,
                Some(layer.id),
                key,
                property_path,
                &value,
                !composition.locked(layer.id),
            ));
        }
    }
    Ok(result)
}

pub(crate) fn sample(
    document: &Document,
    target: &Target,
    time: Time,
    cancel: &Cancel,
) -> Result<Literal, String> {
    cancel.check()?;
    if !target.occurrence.is_empty() {
        return Err("Layer properties have no graph occurrence".into());
    }
    let composition = Composition::from_document(document)?;
    let layer = composition.layer(target.object.ok_or("Missing layer owner")?)?;
    let key = target
        .parameter
        .property()
        .ok_or("Expected a layer property key")?;
    let value = values(layer, time)
        .into_iter()
        .find(|(name, _)| *name == key)
        .map(|(_, value)| value)
        .ok_or("Unknown layer property")?;
    parameters::select_component(value, target)
}
