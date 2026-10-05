//! A shipped editable driver, built with the same group interface as user tools.
use super::*;

pub fn attach_oscillator(m: &mut Motion, target: ObjectId, key: &str) -> Result<ObjectId, String> {
    let original = m.graph.node(target)?;
    if let Some(Input::Link(link)) = original.inputs.get(key) {
        return Ok(link.node);
    }
    if original
        .animation
        .keys()
        .any(|path| path.rsplit_once('.').is_some_and(|(p, _)| p == key))
    {
        return Err("This property has keyframes. Remove its animation before replacing it with an Oscillator.".into());
    }
    let base = match original.inputs.get(key) {
        Some(Input::Value(v)) => v.clone(),
        _ => return Err("Choose a numeric property".into()),
    };
    let (_, values) = crate::animation::parameters::components(&base)
        .ok_or("Oscillator requires a number, vector or color")?;
    let normalized = m
        .signature(original)?
        .0
        .iter()
        .any(|s| s.id == key && s.unit == "0–1");
    let mut minimum = base.clone();
    let mut maximum = base.clone();
    let ranges: Vec<_> = values
        .iter()
        .enumerate()
        .map(|(i, &v)| {
            if normalized {
                [0., 1.]
            } else if matches!(key, "count" | "columns" | "rows") {
                [0., v.max(1.)]
            } else if key.contains("scale") {
                [v * 0.8, v * 1.2]
            } else if key.contains("opacity") {
                [0., 1.]
            } else if matches!(base, Datum::Color(_)) {
                if i == 3 {
                    [v, v]
                } else {
                    [(v * 0.8).clamp(0., 1.), (v * 1.2).clamp(0., 1.)]
                }
            } else if v == 0. && matches!(key, "width" | "height" | "radius" | "length" | "size") {
                [0., 1.]
            } else {
                let span = if v == 0. { 45. } else { v.abs() * 0.2 };
                [v - span, v + span]
            }
        })
        .collect();
    crate::animation::parameters::set_components(
        &mut minimum,
        &ranges.iter().map(|v| v[0]).collect::<Vec<_>>(),
    );
    crate::animation::parameters::set_components(
        &mut maximum,
        &ranges.iter().map(|v| v[1]).collect::<Vec<_>>(),
    );
    let mut proposal = m.clone();
    let mut oscillator = Node::new("fold.motion.oscillator")?;
    oscillator.settings["value_type"] = serde_json::to_value(base.kind()).unwrap();
    oscillator.set("minimum", minimum);
    oscillator.set("maximum", maximum);
    let mut time = Node::new("fold.motion.time")?;
    time.position = [0., 0.];
    oscillator.connect("time", time.id, "value");
    let mut nodes = vec![time];
    let mut inputs = vec![];
    for socket in proposal.signature(&oscillator)?.0 {
        if socket.id == "time" {
            continue;
        }
        let value = match oscillator.inputs.get(&socket.id) {
            Some(Input::Value(v)) => Some(v.clone()),
            _ => socket.default.clone(),
        };
        let name = match socket.id.as_str() {
            "duration" => "Cycle duration",
            "shape" => "Shape",
            "minimum" => "Minimum",
            "maximum" => "Maximum",
            "phase" => "Phase",
            "time_mode" => "Timing",
            "bpm" => "Tempo",
            "time_scale" => "Time scale",
            "time_offset" => "Time offset",
            "phase_spread" => "Phase spread",
            "channel_phase" => "Channel offset",
            "strength" => "Strength",
            _ => &socket.id,
        };
        let input = group_input(
            &mut nodes,
            &mut inputs,
            &socket.id,
            name,
            socket.kind,
            value,
        )?;
        let port = inputs.last_mut().unwrap();
        port.unit = socket.unit.into();
        if matches!(
            socket.id.as_str(),
            "time_scale" | "time_offset" | "phase_spread" | "channel_phase" | "strength"
        ) {
            port.section = "Advanced".into();
            port.advanced = true;
        }
        oscillator.connect(&socket.id, input, "value");
    }
    let oscillator_id = oscillator.id;
    oscillator.position = [320., 0.];
    let mut output = Node::new("fold.motion.group_output")?;
    output.settings = serde_json::json!({"value_type":base.kind()});
    output.connect("value", oscillator.id, "value");
    output.position = [640., 0.];
    let link = Link {
        node: output.id,
        socket: "value".into(),
    };
    nodes.extend([oscillator, output]);
    let group = ObjectId::new();
    let mut driver = Node::new("fold.motion.group_instance")?;
    driver.settings = serde_json::json!({"group":group,"asset":"fold.motion.oscillator","oscillator_node":oscillator_id});
    driver.position = [original.position[0] - 360., original.position[1] + 180.];
    for port in &inputs {
        if let Some(v) = &port.default {
            driver.set(&port.id, v.clone());
        }
    }
    let id = driver.id;
    proposal.groups.insert(
        group,
        GroupDefinition {
            name: "Oscillator".into(),
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
    node(&mut proposal, target)?.connect(key, id, "value");
    proposal.validate()?;
    *m = proposal;
    Ok(id)
}
