//! Three-way scene reconciliation. Graphs retain their native semantic merge;
//! hierarchy properties and opaque metadata are independently atomic values.
use super::{Object, Scene};
use std::collections::BTreeSet;

use fold_graph::reconcile::property;
fn object(base: &Object, proposed: &Object, current: &Object) -> Result<Object, String> {
    let mut result = current.clone();
    macro_rules! field {
        ($name:ident) => {
            result.$name = property(&base.$name, &proposed.$name, &current.$name)
                .map_err(|e| format!("Object {} {}: {e}", base.id, stringify!($name)))?;
        };
    }
    field!(name);
    field!(parent);
    field!(visible);
    field!(locked);
    field!(clock);
    field!(position);
    field!(rotation);
    field!(scale);
    field!(output);
    field!(extensions);
    result.graph = crate::merge::construction(&base.graph, &proposed.graph, &current.graph)
        .map_err(|e| format!("Object {} graph conflict: {e:?}", base.id))?;
    Ok(result)
}
/// Revalidates hierarchy cycles, dependencies and native identities. Callers must
/// also use `Scene::replace` against the current document to enforce protection.
pub fn merge(base: &Scene, proposed: &Scene, current: &Scene) -> Result<Scene, String> {
    let order = |s: &Scene| s.objects.iter().map(|o| o.id).collect::<Vec<_>>();
    let order = property(&order(base), &order(proposed), &order(current))?;
    // Conflicting topology edits require a new proposal. An edit to a removed
    // object cannot disappear merely because it is absent from the final order.
    let ids: BTreeSet<_> = base
        .objects
        .iter()
        .chain(&proposed.objects)
        .chain(&current.objects)
        .map(|o| o.id)
        .collect();
    let mut objects = std::collections::BTreeMap::new();
    for id in ids {
        let find = |s: &Scene| s.objects.iter().find(|o| o.id == id).cloned();
        let (b, p, c) = (find(base), find(proposed), find(current));
        let result = match (&b, &p, &c) {
            (Some(b), Some(p), Some(c)) => Some(object(b, p, c)?),
            _ => property(&b, &p, &c)?,
        };
        if let Some(result) = result {
            objects.insert(id, result);
        }
    }
    let result = Scene {
        canvas: property(&base.canvas, &proposed.canvas, &current.canvas)?,
        output: property(&base.output, &proposed.output, &current.output)?,
        active_camera: property(
            &base.active_camera,
            &proposed.active_camera,
            &current.active_camera,
        )?,
        extensions: property(&base.extensions, &proposed.extensions, &current.extensions)?,
        objects: order
            .into_iter()
            .map(|id| {
                objects
                    .remove(&id)
                    .ok_or("Merged scene order lost an object".into())
            })
            .collect::<Result<_, String>>()?,
    };
    if !objects.is_empty() {
        return Err("Merged scene contains unordered objects".into());
    }
    result.validate()?;
    Ok(result)
}
