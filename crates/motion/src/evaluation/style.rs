//! Retained appearance, independent of geometry topology and storage.
use super::{Geometry, Value};
use crate::{
    registry::Builtin,
    types::{Alpha, Encoding, Literal, Space, Unit},
};
use serde::Serialize;
use std::collections::BTreeMap;

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Color {
    /// Authored components; alpha is not premultiplied and HDR is not clamped.
    pub value: [f64; 4],
    pub encoding: Encoding,
    pub alpha: Alpha,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Fill {
    pub color: Color,
    pub even_odd: bool,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StrokeMode {
    SolidCenteredButtMiter,
    SolidCenteredButtRound,
    SolidCenteredRoundMiter,
    SolidCenteredRoundRound,
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Stroke {
    pub color: Color,
    pub width: f64,
    pub unit: Unit,
    pub mode: StrokeMode,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Appearance {
    /// Absence means no fill; unstyled geometry has no implicit paint.
    pub fill: Option<Fill>,
    pub stroke: Option<Stroke>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub material: Option<super::material::Material>,
}

pub(super) fn apply(
    geometry: &Geometry,
    builtin: Builtin,
    args: &BTreeMap<&str, Value>,
) -> Result<Geometry, String> {
    if geometry.space != Space::Screen || geometry.unit != Unit::Canvas {
        return Err("Planar styling requires screen/canvas geometry".into());
    }
    let primitive = |name| {
        args.get(name)
            .ok_or_else(|| format!("Missing {name}"))?
            .primitive()
    };
    let color = color(primitive("color")?)?;
    let mut styled = geometry.clone();
    match builtin {
        Builtin::Fill => {
            let Literal::Bool { value: even_odd } = primitive("even_odd")? else {
                return Err("Expected fill rule".into());
            };
            styled.appearance.fill = Some(Fill {
                color,
                even_odd: *even_odd,
            });
        }
        Builtin::Stroke
        | Builtin::StrokeButtRound
        | Builtin::StrokeRoundMiter
        | Builtin::StrokeRoundRound => {
            let Literal::Distance {
                value: width,
                unit: Unit::Canvas,
            } = primitive("width")?
            else {
                return Err("Expected canvas stroke width".into());
            };
            if !width.is_finite() || !(0. ..=10000.).contains(width) {
                return Err("Stroke width must be in [0,10000] canvas units".into());
            }
            styled.appearance.stroke = Some(Stroke {
                color,
                width: *width,
                unit: Unit::Canvas,
                mode: match builtin {
                    Builtin::StrokeButtRound => StrokeMode::SolidCenteredButtRound,
                    Builtin::StrokeRoundMiter => StrokeMode::SolidCenteredRoundMiter,
                    Builtin::StrokeRoundRound => StrokeMode::SolidCenteredRoundRound,
                    _ => StrokeMode::SolidCenteredButtMiter,
                },
            });
        }
        _ => return Err("Expected Fill or Stroke".into()),
    }
    Ok(styled)
}

pub(super) fn color(authored_color: &Literal) -> Result<Color, String> {
    let Literal::Color {
        value,
        encoding: Encoding::LinearRec709,
        alpha: Alpha::Straight,
    } = authored_color
    else {
        return Err("Expected straight linear Rec.709 color".into());
    };
    authored_color.validate()?;
    Ok(Color {
        value: *value,
        encoding: Encoding::LinearRec709,
        alpha: Alpha::Straight,
    })
}

impl StrokeMode {
    pub fn cap(self) -> fold_render::vector::Cap {
        use fold_render::vector::Cap;
        match self {
            Self::SolidCenteredButtMiter | Self::SolidCenteredButtRound => Cap::Butt,
            Self::SolidCenteredRoundMiter | Self::SolidCenteredRoundRound => Cap::Round,
        }
    }
    pub fn join(self) -> fold_render::vector::Join {
        use fold_render::vector::Join;
        match self {
            Self::SolidCenteredButtMiter | Self::SolidCenteredRoundMiter => Join::Miter,
            Self::SolidCenteredButtRound | Self::SolidCenteredRoundRound => Join::Round,
        }
    }
}
