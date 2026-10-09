//! Protocol budgets may be tighter than native opaque-document storage limits.
use crate::model::Construction;
use fold_platform::proposals::Failure;
use fold_platform::proposals::FailureKind;
use std::collections::{BTreeMap, VecDeque};
pub fn construction(value: &Construction) -> Result<(), Failure> {
    if value
        .definitions
        .iter()
        .map(|d| d.nodes.len())
        .sum::<usize>()
        > 10000
    {
        return Err(Failure::new(
            FailureKind::Oversized,
            "Construction exceeds 10000 operations",
        ));
    }
    let definitions: BTreeMap<_, _> = value.definitions.iter().map(|d| (d.id, d)).collect();
    let mut incoming: BTreeMap<_, usize> = definitions.keys().map(|id| (*id, 0)).collect();
    let mut depths: BTreeMap<_, usize> = definitions.keys().map(|id| (*id, 1)).collect();
    for definition in definitions.values() {
        for instance in &definition.instances {
            *incoming.get_mut(&instance.definition).ok_or_else(|| {
                Failure::new(FailureKind::MissingResource, "Missing asset definition")
            })? += 1;
        }
    }
    let mut ready: VecDeque<_> = incoming
        .iter()
        .filter(|(_, count)| **count == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut visited = 0;
    while let Some(id) = ready.pop_front() {
        visited += 1;
        let depth = depths[&id];
        if depth > 64 {
            return Err(Failure::new(
                FailureKind::Oversized,
                "Asset nesting exceeds 64",
            ));
        }
        for instance in &definitions[&id].instances {
            depths
                .entry(instance.definition)
                .and_modify(|d| *d = (*d).max(depth + 1));
            let count = incoming
                .get_mut(&instance.definition)
                .expect("checked definition");
            *count -= 1;
            if *count == 0 {
                ready.push_back(instance.definition);
            }
        }
    }
    if visited != definitions.len() {
        return Err(Failure::new(FailureKind::Cycle, "Asset definition cycle"));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    use fold_foundation::DocumentId;
    #[test]
    fn admission_checks_longest_asset_path_without_expanding_instances() {
        let mut value =
            Construction::from_document(&crate::package::create(DocumentId::new()).unwrap())
                .unwrap();
        for count in 2..=65 {
            let child =
                Construction::from_document(&crate::package::create(DocumentId::new()).unwrap())
                    .unwrap();
            value
                .definitions
                .last_mut()
                .unwrap()
                .instances
                .push(child.root.clone());
            value.definitions.extend(child.definitions);
            if count <= 64 {
                assert!(construction(&value).is_ok());
            }
        }
        assert_eq!(
            construction(&value).err().unwrap().kind,
            FailureKind::Oversized
        );
        value.definitions.pop();
        value.definitions.last_mut().unwrap().instances.clear();
        value
            .definitions
            .last_mut()
            .unwrap()
            .instances
            .push(value.root.clone());
        assert_eq!(construction(&value).err().unwrap().kind, FailureKind::Cycle);
    }
}
