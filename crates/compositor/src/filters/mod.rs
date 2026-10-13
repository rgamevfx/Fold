//! Native photographic controls. Authored settings remain in the compositor;
//! immutable GPU programs use the renderer's shared shader and blur services.
use crate::animation::Property;
use serde::{Deserialize, Serialize};
mod render;
pub(crate) use render::{color_shift, compile};

macro_rules! controls {
    ($ty:ty; $( $field:ident, $label:literal, $unit:literal, $lo:expr, $hi:expr; )+) => {
        impl $ty {
            pub fn properties(&self) -> Vec<Property> {
                vec![$(Property { id: stringify!($field).into(), label: $label.into(), unit: $unit,
                    channels: &["Value"], values: vec![self.$field], range: Some([$lo, $hi]) }),+]
            }
            pub fn set_property(&mut self, id: &str, values: &[f64]) -> Result<(), String> {
                let [v] = values else { return Err("Expected one numeric value".into()); };
                match id { $(stringify!($field) => {
                    if !v.is_finite() || !($lo..=$hi).contains(v) { return Err(format!("{} must be in {}..{}", $label, $lo, $hi)); }
                    self.$field = *v;
                },)+ _ => return Err("Unknown filter property".into()) }
                Ok(())
            }
            pub fn validate(&self) -> Result<(), String> {
                for p in self.properties() {
                    let [lo, hi] = p.range.unwrap();
                    if !p.values[0].is_finite() || !(lo..=hi).contains(&p.values[0]) {
                        return Err(format!("{} must be in {lo}..{hi}", p.label));
                    }
                }
                Ok(())
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Lens {
    pub undistort: bool,
    pub distortion: f64,
    pub quartic: f64,
    pub sextic: f64,
    pub tangent_x: f64,
    pub tangent_y: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub squeeze: f64,
    pub scale: f64,
    pub filter: fold_render::operations::Filter,
    pub edges: fold_render::operations::Edges,
}
impl Default for Lens {
    fn default() -> Self {
        Self {
            undistort: false,
            distortion: 0.,
            quartic: 0.,
            sextic: 0.,
            tangent_x: 0.,
            tangent_y: 0.,
            center_x: 0.5,
            center_y: 0.5,
            squeeze: 1.,
            scale: 1.,
            filter: fold_render::operations::Filter::Cubic,
            edges: fold_render::operations::Edges::Transparent,
        }
    }
}
controls!(Lens;
    distortion, "Distortion", "", -1., 1.;
    center_x, "Center X", "", -2., 3.;
    center_y, "Center Y", "", -2., 3.;
    scale, "Output scale", "", 0.1, 10.;
    quartic, "Radial k2", "", -1., 1.;
    sextic, "Radial k3", "", -1., 1.;
    tangent_x, "Tangential X", "", -0.5, 0.5;
    tangent_y, "Tangential Y", "", -0.5, 0.5;
    squeeze, "Anamorphic squeeze", "", 0.25, 4.;
);

pub const FILM_LOOKS: [&str; 8] = [
    "Neutral Negative",
    "Warm Portrait",
    "Cool Daylight",
    "Tungsten Cinema",
    "Vivid Slide",
    "Faded Print",
    "Soft Monochrome",
    "Hard Monochrome",
];
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Film {
    pub preset: FilmPreset,
    pub strength: f64,
    pub exposure: f64,
    pub contrast: f64,
    pub saturation: f64,
    pub rolloff: f64,
    pub shadow_tint: f64,
    pub highlight_tint: f64,
    pub halation: f64,
    pub halation_radius: f64,
    pub halation_threshold: f64,
}
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FilmPreset {
    NeutralNegative,
    WarmPortrait,
    CoolDaylight,
    TungstenCinema,
    VividSlide,
    FadedPrint,
    SoftMonochrome,
    HardMonochrome,
}
impl FilmPreset {
    pub const ALL: [Self; 8] = [
        Self::NeutralNegative,
        Self::WarmPortrait,
        Self::CoolDaylight,
        Self::TungstenCinema,
        Self::VividSlide,
        Self::FadedPrint,
        Self::SoftMonochrome,
        Self::HardMonochrome,
    ];
    pub fn label(self) -> &'static str {
        FILM_LOOKS[self as usize]
    }
}
impl Default for Film {
    fn default() -> Self {
        Self {
            preset: FilmPreset::NeutralNegative,
            strength: 1.,
            exposure: 0.,
            contrast: 1.,
            saturation: 1.,
            rolloff: 0.25,
            shadow_tint: 0.,
            highlight_tint: 0.,
            halation: 0.,
            halation_radius: 12.,
            halation_threshold: 1.,
        }
    }
}
controls!(Film;
    strength, "Strength", "", 0., 1.;
    exposure, "Exposure", "stops", -16., 16.;
    contrast, "Contrast", "", 0.1, 3.;
    saturation, "Saturation", "", 0., 3.;
    rolloff, "Highlight roll-off", "", 0., 2.;
    shadow_tint, "Shadow warmth", "", -1., 1.;
    highlight_tint, "Highlight warmth", "", -1., 1.;
    halation, "Halation", "", 0., 2.;
    halation_radius, "Halation radius", "px", 0., 256.;
    halation_threshold, "Halation threshold", "linear", 0., 16.;
);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum NoiseMode {
    Film,
    Sensor,
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Grain {
    pub mode: NoiseMode,
    pub amount: f64,
    pub size: f64,
    pub color: f64,
    pub shadows: f64,
    pub midtones: f64,
    pub highlights: f64,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub softness: f64,
    pub clumping: f64,
    pub rate: f64,
    pub seed: f64,
    pub freeze: bool,
    pub shot: f64,
    pub read: f64,
    pub fixed: f64,
}
impl Default for Grain {
    fn default() -> Self {
        Self {
            mode: NoiseMode::Film,
            amount: 0.025,
            size: 1.5,
            color: 0.15,
            shadows: 0.7,
            midtones: 1.,
            highlights: 0.35,
            red: 1.,
            green: 1.,
            blue: 1.,
            softness: 0.5,
            clumping: 0.15,
            rate: 24.,
            seed: 1.,
            freeze: false,
            shot: 1.,
            read: 0.3,
            fixed: 0.,
        }
    }
}
controls!(Grain;
    amount, "Amount", "", 0., 1.;
    size, "Size", "px", 0.25, 32.;
    color, "Color amount", "", 0., 1.;
    shadows, "Shadows", "", 0., 4.;
    midtones, "Midtones", "", 0., 4.;
    highlights, "Highlights", "", 0., 4.;
    red, "Red response", "", 0., 4.;
    green, "Green response", "", 0., 4.;
    blue, "Blue response", "", 0., 4.;
    softness, "Softness", "", 0., 1.;
    clumping, "Clumping", "", 0., 1.;
    rate, "Variation rate", "fps", 0., 120.;
    seed, "Seed", "", 0., 1000000.;
    shot, "Shot noise", "", 0., 4.;
    read, "Read noise", "", 0., 4.;
    fixed, "Fixed pattern", "", 0., 4.;
);

/// Collapsed inspector sections; the underlying animation catalog stays complete.
pub(crate) fn group(config: &crate::OperationConfig, property: &str) -> &'static str {
    use crate::OperationConfig as P;
    match config {
        P::ColorShift { .. } if matches!(property, "selected" | "range" | "softness") => "Select",
        P::ColorShift { .. }
            if matches!(
                property,
                "hue_range" | "saturation_range" | "brightness_range"
            ) =>
        {
            "Refine selection"
        }
        P::ColorShift { .. } => "Adjust",
        P::ChromaticAberration { .. }
            if matches!(
                property,
                "center_x"
                    | "center_y"
                    | "falloff"
                    | "red_x"
                    | "red_y"
                    | "green_x"
                    | "green_y"
                    | "blue_x"
                    | "blue_y"
            ) =>
        {
            "Advanced"
        }
        P::LensDistortion { .. }
            if matches!(
                property,
                "quartic" | "sextic" | "tangent_x" | "tangent_y" | "squeeze"
            ) =>
        {
            "Advanced lens"
        }
        P::FilmEmulation { .. } if property.starts_with("halation") => "Halation",
        P::FilmEmulation { .. }
            if matches!(property, "rolloff" | "shadow_tint" | "highlight_tint") =>
        {
            "Tone and color"
        }
        P::Grain { .. }
            if matches!(
                property,
                "shadows" | "midtones" | "highlights" | "red" | "green" | "blue"
            ) =>
        {
            "Tonal and channel response"
        }
        P::Grain { .. } if matches!(property, "shot" | "read" | "fixed") => "Sensor noise",
        P::Grain { .. } if matches!(property, "softness" | "clumping" | "rate" | "seed") => {
            "Texture and timing"
        }
        P::Retime { .. } if property.starts_with("shutter") => "Motion blur",
        _ => "",
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ColorShift {
    pub selected: [f64; 4],
    pub target: [f64; 4],
    pub use_target: bool,
    pub range: f64,
    pub softness: f64,
    pub hue_range: f64,
    pub saturation_range: f64,
    pub brightness_range: f64,
    pub hue: f64,
    pub saturation: f64,
    pub brightness: f64,
}
impl Default for ColorShift {
    fn default() -> Self {
        Self {
            selected: [0.8, 0.08, 0.04, 1.],
            target: [0.04, 0.2, 0.8, 1.],
            use_target: false,
            range: 1.,
            softness: 0.5,
            hue_range: 30.,
            saturation_range: 0.4,
            brightness_range: 2.,
            hue: 0.,
            saturation: 1.,
            brightness: 0.,
        }
    }
}
controls!(ColorShift;
    range,"Range","",0.01,4.;
    softness,"Softness","",0.,1.;
    hue,"Hue","°",-180.,180.;
    saturation,"Saturation","",0.,3.;
    brightness,"Brightness","stops",-8.,8.;
    hue_range,"Hue tolerance","°",0.1,180.;
    saturation_range,"Saturation tolerance","",0.001,1.;
    brightness_range,"Brightness tolerance","stops",0.01,16.;
);
impl ColorShift {
    pub fn color_properties(&self) -> Vec<Property> {
        [
            ("selected", "Selected color", self.selected),
            ("target", "Target color", self.target),
        ]
        .into_iter()
        .map(|(id, label, v)| Property {
            id: id.into(),
            label: label.into(),
            unit: "",
            channels: &["R", "G", "B"],
            values: v[..3].to_vec(),
            range: None,
        })
        .collect()
    }
    pub fn set_color(&mut self, id: &str, v: &[f64]) -> Result<(), String> {
        let v = match v {
            [r, g, b] => [*r, *g, *b, 1.],
            [r, g, b, a] => [*r, *g, *b, *a],
            _ => return Err("Expected RGB color".into()),
        };
        if v.iter().any(|v| !v.is_finite() || v.abs() > 1e6) || v[3] != 1. {
            return Err("Qualifier colors require finite RGB and opaque alpha".into());
        }
        match id {
            "selected" => self.selected = v,
            "target" => self.target = v,
            _ => return Err("Unknown color".into()),
        };
        Ok(())
    }
    pub fn validate_colors(&self) -> Result<(), String> {
        let mut c = self.clone();
        c.set_color("selected", &self.selected)?;
        c.set_color("target", &self.target)
    }
}
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChromaticAberration {
    pub amount: f64,
    pub red: f64,
    pub green: f64,
    pub blue: f64,
    pub blur: f64,
    pub saturation: f64,
    pub protect: f64,
    pub center_x: f64,
    pub center_y: f64,
    pub falloff: f64,
    pub red_x: f64,
    pub red_y: f64,
    pub green_x: f64,
    pub green_y: f64,
    pub blue_x: f64,
    pub blue_y: f64,
    pub edges: fold_render::operations::Edges,
}
impl Default for ChromaticAberration {
    fn default() -> Self {
        Self {
            amount: 1.,
            red: 2.,
            green: 0.,
            blue: -2.,
            blur: 0.,
            saturation: 1.,
            protect: 0.2,
            center_x: 0.5,
            center_y: 0.5,
            falloff: 1.5,
            red_x: 0.,
            red_y: 0.,
            green_x: 0.,
            green_y: 0.,
            blue_x: 0.,
            blue_y: 0.,
            edges: fold_render::operations::Edges::Clamp,
        }
    }
}
controls!(ChromaticAberration;
    amount,"Amount","",-10.,10.;
    red,"Red separation","px",-100.,100.;
    green,"Green separation","px",-100.,100.;
    blue,"Blue separation","px",-100.,100.;
    blur,"Fringe blur","px",0.,128.;
    saturation,"Fringe saturation","",0.,3.;
    protect,"Protect center","",0.,1.;
    center_x,"Center X","",-2.,3.;
    center_y,"Center Y","",-2.,3.;
    falloff,"Falloff","",0.1,6.;
    red_x,"Red X","px",-100.,100.;
    red_y,"Red Y","px",-100.,100.;
    green_x,"Green X","px",-100.,100.;
    green_y,"Green Y","px",-100.,100.;
    blue_x,"Blue X","px",-100.,100.;
    blue_y,"Blue Y","px",-100.,100.;
);
