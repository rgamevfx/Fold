//! Built-in node implementations. Ordinary additions need one module and one
//! catalog entry; the evaluator does not match over node types.
pub mod align_content;
pub mod animation;
pub mod content;
pub mod distribution;
pub mod expression;
pub mod format_text;
pub mod influence;
pub mod interface;
pub mod path_motion;
pub mod path_points;
pub mod response;
pub mod scene;
pub mod text_on_points;
pub mod text_select;
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
    Socket::value(id, Datum::Color(value), "")
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
        atomic_settings: &[],
        settings_schema: None,
        inputs,
        outputs,
        signature: None,
        defaults: no_settings,
        validate: validate_empty,
        evaluate,
    }
}
pub fn bounded_count(value: f64, max: usize) -> Result<usize, String> {
    if !value.is_finite() || value < 0. || value > max as f64 {
        Err(format!("count must be in 0..={max}"))
    } else {
        Ok(value.round() as usize)
    }
}

pub mod copy_colors;

pub mod stagger;

pub mod oscillator;

pub mod color_ramp;

pub mod solver;

pub mod attributes;
pub mod history;

pub mod convex_sweep;
pub mod shape_grid;
pub mod shape_points;

pub mod direction;
pub mod particle_targets;
pub mod particles;

pub mod image;
