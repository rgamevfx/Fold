//! Authored spatial surfaces, independent of mesh storage and topology.
use super::{Geometry, GeometryData, Value, style};
use crate::{
    registry::Builtin,
    types::{Literal, Space, Unit},
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Material {
    pub color: style::Color,
    pub unlit: bool,
    pub roughness: f64,
    pub reflectance: f64,
}
pub(super) fn apply(
    geometry: &Geometry,
    builtin: Builtin,
    args: &BTreeMap<&str, Value>,
) -> Result<Geometry, String> {
    if geometry.space != Space::World
        || geometry.unit != Unit::Meters
        || !matches!(
            geometry.data,
            GeometryData::Solids(_) | GeometryData::Mesh(_) | GeometryData::Instances(_)
        )
    {
        return Err("Material requires world/meter solid geometry".into());
    }
    let primitive = |name| {
        args.get(name)
            .ok_or_else(|| format!("Missing {name}"))?
            .primitive()
    };
    let color = surface_color(primitive("color")?)?;
    let fraction = |name| match primitive(name)? {
        Literal::Scalar { value } if value.is_finite() && (0. ..=1.).contains(value) => Ok(*value),
        _ => Err(format!("{name} must be in [0,1]")),
    };
    let (unlit, roughness, reflectance) = match builtin {
        Builtin::LitMaterial => (false, fraction("roughness")?, fraction("reflectance")?),
        Builtin::UnlitMaterial => (true, 0.5, 0.5),
        _ => return Err("Expected Material operation".into()),
    };
    let mut styled = geometry.clone();
    styled.appearance.material = Some(Material {
        color,
        unlit,
        roughness,
        reflectance,
    });
    Ok(styled)
}

pub(super) fn surface_color(value: &Literal) -> Result<style::Color, String> {
    let color = style::color(value)?;
    if color.value[..3].iter().any(|v| !(0. ..=65504.).contains(v)) {
        return Err("Material RGB must be in [0,65504] linear Rec.709".into());
    }
    Ok(color)
}
