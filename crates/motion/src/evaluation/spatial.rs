//! Evaluated spatial controls retain authored precision and semantic identity.
use super::Value;
use crate::types::{Alpha, Encoding, Literal, Space, Unit};
use serde::Serialize;
use std::{collections::BTreeMap, sync::Arc};

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Projection {
    Perspective { vertical_fov: f64 },
    Orthographic { vertical_size: f64 },
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Camera {
    pub id: u64,
    pub position: [f64; 3],
    pub target: [f64; 3],
    pub up: [f64; 3],
    pub projection: Projection,
    pub near: f64,
    pub far: f64,
    pub ev100: f64,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Light {
    pub id: u64,
    /// World-space direction in which rays travel. No shadow mode is implied.
    pub direction: [f64; 3],
    pub color: [f64; 3],
    pub illuminance: f64,
}
#[derive(Clone, Debug, PartialEq)]
pub struct CameraEntry {
    pub id: u64,
    pub camera: Arc<Camera>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct LightEntry {
    pub id: u64,
    pub light: Arc<Light>,
}

type Args<'a> = BTreeMap<&'a str, Value>;
fn primitive<'a>(args: &'a Args<'_>, name: &str) -> Result<&'a Literal, String> {
    args.get(name)
        .ok_or_else(|| format!("Missing {name}"))?
        .primitive()
}
fn vector(args: &Args<'_>, name: &str, unit: Unit) -> Result<[f64; 3], String> {
    let Literal::Vector {
        value,
        unit: actual,
        space: Space::World,
    } = primitive(args, name)?
    else {
        return Err(format!("Expected world vector: {name}"));
    };
    if *actual != unit || value.len() != 3 || !value.iter().all(|v| v.is_finite() && v.abs() <= 1e6)
    {
        return Err(format!("Invalid world vector: {name}"));
    }
    Ok([value[0], value[1], value[2]])
}
fn scalar(args: &Args<'_>, name: &str) -> Result<f64, String> {
    let Literal::Scalar { value } = primitive(args, name)? else {
        return Err(format!("Expected scalar: {name}"));
    };
    if !value.is_finite() {
        return Err(format!("Non-finite {name}"));
    }
    Ok(*value)
}
fn distance(args: &Args<'_>, name: &str) -> Result<f64, String> {
    let Literal::Distance {
        value,
        unit: Unit::Meters,
    } = primitive(args, name)?
    else {
        return Err(format!("Expected meters: {name}"));
    };
    if !value.is_finite() {
        return Err(format!("Non-finite {name}"));
    }
    Ok(*value)
}
fn normalized(v: [f64; 3]) -> Result<[f64; 3], String> {
    let length = v.iter().map(|x| x * x).sum::<f64>().sqrt();
    if length < 1e-12 {
        return Err("Camera/light direction must be nonzero".into());
    }
    Ok(v.map(|x| x / length))
}
pub(super) fn camera(id: u64, args: &Args<'_>, orthographic: bool) -> Result<Camera, String> {
    let camera = Camera {
        id,
        position: vector(args, "position", Unit::Meters)?,
        target: vector(args, "target", Unit::Meters)?,
        up: vector(args, "up", Unit::Unitless)?,
        projection: if orthographic {
            Projection::Orthographic {
                vertical_size: distance(args, "vertical_size")?,
            }
        } else {
            Projection::Perspective {
                vertical_fov: scalar(args, "vertical_fov")?,
            }
        },
        near: distance(args, "near")?,
        far: distance(args, "far")?,
        ev100: scalar(args, "ev100")?,
    };
    let forward = normalized([0, 1, 2].map(|i| camera.target[i] - camera.position[i]))?;
    let up = normalized(camera.up)?;
    let cross = [
        forward[1] * up[2] - forward[2] * up[1],
        forward[2] * up[0] - forward[0] * up[2],
        forward[0] * up[1] - forward[1] * up[0],
    ];
    if cross.iter().map(|v| v * v).sum::<f64>() < 1e-12 {
        return Err("Camera up direction is parallel to its view direction".into());
    }
    let valid_projection = match camera.projection {
        Projection::Perspective { vertical_fov } => {
            vertical_fov > 0.001 && vertical_fov < std::f64::consts::PI - 0.001
        }
        Projection::Orthographic { vertical_size } => (1e-6..=1e6).contains(&vertical_size),
    };
    if !valid_projection
        || camera.near < 1e-6
        || camera.far <= camera.near
        || camera.far > 1e6
        || !(-20. ..=20.).contains(&camera.ev100)
    {
        return Err("Invalid camera projection, clipping or exposure".into());
    }
    Ok(camera)
}
pub(super) fn light(id: u64, args: &Args<'_>) -> Result<Light, String> {
    let direction = vector(args, "direction", Unit::Unitless)?;
    normalized(direction)?;
    let Literal::Color {
        value,
        encoding: Encoding::LinearRec709,
        alpha: Alpha::Straight,
    } = primitive(args, "color")?
    else {
        return Err("Expected straight linear Rec.709 light color".into());
    };
    if value[3] != 1.
        || !value[..3]
            .iter()
            .all(|v| v.is_finite() && (0. ..=1.).contains(v))
    {
        return Err("Light color must be opaque with RGB in [0,1]".into());
    }
    let illuminance = scalar(args, "illuminance")?;
    if !(0. ..=100_000.).contains(&illuminance) {
        return Err("Directional illuminance must be in [0,100000] lux".into());
    }
    Ok(Light {
        id,
        direction,
        color: [value[0], value[1], value[2]],
        illuminance,
    })
}
