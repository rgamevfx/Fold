//! Convenience wiring builds a regular editable ramp group and a separate driver.
use super::*;
use crate::nodes::color_ramp::{Settings, stop};
pub fn attach_color_ramp(m: &mut Motion, target: ObjectId, key: &str) -> Result<ObjectId, String> {
    let original = m.graph.node(target)?;
    if let Some(Input::Link(link)) = original.inputs.get(key) {
        return Ok(link.node);
    }
    if original
        .animation
        .keys()
        .any(|p| p.rsplit_once('.').is_some_and(|(k, _)| k == key))
    {
        return Err(
            "Remove this property's animation before replacing it with a Color Ramp".into(),
        );
    }
    let base = match original.inputs.get(key) {
        Some(Input::Value(Datum::Color(v))) => *v,
        _ => return Err("Choose a color property".into()),
    };
    let position = original.position;
    let copies = is_duplicator(m, target);
    let mut proposal = m.clone();
    let driver = crate::authoring::add_node(&mut proposal, "fold.motion.color_ramp")?;
    let settings = Settings {
        stops: vec![stop(0., base), stop(1., [0.1, 0.12, 0.85, base[3]])],
        ..Default::default()
    };
    settings.validate()?;
    node(&mut proposal, driver)?.settings = serde_json::to_value(settings).unwrap();
    crate::authoring::group_node(&mut proposal, driver)?;
    let ramp = node(&mut proposal, driver)?;
    ramp.settings["asset"] = "fold.motion.color_ramp".into();
    ramp.settings["ramp_node"] = serde_json::to_value(driver).unwrap();
    ramp.position = [position[0] - 360., position[1] + 420.];
    ramp.extensions.insert(
        "fold.motion.input_visibility".into(),
        serde_json::json!({"value":true}),
    );
    if copies {
        let progress = crate::authoring::add_node(&mut proposal, "fold.motion.copy_progress")?;
        node(&mut proposal, progress)?.position = [position[0] - 680., position[1] + 420.];
        node(&mut proposal, driver)?.connect("value", progress, "value");
        if key == "color" {
            node(&mut proposal, target)?.set("color_enabled", Datum::Bool(true));
        }
    }
    node(&mut proposal, target)?.connect(key, driver, "value");
    proposal.validate()?;
    *m = proposal;
    Ok(driver)
}
/// Update the editable definition while retaining future/unknown node settings.
pub fn set_color_ramp(m: &mut Motion, driver: ObjectId, settings: Settings) -> Result<(), String> {
    settings.validate()?;
    let instance = m.graph.node(driver)?;
    let group = instance
        .settings::<crate::nodes::interface::GroupSettings>()?
        .group
        .ok_or("Missing ramp group")?;
    let inner: ObjectId = serde_json::from_value(instance.settings["ramp_node"].clone())
        .map_err(|e| e.to_string())?;
    let node = m
        .groups
        .get_mut(&group)
        .ok_or("Missing ramp definition")?
        .graph
        .nodes
        .iter_mut()
        .find(|n| n.id == inner && n.kind == "fold.motion.color_ramp")
        .ok_or("Missing ramp node")?;
    node.settings["stops"] = serde_json::to_value(settings.stops).unwrap();
    node.settings["interpolation"] = serde_json::to_value(settings.interpolation).unwrap();
    Ok(())
}
