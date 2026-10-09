//! Motion adapter for the shared parameter service. No parallel evaluator.
use crate::types::Literal;
use crate::{evaluation::Prepared, model::Construction};
use fold_foundation::{DocumentId, ObjectId, Time};
use fold_media::Cancel;
use fold_platform::parameters::{Descriptor, Target, with_components};

pub fn catalog(
    graph: &Construction,
    document: DocumentId,
    object: Option<ObjectId>,
) -> Result<Vec<Descriptor>, String> {
    let types = graph.parameter_types()?;
    let locked = crate::protection::locked_ids(graph).map_err(|e| format!("{e:?}"))?;
    let mut entries = vec![];
    let mut pending = vec![(&graph.root, vec![], vec![graph.root.name.clone()])];
    // Validated definitions are acyclic, but repeated instances can expand
    // exponentially. Bound the expansion independently of document size.
    let mut occurrences = 0;
    while let Some((instance, occurrence, path)) = pending.pop() {
        occurrences += 1;
        if occurrences > 32768 || occurrence.len() > 256 {
            return Err("Parameter occurrence budget exceeded".into());
        }
        let definition = graph.definition(instance.definition)?;
        let parameters = definition
            .inputs
            .iter()
            .map(|p| (p.id, vec![p.name.clone()]))
            .chain(definition.nodes.iter().flat_map(|node| {
                node.parameters
                    .iter()
                    .map(move |(name, p)| (p.id, vec![node.name.clone(), name.clone()]))
            }));
        for (parameter, suffix) in parameters {
            let mut parameter_path = path.clone();
            parameter_path.extend(suffix);
            entries.extend(with_components(Descriptor {
                target: Target {
                    document,
                    placement: None,
                    object,
                    occurrence: occurrence.clone(),
                    parameter: parameter.into(),
                    component: None,
                },
                path: parameter_path,
                value_type: types.get(&parameter).cloned(),
                editable: !instance
                    .overrides
                    .get(&parameter)
                    .is_some_and(|p| locked.contains(&p.id))
                    && !locked.contains(&parameter)
                    && !occurrence.iter().any(|id| locked.contains(id))
                    && !locked.contains(&graph.root.id),
            }));
            if entries.len() > 131072 {
                return Err("Parameter catalog budget exceeded".into());
            }
        }
        for instance in definition.instances.iter().rev() {
            let mut ids = occurrence.clone();
            ids.push(instance.id);
            let mut names = path.clone();
            names.push(instance.name.clone());
            pending.push((instance, ids, names));
        }
    }
    Ok(entries)
}

pub fn sample(
    graph: Construction,
    target: &Target,
    time: Time,
    cancel: &Cancel,
) -> Result<Literal, String> {
    cancel.check()?;
    let parameter = target
        .parameter
        .native()
        .ok_or("Expected a native Motion parameter")?;
    let prepared = Prepared::new(graph)?;
    let value = prepared
        .sample_parameters(&target.occurrence, &[parameter], time, &|| {
            Ok(cancel.check()?)
        })?
        .remove(&parameter)
        .ok_or("Parameter is not a primitive value")?;
    fold_platform::parameters::select_component(value, target)
}
