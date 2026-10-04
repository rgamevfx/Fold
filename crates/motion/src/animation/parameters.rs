//! Component animation of literal motion inputs. Driven sockets keep their
//! existing graph semantics; animation never silently replaces a connection.
use crate::{
    fields::Datum,
    graph::{Input, Node},
};
use fold_animation::Curve;
use fold_foundation::Time;
use std::collections::BTreeMap;
pub type Channels = BTreeMap<String, Curve>;
pub fn components(value: &Datum) -> Option<(&'static [&'static str], Vec<f64>)> {
    match value {
        Datum::Scalar(v) => Some((&["Value"], vec![*v])),
        Datum::Vector(v) => Some((&["X", "Y"], v.to_vec())),
        Datum::Color(v) => Some((&["R", "G", "B", "A"], v.to_vec())),
        _ => None,
    }
}
pub fn set_components(value: &mut Datum, components: &[f64]) {
    match value {
        Datum::Scalar(v) => *v = components[0],
        Datum::Vector(v) => v.copy_from_slice(components),
        Datum::Color(v) => v.copy_from_slice(components),
        _ => {}
    }
}
impl Node {
    pub fn remove_animation_channel(&mut self, path: &str, time: Time) {
        if let Some(curve) = self.animation.remove(path)
            && let Some(value) = curve.sample(time)
            && let Some((input, component)) = path.rsplit_once('.')
            && let Ok(component) = component.parse::<usize>()
            && let Some(Input::Value(base)) = self.inputs.get_mut(input)
            && let Some((_, mut values)) = components(base)
            && let Some(v) = values.get_mut(component)
        {
            *v = value;
            set_components(base, &values);
        }
    }

    pub fn sample_input(&self, socket: &str, base: &Datum, time: Time) -> Datum {
        let mut result = base.clone();
        if let Some((_, mut values)) = components(base) {
            for (i, value) in values.iter_mut().enumerate() {
                if let Some(curve) = self.animation.get(&format!("{socket}.{i}")) {
                    *value = curve.sample(time).unwrap_or(*value);
                }
            }
            set_components(&mut result, &values);
        }
        result
    }
    pub fn validate_animation(&self) -> Result<(), String> {
        for (path, curve) in &self.animation {
            let (socket, component) = path
                .rsplit_once('.')
                .ok_or("Invalid motion animation path")?;
            let component = component
                .parse::<usize>()
                .map_err(|_| "Invalid animation component")?;
            let Some(Input::Value(base)) = self.inputs.get(socket) else {
                return Err(
                    "Animation requires a literal input; disconnect its driver first".into(),
                );
            };
            let (_, values) = components(base).ok_or("Input is not animatable")?;
            if component >= values.len() {
                return Err("Animation component is unavailable".into());
            }
            curve.validate()?;
        }
        Ok(())
    }
}
