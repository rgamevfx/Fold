//! Application typography. Font handles belong to the host ImGui context;
//! panels borrow/copy them, never load their own fonts or rebuild the atlas.
use super::appearance::{Appearance, GraphText};
use super::imgui::fonts::atlas::StbTrueTypeFontData;
use super::imgui::{Context, FontConfig, FontId, FontSource, Ui};
use std::{cell::Cell, rc::Rc};

pub const BODY_SIZE: f32 = 15.;

#[derive(Clone, Copy)]
pub enum TextRole {
    Body,
    Title,
    Secondary,
}
// Select the next sufficient density. Continuous zoom must not create an
// unbounded collection of baked font sizes. ImGui multiplies this density by
// framebuffer density; do not apply OS DPI a second time here.
const DENSITIES: [f32; 7] = [1., 1.5, 2., 3., 4., 6., 8.];

#[derive(Clone)]
pub struct Typography {
    fonts: [FontId; 7],
    appearance: Rc<Cell<Appearance>>,
}
impl Typography {
    /// Install once, before the first frame, on an empty application atlas.
    /// The first font is also the default for ordinary ImGui controls.
    pub fn install(context: &mut Context) -> Self {
        let data = StbTrueTypeFontData::from_slice(include_bytes!(
            "../../../resources/fonts/NotoSans-Regular.ttf"
        ))
        .expect("bundled Noto Sans must remain a validated TrueType font");
        let fonts = DENSITIES.map(|density| {
            context
                .font_atlas()
                .add_font(
                    &[
                        FontSource::stb_truetype_with_size(data.clone(), BODY_SIZE).with_config(
                            FontConfig::new()
                                .name(&format!("Fold UI {density}x"))
                                .pixel_snap_h(false)
                                .rasterizer_density(density),
                        ),
                    ],
                )
        });
        context.style_mut().set_font_size_base(BODY_SIZE);
        Self {
            fonts,
            appearance: Rc::new(Cell::new(Appearance::default())),
        }
    }

    pub fn appearance(&self) -> Appearance {
        self.appearance.get()
    }

    pub fn set_appearance(&self, value: Appearance) -> Result<(), String> {
        value.validate()?;
        self.appearance.set(value);
        Ok(())
    }

    pub(crate) fn canvas_font(&self, zoom: f32) -> FontId {
        self.fonts[density_index(zoom)]
    }
}

fn density_index(zoom: f32) -> usize {
    DENSITIES
        .iter()
        .position(|density| *density >= zoom)
        .unwrap_or(DENSITIES.len() - 1)
}

/// Select an application text role. ImGui applies global UI and DPI scaling;
/// layout and drawing use exactly the same baked metrics and rounded size.
pub fn push_role(ui: &Ui, role: TextRole, text: GraphText) -> super::imgui::FontStackToken<'_> {
    let size = match role {
        TextRole::Body => text.labels,
        TextRole::Title => text.titles,
        TextRole::Secondary => text.secondary,
    };
    ui.push_font_with_size(None, size)
}

pub fn text_size(ui: &Ui, role: TextRole, text: &str, sizes: GraphText) -> [f32; 2] {
    let _role = push_role(ui, role, sizes);
    ui.calc_text_size(text)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn continuous_zoom_has_bounded_sufficient_density() {
        for step in 1..=800 {
            let zoom = step as f32 / 100.;
            let index = density_index(zoom);
            assert!(DENSITIES[index] >= zoom);
            assert!(index == 0 || DENSITIES[index - 1] < zoom);
        }
    }
}
