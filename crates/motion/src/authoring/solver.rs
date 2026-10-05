//! Create an editable identity update, ready to connect ordinary operations.
use super::*;
use crate::{fields::Kind, graph::Graph, nodes::interface::GroupDefinition};
use std::collections::BTreeMap;
pub fn create(m: &mut Motion, kind: Kind) -> Result<ObjectId, String> {
    if !matches!(kind, Kind::Content | Kind::Points) {
        return Err("Solver requires Content or Points".into());
    }
    let mut previous = Node::new("fold.motion.solver_state")?;
    previous.settings = serde_json::json!({"value_type":kind});
    previous.position = [0., 0.];
    let mut out = Node::new("fold.motion.group_output")?;
    out.settings = serde_json::json!({"value_type":kind});
    out.connect("value", previous.id, "state");
    out.position = [600., 0.];
    let link = Link {
        node: out.id,
        socket: "value".into(),
    };
    let group = ObjectId::new();
    m.groups.insert(
        group,
        GroupDefinition {
            name: "Solver Update".into(),
            version: 1,
            inputs: vec![],
            outputs: BTreeMap::from([("state".into(), link.clone())]),
            graph: Graph {
                nodes: vec![previous, out],
                output: link,
            },
        },
    );
    let mut solver = Node::new("fold.motion.solver")?;
    solver.settings = serde_json::to_value(crate::nodes::solver::Settings {
        group: Some(group),
        state_kind: kind,
        ..Default::default()
    })
    .unwrap();
    let id = solver.id;
    m.graph.nodes.push(solver);
    Ok(id)
}

/// Retype a newly created identity solver atomically. Once authored, its graph
/// owns its types; a fresh solver avoids silently rewriting user operations.
pub fn set_state_kind(m: &mut Motion, id: ObjectId, kind: Kind) -> Result<(), String> {
    if !matches!(kind, Kind::Content | Kind::Points) {
        return Err("Solver requires Content or Points".into());
    }
    let mut candidate = m.clone();
    let settings = candidate
        .graph
        .node(id)?
        .settings::<crate::nodes::solver::Settings>()?;
    let group = candidate
        .groups
        .get_mut(&settings.group.ok_or("missing update graph")?)
        .ok_or("missing update graph")?;
    if group.graph.nodes.len() != 2
        || !group
            .graph
            .nodes
            .iter()
            .any(|n| n.kind == "fold.motion.solver_state")
    {
        return Err("State type is fixed after authoring the update graph; create a new solver for another type".into());
    }
    for n in &mut group.graph.nodes {
        n.settings["value_type"] = serde_json::to_value(kind).unwrap();
    }
    node(&mut candidate, id)?.settings["state_kind"] = serde_json::to_value(kind).unwrap();
    candidate.validate()?;
    *m = candidate;
    Ok(())
}
