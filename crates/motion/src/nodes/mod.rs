//! Built-in node implementations. Ordinary additions need one module and one
//! catalog entry; the evaluator does not match over node types.
pub mod animation;
pub mod content;
pub mod distribution;
pub mod influence;
pub mod interface;
pub mod response;
pub mod scene;
pub mod values;
use crate::{
    evaluation::{Evaluator, Outputs},
    fields::{Datum, Kind},
    graph::{
        Node,
        definition::{Definition, Socket, no_settings, validate_empty},
    },
};
pub fn scalar(id: &'static str, value: f64, unit: &'static str) -> Socket {
    Socket::value(id, Datum::Scalar(value), unit)
}
pub fn vector(id: &'static str, value: [f64; 2], unit: &'static str) -> Socket {
    Socket::value(id, Datum::Vector(value), unit)
}
pub fn color(id: &'static str, value: [f64; 4]) -> Socket {
    Socket::value(id, Datum::Color(value), "linear sRGB")
}
pub fn boolean(id: &'static str, value: bool) -> Socket {
    Socket::value(id, Datum::Bool(value), "")
}
pub fn definition(
    id: &'static str,
    name: &'static str,
    category: &'static str,
    inputs: Vec<Socket>,
    outputs: Vec<(&'static str, Kind)>,
    evaluate: fn(&mut Evaluator<'_>, &Node) -> Result<Outputs, String>,
) -> Definition {
    Definition {
        id,
        name,
        category,
        description: name,
        inputs,
        outputs,
        signature: None,
        defaults: no_settings,
        validate: validate_empty,
        evaluate,
    }
}
pub fn bounded_count(value: f64, max: usize) -> Result<usize, String> {
    if !value.is_finite() || value < 0. || value > max as f64 || value.fract() != 0. {
        Err(format!("count must be an integer in 0..={max}"))
    } else {
        Ok(value as usize)
    }
}
