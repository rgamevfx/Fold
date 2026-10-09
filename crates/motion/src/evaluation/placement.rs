//! Explicit placement spaces; world transforms apply scale, rotation, translation.
use super::{Geometry, GeometryData, Value};
use crate::types::{Literal, Space, Unit};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(tag = "mode", rename_all = "snake_case")]
pub enum Transform {
    Screen { position: [f64; 2] },
    World(World),
    CameraPlane(CameraPlane),
}
/// Camera attachment retains the exact evaluated camera, independent of which
/// camera the containing scene selects. Both sizing modes participate in scene depth.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CameraPlane {
    pub camera: std::sync::Arc<super::spatial::Camera>,
    pub distance: f64,
    pub position: [f64; 2],
    pub sizing: PlaneSizing,
    pub depth: PlaneDepth,
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum PlaneSizing {
    Viewport,
    Physical { meters_per_unit: f64 },
}
#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum PlaneDepth {
    Scene,
}

pub(super) fn camera_plane(
    geometry: &Geometry,
    args: &BTreeMap<&str, Value>,
    physical: bool,
) -> Result<(CameraPlane, f64), String> {
    if geometry.space != Space::Screen
        || geometry.unit != Unit::Canvas
        || !matches!(
            geometry.data,
            GeometryData::Contours(_) | GeometryData::Text(_) | GeometryData::Outlines(_)
        )
    {
        return Err("Camera Plane requires planar screen/canvas geometry".into());
    }
    let Some(Value::Camera(camera)) = args.get("camera") else {
        return Err("Expected attached camera".into());
    };
    let primitive = |name| {
        args.get(name)
            .ok_or_else(|| format!("Missing {name}"))?
            .primitive()
    };
    let Literal::Distance {
        value: distance,
        unit: Unit::Meters,
    } = primitive("distance")?
    else {
        return Err("Expected camera distance in meters".into());
    };
    let Literal::Vector {
        value: position,
        unit: Unit::Canvas,
        space: Space::Screen,
    } = primitive("position")?
    else {
        return Err("Expected canvas position".into());
    };
    let Literal::Scalar { value: opacity } = primitive("opacity")? else {
        return Err("Expected opacity".into());
    };
    if !distance.is_finite()
        || !(1e-6..=1e6).contains(distance)
        || position.len() != 2
        || position.iter().any(|v| !v.is_finite() || v.abs() > 1e6)
        || !(0. ..=1.).contains(opacity)
    {
        return Err("Invalid Camera Plane distance, position or opacity".into());
    }
    let sizing = if physical {
        let Literal::Scalar { value } = primitive("meters_per_unit")? else {
            return Err("Expected physical scale".into());
        };
        if !value.is_finite() || !(1e-9..=1e6).contains(value) {
            return Err("Invalid Camera Plane physical scale".into());
        }
        PlaneSizing::Physical {
            meters_per_unit: *value,
        }
    } else {
        PlaneSizing::Viewport
    };
    Ok((
        CameraPlane {
            camera: camera.clone(),
            distance: *distance,
            position: [position[0], position[1]],
            sizing,
            depth: PlaneDepth::Scene,
        },
        *opacity,
    ))
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct World {
    /// Evaluated hierarchy matrix, including shear. Authored controls remain TRS.
    pub affine: Option<fold_foundation::affine::Affine>,
    pub position: [f64; 3],
    /// Unit quaternion, xyzw; authored precision is retained.
    pub rotation: [f64; 4],
    pub scale: [f64; 3],
}
impl World {
    pub fn matrix(&self) -> fold_foundation::affine::Affine {
        self.affine.unwrap_or_else(|| {
            fold_foundation::affine::Affine::trs(self.position, self.rotation, self.scale)
        })
    }
    pub fn point(&self, point: [f64; 3]) -> [f64; 3] {
        self.matrix().point(point)
    }
}
pub(super) fn world(
    geometry: &Geometry,
    args: &BTreeMap<&str, Value>,
) -> Result<(World, f64), String> {
    if geometry.space != Space::World
        || geometry.unit != Unit::Meters
        || !matches!(
            geometry.data,
            GeometryData::Solids(_) | GeometryData::Mesh(_) | GeometryData::Instances(_)
        )
    {
        return Err("World placement requires world/meter solid geometry".into());
    }
    let primitive = |name| {
        args.get(name)
            .ok_or_else(|| format!("Missing {name}"))?
            .primitive()
    };
    let vector = |name, unit| match primitive(name)? {
        Literal::Vector {
            value,
            unit: actual,
            space: Space::World,
        } if *actual == unit && value.len() == 3 && value.iter().all(|v| v.is_finite()) => {
            Ok([value[0], value[1], value[2]])
        }
        _ => Err(format!("Expected world {name}")),
    };
    let position = vector("position", Unit::Meters)?;
    let scale = vector("scale", Unit::Unitless)?;
    let Literal::Rotation {
        value: rotation,
        space: Space::World,
    } = primitive("rotation")?
    else {
        return Err("Expected world rotation".into());
    };
    primitive("rotation")?.validate()?;
    let Literal::Scalar { value: opacity } = primitive("opacity")? else {
        return Err("Expected opacity".into());
    };
    if position.iter().any(|v| v.abs() > 1e6)
        || scale.iter().any(|v| *v <= 0. || *v > 1e6)
        || !(0. ..=1.).contains(opacity)
    {
        return Err("World position, scale or opacity exceeds supported ranges".into());
    }
    Ok((
        World {
            affine: None,
            position,
            rotation: *rotation,
            scale,
        },
        *opacity,
    ))
}
