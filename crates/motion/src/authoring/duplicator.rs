//! Distribution consumes independent value drivers; convenience actions wire them.
use super::*;

pub fn is_duplicator(m: &Motion, id: ObjectId) -> bool {
    m.graph.node(id).is_ok_and(|n| {
        n.settings.get("asset").and_then(|v| v.as_str()) == Some("fold.motion.duplicator")
            && n.inputs.contains_key("offset_y")
    })
}
pub fn copy_driver(m: &Motion, id: ObjectId, port: &str) -> Option<ObjectId> {
    match m.graph.node(id).ok()?.inputs.get(port)? {
        Input::Link(l) => Some(l.node),
        _ => None,
    }
}
pub fn attach_copy_wave(m: &mut Motion, id: ObjectId) -> Result<ObjectId, String> {
    if !is_duplicator(m, id) {
        return Err("Select a Duplicator with per-copy inputs".into());
    }
    if let Some(driver) = copy_driver(m, id, "offset_y") {
        return Ok(driver);
    }
    let mut proposal = m.clone();
    let mut nodes = vec![];
    let mut inputs = vec![];
    let time = Node::new("fold.motion.time")?;
    let mut wave = Node::new("fold.motion.wave")?;
    wave.connect("coordinate", time.id, "value");
    for (key, label, value, unit) in [
        ("amplitude", "Amplitude", 45., ""),
        ("frequency", "Speed", 0.75, "Hz"),
        ("phase", "Phase", 0., "cycles"),
        ("offset", "Center", 0., ""),
    ] {
        let input = group_input(
            &mut nodes,
            &mut inputs,
            key,
            label,
            Kind::Scalar,
            Some(Datum::Scalar(value)),
        )?;
        inputs.last_mut().unwrap().unit = unit.into();
        wave.connect(key, input, "value");
    }
    let mut output = Node::new("fold.motion.group_output")?;
    output.settings = serde_json::json!({"value_type":"Scalar"});
    output.connect("value", wave.id, "value");
    let link = Link {
        node: output.id,
        socket: "value".into(),
    };
    nodes.extend([time, wave, output]);
    for (i, n) in nodes.iter_mut().enumerate() {
        n.position = [(i % 3) as f32 * 320., (i / 3) as f32 * 220.];
    }
    let group = ObjectId::new();
    let mut driver = Node::new("fold.motion.group_instance")?;
    driver.settings = serde_json::json!({"group":group});
    driver.position = [-360., 240.];
    for port in &inputs {
        driver.set(&port.id, port.default.clone().unwrap());
    }
    let driver_id = driver.id;
    proposal.groups.insert(
        group,
        GroupDefinition {
            name: "Wave".into(),
            version: 1,
            inputs,
            outputs: BTreeMap::from([("value".into(), link.clone())]),
            graph: Graph {
                nodes,
                output: link,
            },
        },
    );
    proposal.graph.nodes.push(driver);
    node(&mut proposal, id)?.connect("offset_y", driver_id, "value");
    proposal.validate()?;
    *m = proposal;
    Ok(driver_id)
}
pub fn attach_copy_palette(m: &mut Motion, id: ObjectId) -> Result<ObjectId, String> {
    if !is_duplicator(m, id) {
        return Err("Select a Duplicator with per-copy inputs".into());
    }
    if let Some(driver) = copy_driver(m, id, "color") {
        node(m, id)?.set("color_enabled", Datum::Bool(true));
        return Ok(driver);
    }
    let mut proposal = m.clone();
    let driver = crate::authoring::add_node(&mut proposal, "fold.motion.palette")?;
    crate::authoring::group_node(&mut proposal, driver)?;
    let group = node(&mut proposal, driver)?
        .settings::<crate::nodes::interface::GroupSettings>()?
        .group
        .unwrap();
    for port in &mut proposal.groups.get_mut(&group).unwrap().inputs {
        port.name = if port.id == "start" {
            "Start color"
        } else {
            "End color"
        }
        .into();
    }
    node(&mut proposal, driver)?.position = [-360., 500.];
    node(&mut proposal, id)?.connect("color", driver, "value");
    node(&mut proposal, id)?.set("color_enabled", Datum::Bool(true));
    proposal.validate()?;
    *m = proposal;
    Ok(driver)
}
pub fn duplicate(m: &mut Motion, source: ObjectId) -> Result<ObjectId, String> {
    let mut m2 = m.clone();
    let source_object = m2
        .scene
        .as_ref()
        .and_then(|s| s.objects.iter().find(|o| o.id == source))
        .ok_or("Choose a source object")?
        .clone();
    let placement = source_object
        .transform
        .and_then(|id| m2.graph.node(id).ok())
        .and_then(|n| n.inputs.get("translation"))
        .cloned();
    let id = super::super::create_procedural(&mut m2)?;
    let group = node(&mut m2, id)?
        .settings::<crate::nodes::interface::GroupSettings>()?
        .group
        .unwrap();
    let mut nodes = vec![];
    let mut inputs = vec![];
    let mut sockets = BTreeMap::new();
    let source_input = group_input(
        &mut nodes,
        &mut inputs,
        "source",
        "Source",
        Kind::Content,
        None,
    )?;
    for (key, label, value, section, unit) in [
        (
            "count",
            "Copies",
            Datum::Scalar(32.),
            "Distribution",
            "integer",
        ),
        (
            "spacing",
            "Spacing",
            Datum::Vector([8., 0.]),
            "Distribution",
            "px",
        ),
        (
            "offset_x",
            "X offset",
            Datum::Scalar(0.),
            "Per-copy transform",
            "px",
        ),
        (
            "offset_y",
            "Y offset",
            Datum::Scalar(0.),
            "Per-copy transform",
            "px",
        ),
        (
            "rotation",
            "Rotation",
            Datum::Scalar(0.),
            "Per-copy transform",
            "degrees",
        ),
        (
            "scale",
            "Scale",
            Datum::Vector([1.; 2]),
            "Per-copy transform",
            "factor",
        ),
        (
            "stagger",
            "Stagger",
            Datum::Scalar(0.025),
            "Per-copy transform",
            "seconds / copy",
        ),
        (
            "color_enabled",
            "Apply color",
            Datum::Bool(false),
            "Per-copy color",
            "",
        ),
        (
            "color",
            "Color",
            Datum::Color([1.; 4]),
            "Per-copy color",
            "",
        ),
        (
            "shapes_only",
            "Preserve text color",
            Datum::Bool(true),
            "Per-copy color",
            "",
        ),
    ] {
        let input = group_input(
            &mut nodes,
            &mut inputs,
            key,
            label,
            value.kind(),
            Some(value),
        )?;
        let port = inputs.last_mut().unwrap();
        port.section = section.into();
        port.unit = unit.into();
        sockets.insert(key, input);
    }
    let mut points = Node::new("fold.motion.linear_points")?;
    points.connect("count", sockets["count"], "value");
    points.connect("spacing", sockets["spacing"], "value");
    let mut copies = Node::new("fold.motion.instance_on_points")?;
    copies.connect("source", source_input, "value");
    copies.connect("points", points.id, "points");
    let mut xy = Node::new("fold.motion.combine_xy")?;
    xy.connect("x", sockets["offset_x"], "value");
    xy.connect("y", sockets["offset_y"], "value");
    let mut output_link = Link {
        node: copies.id,
        socket: "content".into(),
    };
    let mut responses = vec![];
    for (kind, key, source) in [
        ("fold.motion.set_position", "value", xy.id),
        ("fold.motion.set_rotation", "value", sockets["rotation"]),
        ("fold.motion.set_scale", "value", sockets["scale"]),
        ("fold.motion.apply_copy_color", "color", sockets["color"]),
    ] {
        let mut delay = Node::new("fold.motion.stagger")?;
        delay.settings = serde_json::json!({"value_type":match kind {"fold.motion.set_position"|"fold.motion.set_scale"=>Kind::Vector,"fold.motion.apply_copy_color"=>Kind::Color,_=>Kind::Scalar}});
        delay.connect("value", source, "value");
        delay.connect("delay", sockets["stagger"], "value");
        let mut response = Node::new(kind)?;
        response
            .inputs
            .insert("content".into(), Input::Link(output_link));
        response.connect(key, delay.id, "value");
        if key == "color" {
            response.connect("enabled", sockets["color_enabled"], "value");
            response.connect("shapes_only", sockets["shapes_only"], "value");
        }
        output_link = Link {
            node: response.id,
            socket: "content".into(),
        };
        delay.position = [responses.len() as f32 * 180., 500.];
        response.position = [responses.len() as f32 * 180., 0.];
        responses.extend([delay, response]);
    }
    for (i, n) in nodes.iter_mut().enumerate() {
        n.position = [-600., i as f32 * 140.];
    }
    points.position = [-280., 0.];
    copies.position = [-280., 260.];
    xy.position = [-280., 550.];
    let mut out = Node::new("fold.motion.group_output")?;
    out.settings = serde_json::json!({"value_type":"Content"});
    out.inputs.insert("value".into(), Input::Link(output_link));
    out.position = [1500., 0.];
    let output = Link {
        node: out.id,
        socket: "value".into(),
    };
    nodes.extend([points, copies, xy, out]);
    nodes.extend(responses);
    for port in &inputs {
        if let Some(value) = &port.default {
            node(&mut m2, id)?.set(&port.id, value.clone());
        }
    }
    m2.groups.insert(
        group,
        GroupDefinition {
            name: "Duplicator".into(),
            version: 1,
            inputs,
            outputs: BTreeMap::from([("content".into(), output.clone())]),
            graph: Graph { nodes, output },
        },
    );
    node(&mut m2, id)?.settings["asset"] = "fold.motion.duplicator".into();
    set_reference(&mut m2, id, "source", source, true)?;
    let obj = m2
        .scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == id)
        .unwrap();
    obj.name = "Duplicator".into();
    let tr = obj.transform.unwrap();
    if let Some(placement) = placement {
        node(&mut m2, tr)?
            .inputs
            .insert("translation".into(), placement);
    }
    node(&mut m2, id)?.position = [0., 0.];
    node(&mut m2, tr)?.position = [380., 0.];
    m2.scene
        .as_mut()
        .unwrap()
        .objects
        .iter_mut()
        .find(|o| o.id == source)
        .unwrap()
        .visible = false;
    m2.validate()?;
    *m = m2;
    Ok(id)
}
