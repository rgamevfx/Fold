use super::*;
use fold_foundation::Time;
pub fn keyframe_input(
    motion: &mut Motion,
    target: ObjectId,
    socket: &str,
    time: Time,
) -> Result<ObjectId, String> {
    let value = match motion.graph.node(target)?.inputs.get(socket) {
        Some(Input::Value(v))
            if matches!(v, Datum::Scalar(_) | Datum::Vector(_) | Datum::Color(_)) =>
        {
            v.clone()
        }
        _ => {
            return Err("select an unconnected numeric input to animate its authored value".into());
        }
    };
    let id = add_node(motion, "fold.motion.keyframes")?;
    node(motion, id)?.settings = serde_json::to_value(crate::animation::Track {
        channels: vec![],
        keys: vec![crate::animation::Key {
            time,
            value,
            interpolation: crate::animation::Interpolation::Linear,
        }],
    })
    .map_err(|e| e.to_string())?;
    node(motion, target)?.connect(socket, id, "value");
    Ok(id)
}
pub fn expose_input(
    motion: &mut Motion,
    target: ObjectId,
    socket: &str,
) -> Result<ObjectId, String> {
    let value = match motion.graph.node(target)?.inputs.get(socket) {
        Some(Input::Value(v)) => v.clone(),
        _ => return Err("select an unconnected input to publish".into()),
    };
    if !matches!(
        value,
        Datum::Scalar(_) | Datum::Vector(_) | Datum::Color(_) | Datum::Bool(_)
    ) {
        return Err("unsupported published control type".into());
    }
    let id = add_node(motion, "fold.motion.published_control")?;
    node(motion, id)?.settings["value_type"] = serde_json::to_value(value.kind()).unwrap();
    if let Datum::Scalar(max) = value {
        node(motion, id)?.set("default", Datum::Scalar(1.));
        let map = add_node(motion, "fold.motion.map_range")?;
        node(motion, map)?.connect("value", id, "value");
        node(motion, map)?.set("target_max", Datum::Scalar(max));
        node(motion, target)?.connect(socket, map, "value");
    } else {
        node(motion, id)?.set("default", value);
        node(motion, target)?.connect(socket, id, "value");
    }
    Ok(id)
}
