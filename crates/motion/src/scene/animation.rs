//! Stable animation paths for scene-owned controls, separate from node inputs.
use super::*;
pub type Channels = crate::animation::parameters::Channels;
pub fn sample(channels: &Channels, path: &str, base: f64, time: Time) -> f64 {
    channels
        .get(path)
        .and_then(|c| c.sample(time))
        .unwrap_or(base)
}
pub fn channel_mut<'a>(
    scene: &'a mut Scene,
    id: ObjectId,
    path: &str,
) -> Option<(&'a mut f64, &'a mut Channels)> {
    for object in &mut scene.objects {
        if object.id == id && path == "scene.opacity.0" {
            return Some((&mut object.opacity, &mut object.animation));
        }
        for c in &mut object.constraints {
            if c.id == id {
                return match path {
                    "influence.0" => Some((&mut c.influence, &mut c.animation)),
                    "progress.0" => Some((&mut c.progress, &mut c.animation)),
                    _ => None,
                };
            }
        }
        for m in &mut object.masks {
            if m.id == id {
                return match path {
                    "opacity.0" => Some((&mut m.opacity, &mut m.animation)),
                    "feather.0" => Some((&mut m.feather, &mut m.animation)),
                    _ => None,
                };
            }
        }
    }
    None
}
pub(super) fn validate(channels: &Channels, paths: &[&str]) -> Result<(), String> {
    for (path, curve) in channels {
        if !paths.contains(&path.as_str()) {
            return Err("unknown scene animation path".into());
        }
        curve.validate()?;
    }
    Ok(())
}
