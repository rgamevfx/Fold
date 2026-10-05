//! Shipped particle setup, assembled from the same editable public nodes.
use crate::{
    Motion,
    authoring::{self as a, node},
    fields::{Datum, Kind},
    graph::{Graph, Link, Node},
    nodes::interface::{GroupDefinition, Port},
};
use fold_foundation::{ObjectId, Time};
use std::collections::BTreeMap;

fn port(
    nodes: &mut Vec<Node>,
    ports: &mut Vec<Port>,
    key: &str,
    name: &str,
    kind: Kind,
    default: Option<Datum>,
    section: &str,
    unit: &str,
) -> Result<ObjectId, String> {
    let mut n = Node::new("fold.motion.group_input")?;
    n.settings = serde_json::json!({"key":key,"value_type":kind});
    n.position = [-400., nodes.len() as f32 * 130.];
    let id = n.id;
    nodes.push(n);
    ports.push(Port {
        id: key.into(),
        name: name.into(),
        kind,
        default,
        section: section.into(),
        unit: unit.into(),
        advanced: false,
    });
    Ok(id)
}
fn definition(
    name: &str,
    mut nodes: Vec<Node>,
    inputs: Vec<Port>,
    source: ObjectId,
    socket: &str,
    kind: Kind,
    output: &str,
) -> Result<GroupDefinition, String> {
    let mut out = Node::new("fold.motion.group_output")?;
    out.settings = serde_json::json!({"value_type":kind});
    out.connect("value", source, socket);
    out.position = [1400., 0.];
    let link = Link {
        node: out.id,
        socket: "value".into(),
    };
    nodes.push(out);
    Ok(GroupDefinition {
        name: name.into(),
        version: 1,
        inputs,
        outputs: BTreeMap::from([(output.into(), link.clone())]),
        graph: Graph {
            nodes,
            output: link,
        },
    })
}

/// A group with explicit initial Points and attractor Content inputs. No text,
/// distribution or reference-specific timing is embedded in the evaluator.
pub fn create(m: &mut Motion) -> Result<ObjectId, String> {
    let mut nodes = vec![];
    let mut ports = vec![];
    let points = port(
        &mut nodes,
        &mut ports,
        "points",
        "Initial points",
        Kind::Points,
        None,
        "Sources",
        "",
    )?;
    let attractor = port(
        &mut nodes,
        &mut ports,
        "attractor",
        "Attractor",
        Kind::Content,
        None,
        "Sources",
        "",
    )?;
    let mut targets = Node::new("fold.motion.particle_targets")?;
    targets.connect("points", points, "value");
    targets.connect("attractor", attractor, "value");
    let seed = port(
        &mut nodes,
        &mut ports,
        "target_seed",
        "Target seed",
        Kind::Scalar,
        Some(Datum::Scalar(1.)),
        "Sources",
        "integer",
    )?;
    targets.connect("seed", seed, "value");
    targets.position = [0., 0.];
    let mut solver = Node::new("fold.motion.solver")?;
    let update_id = ObjectId::new();
    solver.settings = serde_json::to_value(crate::nodes::solver::Settings {
        group: Some(update_id),
        state_kind: Kind::Points,
        ..Default::default()
    })
    .unwrap();
    solver.connect("initial", targets.id, "points");
    solver.position = [350., 0.];
    let mut update_nodes = vec![];
    let mut update_ports = vec![];
    let mut previous = Node::new("fold.motion.solver_state")?;
    previous.settings = serde_json::json!({"value_type":"Points"});
    let mut step = Node::new("fold.motion.particle_step")?;
    step.connect("points", previous.id, "state");
    step.connect("delta_time", previous.id, "delta_time");
    step.position = [400., 0.];
    for (key, label, value, unit) in [
        ("attraction", "Attraction", Datum::Scalar(1.), "0–1"),
        ("frequency", "Response", Datum::Scalar(0.5), "Hz"),
        ("variation", "Speed variation", Datum::Scalar(0.5), "0–1"),
        ("damping", "Damping", Datum::Scalar(1.), "ratio"),
        ("turbulence", "Turbulence", Datum::Scalar(0.), "px/s²"),
        (
            "acceleration",
            "Acceleration",
            Datum::Vector([0., 0.]),
            "px/s²",
        ),
        ("seed", "Motion seed", Datum::Scalar(1.), "integer"),
    ] {
        let outer = port(
            &mut nodes,
            &mut ports,
            key,
            label,
            value.kind(),
            Some(value.clone()),
            "Motion",
            unit,
        )?;
        let inner = port(
            &mut update_nodes,
            &mut update_ports,
            key,
            label,
            value.kind(),
            Some(value),
            "Motion",
            unit,
        )?;
        solver.connect(key, outer, "value");
        step.connect(key, inner, "value");
        if matches!(key, "acceleration" | "seed") {
            ports.last_mut().unwrap().advanced = true;
        }
    }
    let step_id = step.id;
    update_nodes.extend([previous, step]);
    m.groups.insert(
        update_id,
        definition(
            "Particle Update",
            update_nodes,
            update_ports,
            step_id,
            "points",
            Kind::Points,
            "state",
        )?,
    );
    let size = port(
        &mut nodes,
        &mut ports,
        "size",
        "Particle size",
        Kind::Scalar,
        Some(Datum::Scalar(4.)),
        "Appearance",
        "px",
    )?;
    let color = port(
        &mut nodes,
        &mut ports,
        "color",
        "Color",
        Kind::Color,
        Some(Datum::Color([0.1, 0.3, 0.8, 1.])),
        "Appearance",
        "",
    )?;
    let mut dot = Node::new("fold.motion.ellipse")?;
    dot.connect("width", size, "value");
    dot.connect("height", size, "value");
    dot.position = [350., 400.];
    let mut copies = Node::new("fold.motion.instance_on_points")?;
    copies.connect("points", solver.id, "content");
    copies.connect("source", dot.id, "content");
    copies.position = [700., 0.];
    let mut tint = Node::new("fold.motion.apply_copy_color")?;
    tint.connect("content", copies.id, "content");
    tint.connect("color", color, "value");
    tint.set("enabled", Datum::Bool(true));
    tint.position = [1050., 0.];
    let tint_id = tint.id;
    nodes.extend([targets, solver, dot, copies, tint]);
    let group = ObjectId::new();
    let def = definition(
        "Attract Particles",
        nodes,
        ports,
        tint_id,
        "content",
        Kind::Content,
        "content",
    )?;
    let mut instance = Node::new("fold.motion.group_instance")?;
    instance.settings = serde_json::json!({"group":group,"asset":"fold.motion.attract_particles"});
    for p in &def.inputs {
        if let Some(v) = &p.default {
            instance.set(&p.id, v.clone());
        }
    }
    let id = instance.id;
    m.groups.insert(group, def);
    m.graph.nodes.push(instance);
    Ok(id)
}

/// Selected shape → editable particle layer, separate point source and palette.
pub fn from_object(m: &mut Motion, attractor: ObjectId) -> Result<ObjectId, String> {
    let mut candidate = m.clone();
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
    a::scene::assets::set_reference(&mut candidate, owner, "attractor", attractor, false)?;
    let scatter = a::add_node(&mut candidate, "fold.motion.scatter_points")?;
    let size = [candidate.info.width as f64, candidate.info.height as f64];
    node(&mut candidate, scatter)?.set("size", Datum::Vector(size));
    node(&mut candidate, scatter)?.set("center", Datum::Vector([size[0] * 0.5, size[1] * 0.5]));
    node(&mut candidate, scatter)?.set("perimeter", Datum::Bool(true));
    node(&mut candidate, owner)?.connect("points", scatter, "points");
    a::group_node(&mut candidate, scatter)?;
    let scatter_group = node(&mut candidate, scatter)?
        .settings::<crate::nodes::interface::GroupSettings>()?
        .group
        .unwrap();
    for port in &mut candidate.groups.get_mut(&scatter_group).unwrap().inputs {
        port.name = match port.id.as_str() {
            "count" => "Count",
            "size" => "Area size",
            "center" => "Center",
            "jitter" => "Spread",
            "perimeter" => "Along edges",
            "seed" => "Position seed",
            _ => &port.name,
        }
        .into();
    }
    node(&mut candidate, scatter)?.position = [-400., 0.];
    let palette = random_palette(&mut candidate, owner)?;
    node(&mut candidate, palette)?.position = [-400., 450.];
    let mut curve = fold_animation::Curve::default();
    curve.insert(Time::ZERO, 0.);
    curve.insert(Time::new(1, 1).unwrap(), 1.);
    node(&mut candidate, owner)?
        .animation
        .insert("attraction.0".into(), curve);
    for object in &mut candidate.scene.as_mut().unwrap().objects {
        if object.id == owner {
            object.name = "Attract Particles".into();
        }
        if object.id == attractor {
            object.visible = false;
        }
    }
    candidate.validate()?;
    *m = candidate;
    Ok(owner)
}

fn random_palette(m: &mut Motion, owner: ObjectId) -> Result<ObjectId, String> {
    use crate::nodes::color_ramp::{Interpolation, Settings, stop};
    let id = a::scene::assets::attach_color_ramp(m, owner, "color")?;
    a::scene::assets::set_color_ramp(
        m,
        id,
        Settings {
            stops: vec![
                stop(0., [0.025, 0.025, 0.025, 1.]),
                stop(0.25, [0.65, 0.8, 0.01, 1.]),
                stop(0.5, [0.25, 0.1, 0.8, 1.]),
                stop(0.75, [0.01, 0.5, 0.25, 1.]),
            ],
            interpolation: Interpolation::Stepped,
        },
    )?;
    let group = node(m, id)?.settings["group"].clone();
    let group = m
        .groups
        .get_mut(&serde_json::from_value::<ObjectId>(group).unwrap())
        .unwrap();
    group.name = "Random Palette".into();
    let input = group
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.group_input")
        .ok_or("Missing palette input")?;
    input.settings = serde_json::json!({"key":"seed","value_type":"Scalar"});
    let seed_id = input.id;
    let identity = Node::new("fold.motion.element_id")?;
    let mut random = Node::new("fold.motion.random_value")?;
    random.connect("id", identity.id, "value");
    random.connect("seed", seed_id, "value");
    group
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.kind == "fold.motion.color_ramp")
        .unwrap()
        .connect("value", random.id, "value");
    group.graph.nodes.extend([identity, random]);
    group.inputs[0].id = "seed".into();
    group.inputs[0].name = "Color seed".into();
    group.inputs[0].unit = "integer".into();
    group.inputs[0].default = Some(Datum::Scalar(1.));
    let instance = node(m, id)?;
    instance.inputs.remove("value");
    instance.set("seed", Datum::Scalar(1.));
    Ok(id)
}
