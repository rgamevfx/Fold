//! Runtime script inputs borrow the current immutable document. Parameter edits
//! change these inputs, not the script source or compiled module.
use crate::model::{Canvas, Content, ControlValue, Object};
use fold_foundation::Time;

#[derive(Clone, Copy)]
pub struct Context<'a> {
    pub canvas: &'a Canvas,
    pub object: &'a Object,
    pub time: Time,
}
impl<'a> Context<'a> {
    pub fn seconds(self) -> f64 {
        fold_animation::seconds(self.time)
    }
    pub fn number(self, name: &str) -> Result<f64, String> {
        match self.value(name)? {
            ControlValue::Number(value) => Ok(value.at(self.time)),
            _ => Err(format!("Script control {name:?} is not a number")),
        }
    }
    pub fn text(self, name: &str) -> Result<&'a str, String> {
        match self.value(name)? {
            ControlValue::Text(value) => Ok(value),
            _ => Err(format!("Script control {name:?} is not text")),
        }
    }
    pub fn boolean(self, name: &str) -> Result<bool, String> {
        match self.value(name)? {
            ControlValue::Boolean(value) => Ok(*value),
            _ => Err(format!("Script control {name:?} is not a boolean")),
        }
    }
    pub fn color(self, name: &str) -> Result<[f64; 4], String> {
        match self.value(name)? {
            ControlValue::Color(value) => Ok(*value),
            _ => Err(format!("Script control {name:?} is not a color")),
        }
    }
    /// Resource name for an SVG image href. The host owns decode and lifetime.
    pub fn image(self, name: &str) -> Result<String, String> {
        match self.value(name)? {
            ControlValue::Image(asset) => Ok(asset.to_string()),
            _ => Err(format!("Script control {name:?} is not an image")),
        }
    }
    fn value(self, name: &str) -> Result<&'a ControlValue, String> {
        let Content::Script { controls, .. } = &self.object.content else {
            return Err("Object is not a script".into());
        };
        controls
            .values()
            .find(|control| control.name == name)
            .map(|control| &control.value)
            .ok_or_else(|| format!("Missing script control {name:?}"))
    }
}
