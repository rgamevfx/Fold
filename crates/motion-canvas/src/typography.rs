//! Deterministic line layout using the same packaged font as the Motion renderer.
//! Font selection must be extended with immutable font resources, not system-font
//! lookup on the UI thread.
use crate::model::{TextAlignment, TextLayout};
const FONT: &[u8] = include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf");
pub struct Line {
    pub text: String,
    pub x: f64,
    pub baseline: f64,
    pub width: f64,
}
pub fn layout(text: &str, size: f64, settings: &TextLayout) -> Vec<Line> {
    let face = rustybuzz::Face::from_slice(FONT, 0).expect("packaged Noto Sans");
    let scale = size / f64::from(face.units_per_em());
    let measure = |text: &str| {
        let mut buffer = rustybuzz::UnicodeBuffer::new();
        buffer.push_str(text);
        buffer.guess_segment_properties();
        let glyphs = rustybuzz::shape(&face, &[], buffer);
        glyphs
            .glyph_positions()
            .iter()
            .map(|p| f64::from(p.x_advance) * scale)
            .sum::<f64>()
            + text.chars().count().saturating_sub(1) as f64 * settings.tracking
    };
    let mut lines = vec![];
    for paragraph in text.split('\n') {
        let mut line = String::new();
        for word in paragraph.split_inclusive(char::is_whitespace) {
            let candidate = format!("{line}{word}");
            if settings.width > 0.
                && !line.is_empty()
                && measure(candidate.trim_end()) > settings.width
            {
                lines.push(line.trim_end().to_owned());
                line.clear();
            }
            line.push_str(word);
        }
        lines.push(line.trim_end().to_owned());
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(index, text)| {
            let width = measure(&text);
            let extra = if settings.width > 0. {
                settings.width - width
            } else {
                0.
            };
            let x = match settings.alignment {
                TextAlignment::Left => 0.,
                TextAlignment::Center => extra * 0.5,
                TextAlignment::Right => extra,
            };
            Line {
                text,
                x,
                baseline: f64::from(face.ascender()) * scale
                    + index as f64 * size * settings.line_height,
                width,
            }
        })
        .collect()
}
