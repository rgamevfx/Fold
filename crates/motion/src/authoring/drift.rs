//! A reusable drifting-shapes preset, composed entirely from editable nodes.
//! This module authors a graph; it adds no evaluation behavior.
use crate::{
    Motion,
    authoring::{self as a, node},
    fields::{Datum, Kind},
    graph::{Link, Node},
    nodes::interface::{GroupDefinition, Port},
};
use fold_foundation::ObjectId;
use std::collections::BTreeMap;
fn add(m: &mut Motion, kind: &str) -> Result<ObjectId, String> {
    a::add_node(m, &format!("fold.motion.{kind}"))
}
fn wire(
    m: &mut Motion,
    to: ObjectId,
    input: &str,
    from: ObjectId,
    output: &str,
) -> Result<(), String> {
    node(m, to)?.connect(input, from, output);
    Ok(())
}
fn port(
    m: &mut Motion,
    ports: &mut Vec<Port>,
    key: &str,
    name: &str,
    kind: Kind,
    default: Option<Datum>,
    unit: &str,
) -> Result<ObjectId, String> {
    let id = add(m, "group_input")?;
    node(m, id)?.settings = serde_json::json!({"key":key,"value_type":kind});
    ports.push(Port {
        id: key.into(),
        name: name.into(),
        kind,
        default,
        unit: unit.into(),
        section: String::new(),
        advanced: false,
    });
    Ok(id)
}
fn math(m: &mut Motion, op: &str, kind: Kind) -> Result<ObjectId, String> {
    let id = add(m, "math")?;
    node(m, id)?.settings = serde_json::json!({"operation":op,"value_type":kind});
    if kind == Kind::Vector {
        node(m, id)?.set("a", Datum::Vector([0.; 2]));
        node(m, id)?.set("b", Datum::Vector([0.; 2]));
    }
    Ok(id)
}
fn random(m: &mut Motion, id: ObjectId, seed: f64, min: f64, max: f64) -> Result<ObjectId, String> {
    let n = add(m, "random_value")?;
    wire(m, n, "id", id, "value")?;
    node(m, n)?.set("seed", Datum::Scalar(seed));
    node(m, n)?.set("min", Datum::Scalar(min));
    node(m, n)?.set("max", Datum::Scalar(max));
    Ok(n)
}
fn response(m: &mut Motion, combine: &str) -> Result<ObjectId, String> {
    let n = add(m, "set_position")?;
    node(m, n)?.settings = serde_json::json!({"domain":"Instances","combine":combine});
    Ok(n)
}

/// Source remains a live content port. All distribution, movement and styling
/// controls are published by this group, not built into the Solver node.
pub fn create(m: &mut Motion) -> Result<ObjectId, String> {
    let mut build = Motion::empty();
    build.graph.nodes.clear();
    let mut ports = vec![];
    let source = port(
        &mut build,
        &mut ports,
        "source",
        "Source",
        Kind::Content,
        None,
        "",
    )?;
    let count = port(
        &mut build,
        &mut ports,
        "count",
        "Count",
        Kind::Scalar,
        Some(Datum::Scalar(24.)),
        "integer",
    )?;
    let bounds = port(
        &mut build,
        &mut ports,
        "bounds",
        "Bounds",
        Kind::Vector,
        Some(Datum::Vector([360., 640.])),
        "px",
    )?;
    let speed = port(
        &mut build,
        &mut ports,
        "speed",
        "Speed",
        Kind::Scalar,
        Some(Datum::Scalar(80.)),
        "px/s",
    )?;
    let seed = port(
        &mut build,
        &mut ports,
        "seed",
        "Seed",
        Kind::Scalar,
        Some(Datum::Scalar(8.)),
        "integer",
    )?;
    let turn = port(
        &mut build,
        &mut ports,
        "turn_interval",
        "Turn interval",
        Kind::Scalar,
        Some(Datum::Scalar(1.2)),
        "seconds",
    )?;
    let angle = port(
        &mut build,
        &mut ports,
        "angle_step",
        "Angle step",
        Kind::Scalar,
        Some(Datum::Scalar(45.)),
        "degrees",
    )?;
    let trail = port(
        &mut build,
        &mut ports,
        "trail_samples",
        "Trail samples",
        Kind::Scalar,
        Some(Datum::Scalar(5.)),
        "integer",
    )?;
    let fade = port(
        &mut build,
        &mut ports,
        "fade",
        "Trail fade",
        Kind::Scalar,
        Some(Datum::Scalar(0.65)),
        "factor",
    )?;
    let sweep_enabled = port(
        &mut build,
        &mut ports,
        "sweep_enabled",
        "Convex sweep",
        Kind::Bool,
        Some(Datum::Bool(false)),
        "",
    )?;
    let stretch = port(
        &mut build,
        &mut ports,
        "stretch",
        "Sweep duration",
        Kind::Scalar,
        Some(Datum::Scalar(0.15)),
        "seconds",
    )?;
    let opacity_min = port(
        &mut build,
        &mut ports,
        "opacity_min",
        "Min opacity",
        Kind::Scalar,
        Some(Datum::Scalar(0.12)),
        "factor",
    )?;
    let opacity_max = port(
        &mut build,
        &mut ports,
        "opacity_max",
        "Max opacity",
        Kind::Scalar,
        Some(Datum::Scalar(1.)),
        "factor",
    )?;
    let points = add(&mut build, "linear_points")?;
    wire(&mut build, points, "count", count, "value")?;
    let copies = add(&mut build, "instance_on_points")?;
    wire(&mut build, copies, "source", source, "value")?;
    wire(&mut build, copies, "points", points, "points")?;
    let id = add(&mut build, "element_id")?;
    let x = random(&mut build, id, 1., 0., 1.)?;
    let y = random(&mut build, id, 2., 0., 1.)?;
    wire(&mut build, x, "seed", seed, "value")?;
    wire(&mut build, y, "seed", seed, "value")?;
    let xy = add(&mut build, "combine_xy")?;
    wire(&mut build, xy, "x", x, "value")?;
    wire(&mut build, xy, "y", y, "value")?;
    let scale = math(&mut build, "Multiply", Kind::Vector)?;
    wire(&mut build, scale, "a", xy, "value")?;
    wire(&mut build, scale, "b", bounds, "value")?;
    let position = response(&mut build, "Replace")?;
    wire(&mut build, position, "content", copies, "content")?;
    wire(&mut build, position, "value", scale, "value")?;
    let variation = random(&mut build, id, 3., 0.12, 1.)?;
    wire(&mut build, variation, "seed", seed, "value")?;
    wire(&mut build, variation, "min", opacity_min, "value")?;
    wire(&mut build, variation, "max", opacity_max, "value")?;
    let opacity = add(&mut build, "set_opacity")?;
    wire(&mut build, opacity, "content", position, "content")?;
    wire(&mut build, opacity, "value", variation, "value")?;
    let solver = a::solver::create(&mut build, Kind::Content)?;
    wire(&mut build, solver, "initial", opacity, "content")?;
    let update_id = node(&mut build, solver)?
        .settings::<crate::nodes::solver::Settings>()?
        .group
        .unwrap();
    let mut update = Motion::empty();
    let group = build.groups.remove(&update_id).unwrap();
    update.graph = group.graph;
    let previous = update.graph.nodes[0].id;
    let out = update.graph.nodes[1].id;
    let mut update_ports = vec![];
    let mut inputs = BTreeMap::new();
    for (key, outer) in [
        ("bounds", bounds),
        ("speed", speed),
        ("seed", seed),
        ("turn_interval", turn),
        ("angle_step", angle),
    ] {
        let p = ports.iter().find(|p| p.id == key).unwrap();
        inputs.insert(
            key,
            port(
                &mut update,
                &mut update_ports,
                key,
                &p.name,
                p.kind,
                p.default.clone(),
                &p.unit,
            )?,
        );
        wire(&mut build, solver, key, outer, "value")?;
    }
    let time = add(&mut update, "time")?;
    let epoch = math(&mut update, "Divide", Kind::Scalar)?;
    wire(&mut update, epoch, "a", time, "value")?;
    wire(&mut update, epoch, "b", inputs["turn_interval"], "value")?;
    let floor = math(&mut update, "Floor", Kind::Scalar)?;
    wire(&mut update, floor, "a", epoch, "value")?;
    let noise_seed = math(&mut update, "Add", Kind::Scalar)?;
    wire(&mut update, noise_seed, "a", floor, "value")?;
    wire(&mut update, noise_seed, "b", inputs["seed"], "value")?;
    let id = add(&mut update, "element_id")?;
    let angle = random(&mut update, id, 0., 0., 360.)?;
    wire(&mut update, angle, "seed", noise_seed, "value")?;
    let divisions = math(&mut update, "Divide", Kind::Scalar)?;
    wire(&mut update, divisions, "a", angle, "value")?;
    wire(&mut update, divisions, "b", inputs["angle_step"], "value")?;
    let round = math(&mut update, "Round", Kind::Scalar)?;
    wire(&mut update, round, "a", divisions, "value")?;
    let heading = math(&mut update, "Multiply", Kind::Scalar)?;
    wire(&mut update, heading, "a", round, "value")?;
    wire(&mut update, heading, "b", inputs["angle_step"], "value")?;
    let direction = add(&mut update, "direction")?;
    wire(&mut update, direction, "angle", heading, "value")?;
    wire(&mut update, direction, "length", inputs["speed"], "value")?;
    let store = add(&mut update, "store_attribute")?;
    node(&mut update, store)?.settings =
        serde_json::json!({"geometry_kind":"Content","value_type":"Vector"});
    node(&mut update, store)?.set("name", Datum::Text("velocity".into()));
    wire(&mut update, store, "content", previous, "state")?;
    wire(&mut update, store, "value", direction, "value")?;
    let dt = add(&mut update, "combine_xy")?;
    wire(&mut update, dt, "x", previous, "delta_time")?;
    wire(&mut update, dt, "y", previous, "delta_time")?;
    let displacement = math(&mut update, "Multiply", Kind::Vector)?;
    wire(&mut update, displacement, "a", direction, "value")?;
    wire(&mut update, displacement, "b", dt, "value")?;
    let move_node = response(&mut update, "Add")?;
    wire(&mut update, move_node, "content", store, "content")?;
    wire(&mut update, move_node, "value", displacement, "value")?;
    let current = add(&mut update, "position")?;
    let wrap = math(&mut update, "Modulo", Kind::Vector)?;
    wire(&mut update, wrap, "a", current, "value")?;
    wire(&mut update, wrap, "b", inputs["bounds"], "value")?;
    let wrapped = response(&mut update, "Replace")?;
    wire(&mut update, wrapped, "content", move_node, "content")?;
    wire(&mut update, wrapped, "value", wrap, "value")?;
    wire(&mut update, out, "value", wrapped, "content")?;
    build.groups.insert(
        update_id,
        GroupDefinition {
            name: "Drift Update".into(),
            version: 1,
            inputs: update_ports,
            outputs: group.outputs,
            graph: update.graph,
        },
    );
    let velocity = add(&mut build, "read_attribute")?;
    node(&mut build, velocity)?.settings =
        serde_json::json!({"geometry_kind":"Content","value_type":"Vector"});
    node(&mut build, velocity)?.set("name", Datum::Text("velocity".into()));
    wire(&mut build, velocity, "content", solver, "content")?;
    let seconds = add(&mut build, "combine_xy")?;
    wire(&mut build, seconds, "x", stretch, "value")?;
    wire(&mut build, seconds, "y", stretch, "value")?;
    let offset = math(&mut build, "Multiply", Kind::Vector)?;
    wire(&mut build, offset, "a", velocity, "value")?;
    wire(&mut build, offset, "b", seconds, "value")?;
    let negate = math(&mut build, "Negate", Kind::Vector)?;
    wire(&mut build, negate, "a", offset, "value")?;
    let sweep = add(&mut build, "convex_sweep")?;
    wire(&mut build, sweep, "enabled", sweep_enabled, "value")?;
    wire(&mut build, sweep, "content", solver, "content")?;
    wire(&mut build, sweep, "offset", negate, "value")?;
    let history = add(&mut build, "history")?;
    wire(&mut build, history, "content", sweep, "content")?;
    wire(&mut build, history, "samples", trail, "value")?;
    wire(&mut build, history, "fade", fade, "value")?;
    let out = add(&mut build, "group_output")?;
    node(&mut build, out)?.settings = serde_json::json!({"value_type":"Content"});
    wire(&mut build, out, "value", history, "content")?;
    let link = Link {
        node: out,
        socket: "value".into(),
    };
    build.graph.output = link.clone();
    for p in &mut ports {
        p.section = match p.id.as_str() {
            "source" | "count" | "bounds" | "seed" => "Distribution",
            "speed" | "turn_interval" | "angle_step" => "Motion",
            "opacity_min" | "opacity_max" => "Appearance",
            _ => "Trails",
        }
        .into();
    }
    ports.sort_by_key(|p| match p.section.as_str() {
        "Distribution" => 0,
        "Motion" => 1,
        "Appearance" => 2,
        _ => 3,
    });
    let definition = GroupDefinition {
        name: "Drifting Shapes".into(),
        version: 1,
        inputs: ports,
        outputs: BTreeMap::from([("content".into(), link)]),
        graph: build.graph,
    };
    let group = ObjectId::new();
    m.groups.extend(build.groups);
    let defaults: Vec<_> = definition
        .inputs
        .iter()
        .filter_map(|p| p.default.clone().map(|v| (p.id.clone(), v)))
        .collect();
    m.groups.insert(group, definition);
    let mut instance = Node::new("fold.motion.group_instance")?;
    instance.settings = serde_json::json!({"group":group});
    for (key, value) in defaults {
        instance.set(&key, value);
    }
    let id = instance.id;
    m.graph.nodes.push(instance);
    Ok(id)
}

/// Artist action: selected source → an editable drifting-shapes object.
/// Callers commit the candidate through the normal project transaction.
pub fn from_object(m: &mut Motion, source: ObjectId) -> Result<ObjectId, String> {
    let mut candidate = m.clone();
    if !candidate
        .scene
        .as_ref()
        .is_some_and(|s| s.objects.iter().any(|o| o.id == source))
    {
        return Err("Choose a source object".into());
    }
    let instance = create(&mut candidate)?;
    let owner = a::scene::create_procedural(&mut candidate)?;
    let obsolete = node(&mut candidate, owner)?.settings["group"].clone();
    let mut construction = candidate.graph.node(instance)?.clone();
    construction.id = owner;
    *node(&mut candidate, owner)? = construction;
    candidate.graph.nodes.retain(|n| n.id != instance);
    if let Ok(id) = serde_json::from_value::<ObjectId>(obsolete) {
        candidate.groups.remove(&id);
    }
    let bounds = [candidate.info.width as f64, candidate.info.height as f64];
    node(&mut candidate, owner)?.set("bounds", Datum::Vector(bounds));
    a::scene::assets::set_reference(&mut candidate, owner, "source", source, true)?;
    for object in &mut candidate.scene.as_mut().unwrap().objects {
        if object.id == owner {
            object.name = "Drifting Shapes".into();
        }
        if object.id == source {
            object.visible = false;
        }
    }
    candidate.validate()?;
    *m = candidate;
    Ok(owner)
}
