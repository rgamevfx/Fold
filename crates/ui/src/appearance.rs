//! User appearance preferences, independent of projects and feature models.
use super::imgui::{Style, StyleColor as C};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GraphText {
    pub labels: f32,
    pub spacing: f32,
    pub titles: f32,
    pub secondary: f32,
    pub hide_details: bool,
}
impl Default for GraphText {
    fn default() -> Self {
        Self {
            labels: 22.,
            spacing: 18.,
            titles: 22.,
            secondary: 15.,
            hide_details: false,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Colors {
    pub text: [f32; 3],
    pub secondary: [f32; 3],
    pub background: [f32; 3],
    pub surface: [f32; 3],
    pub header: [f32; 3],
    pub border: [f32; 3],
    pub accent: [f32; 3],
}
impl Default for Colors {
    fn default() -> Self {
        Self {
            text: [0.94, 0.95, 0.97],
            secondary: [0.60, 0.63, 0.68],
            background: [0.06, 0.065, 0.075],
            surface: [0.10, 0.115, 0.135],
            header: [0.14, 0.17, 0.21],
            border: [0.27, 0.30, 0.35],
            accent: [0.30, 0.60, 0.88],
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Appearance {
    pub application_text: f32,
    pub graph: GraphText,
    pub colors: Colors,
}
impl Default for Appearance {
    fn default() -> Self {
        Self {
            application_text: 15.,
            graph: GraphText::default(),
            colors: Colors::default(),
        }
    }
}
impl Appearance {
    pub fn validate(&self) -> Result<(), String> {
        for (name, value, min, max) in [
            ("Application text", self.application_text, 12., 24.),
            ("Node labels", self.graph.labels, 12., 28.),
            ("Node spacing", self.graph.spacing, 12., 28.),
            ("Node titles", self.graph.titles, 14., 32.),
            ("Node secondary text", self.graph.secondary, 10., 24.),
        ] {
            if !value.is_finite() || !(min..=max).contains(&value) {
                return Err(format!("{name} must be between {min} and {max} px"));
            }
        }
        for color in [
            self.colors.text,
            self.colors.secondary,
            self.colors.background,
            self.colors.surface,
            self.colors.header,
            self.colors.border,
            self.colors.accent,
        ] {
            if color
                .iter()
                .any(|v| !v.is_finite() || !(0. ..=1.).contains(v))
            {
                return Err("UI color values must be between 0 and 1".into());
            }
        }
        Ok(())
    }

    pub(crate) fn apply(&self, style: &mut Style) {
        style.set_font_size_base(self.application_text);
        let c = self.colors;
        for (tokens, color) in [
            (&[C::Text][..], c.text),
            (&[C::TextDisabled][..], c.secondary),
            (
                &[C::WindowBg, C::ChildBg, C::DockingEmptyBg][..],
                c.background,
            ),
            (&[C::PopupBg, C::FrameBg, C::ScrollbarBg][..], c.surface),
            (
                &[
                    C::TitleBg,
                    C::TitleBgActive,
                    C::TitleBgCollapsed,
                    C::MenuBarBg,
                    C::Button,
                    C::Header,
                    C::Tab,
                    C::TabDimmed,
                    C::TableHeaderBg,
                ][..],
                c.header,
            ),
            (
                &[
                    C::Border,
                    C::Separator,
                    C::ScrollbarGrab,
                    C::TableBorderStrong,
                    C::TableBorderLight,
                ][..],
                c.border,
            ),
            (
                &[
                    C::CheckMark,
                    C::SliderGrab,
                    C::SliderGrabActive,
                    C::ButtonHovered,
                    C::ButtonActive,
                    C::HeaderHovered,
                    C::HeaderActive,
                    C::TabSelected,
                    C::TabHovered,
                    C::TabSelectedOverline,
                    C::FrameBgHovered,
                    C::FrameBgActive,
                    C::SeparatorHovered,
                    C::SeparatorActive,
                    C::ResizeGripHovered,
                    C::ResizeGripActive,
                    C::ScrollbarGrabHovered,
                    C::ScrollbarGrabActive,
                ][..],
                c.accent,
            ),
        ] {
            for token in tokens {
                style.set_color(*token, [color[0], color[1], color[2], 1.]);
            }
        }
        style.set_color(
            C::TextSelectedBg,
            [c.accent[0], c.accent[1], c.accent[2], 0.35],
        );
        style.set_color(
            C::DockingPreview,
            [c.accent[0], c.accent[1], c.accent[2], 0.5],
        );
    }
}
