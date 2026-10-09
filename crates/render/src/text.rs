//! Fold-owned CPU text preparation. No system discovery, project state or GPU types.
pub mod extrude;
mod font;
pub mod mesh;
pub mod outlines;
pub use font::{FontSource, LoadError};

use sha2::{Digest, Sha256};
use std::{ops::Range, sync::Arc};

/// Positions and advances use the requested layout units, with Y increasing down.
#[derive(Clone, Debug, PartialEq)]
pub struct Glyph {
    pub id: u32,
    pub position: [f32; 2],
    pub advance: f32,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Cluster {
    /// UTF-8 byte range in the original text, not a glyph or character index.
    pub text: Range<usize>,
    pub glyphs: Range<usize>,
    pub advance: f32,
    pub word_boundary: bool,
    pub ligature_continuation: bool,
}
#[derive(Clone, Debug, PartialEq)]
pub struct Word {
    /// Non-whitespace Unicode word-boundary segment, in logical UTF-8 order.
    /// Punctuation follows the shaper's segmentation, not glyph-array order.
    pub text: Range<usize>,
    pub clusters: Vec<usize>,
    pub glyphs: Vec<usize>,
    /// Center of positioned glyph origins/advance endpoints, including baseline
    /// offsets. This is an advance-center pivot, not a visible-ink bounding box.
    pub pivot: [f32; 2],
}
#[derive(Clone, Debug, PartialEq)]
pub struct Run {
    pub line: usize,
    pub text: Range<usize>,
    pub clusters: Range<usize>,
    pub glyphs: Range<usize>,
    pub face_index: u32,
    pub font_size: f32,
    pub normalized_coords: Vec<i16>,
    pub rtl: bool,
    pub baseline: f32,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Diagnostic {
    MissingGlyph { text: Range<usize> },
}
/// One retained layout is intended to feed drawing, outlines and inspection.
/// Those rendering consumers are qualified separately from shaping.
#[derive(Clone, Debug, PartialEq)]
pub struct Layout {
    pub line_count: usize,
    pub identity: [u8; 32],
    pub text: Arc<str>,
    pub font: Arc<[u8]>,
    pub font_hash: [u8; 32],
    pub width: f32,
    pub height: f32,
    pub runs: Vec<Run>,
    pub clusters: Vec<Cluster>,
    pub words: Vec<Word>,
    pub glyphs: Vec<Glyph>,
    pub diagnostics: Vec<Diagnostic>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Error {
    InvalidFont,
    MissingFamily,
    InvalidMetrics,
    Budget,
    FontSubstitution,
    UnsupportedSynthesis,
}
/// Reusable worker-owned shaping context for an explicitly supplied font resource.
/// Font selection never consults machine-local fonts or silently substitutes a face.
pub struct Service {
    fonts: parley::FontContext,
    context: parley::LayoutContext<()>,
    font: Arc<[u8]>,
    font_hash: [u8; 32],
    family: String,
}
impl Service {
    pub fn new(font: Vec<u8>, family: &str) -> Result<Self, Error> {
        Self::from_font(FontSource::from_bytes(font)?, family)
    }
    /// Creates a shaping context from an owned source snapshot, without reopening its path.
    pub fn from_font(font: FontSource, family: &str) -> Result<Self, Error> {
        if family.len() > 1024 {
            return Err(Error::Budget);
        }
        let mut fonts = parley::FontContext::new();
        if fonts
            .collection
            .register_fonts(
                parley::fontique::Blob::new(Arc::new(font.bytes.clone())),
                None,
            )
            .is_empty()
        {
            return Err(Error::InvalidFont);
        }
        if fonts.collection.family_by_name(family).is_none() {
            return Err(Error::MissingFamily);
        }
        Ok(Self {
            font_hash: font.hash,
            font: font.bytes,
            family: family.into(),
            fonts,
            context: parley::LayoutContext::new(),
        })
    }
    /// Uniform style with optional paragraph width. Font size is in layout units.
    pub fn layout(
        &mut self,
        text: &str,
        size: f32,
        width: Option<f32>,
    ) -> Result<Arc<Layout>, Error> {
        if text.len() > 64 * 1024 {
            return Err(Error::Budget);
        }
        if !size.is_finite() || size <= 0. || width.is_some_and(|w| !w.is_finite() || w <= 0.) {
            return Err(Error::InvalidMetrics);
        }
        let mut builder = self
            .context
            .ranged_builder(&mut self.fonts, text, 1., false);
        builder.push_default(parley::StyleProperty::FontSize(size));
        builder.push_default(parley::StyleProperty::FontStack(
            self.family.as_str().into(),
        ));
        let mut shaped = builder.build(text);
        shaped.break_all_lines(width);
        let mut hash = Sha256::new();
        hash.update(b"fold.text.layout.v1/parley.0.7.0\0");
        hash.update(self.font_hash);
        hash.update((self.family.len() as u64).to_le_bytes());
        hash.update(self.family.as_bytes());
        hash.update(size.to_bits().to_le_bytes());
        hash.update([u8::from(width.is_some())]);
        hash.update(width.unwrap_or(0.).to_bits().to_le_bytes());
        hash.update(text.as_bytes());
        let mut result = Layout {
            line_count: 0,
            identity: hash.finalize().into(),
            text: text.into(),
            font: self.font.clone(),
            font_hash: self.font_hash,
            width: shaped.width(),
            height: shaped.height(),
            runs: vec![],
            clusters: vec![],
            words: vec![],
            glyphs: vec![],
            diagnostics: vec![],
        };
        let mut checked_fonts = std::collections::BTreeSet::new();
        for (line_index, line) in shaped.lines().enumerate() {
            result.line_count += 1;
            let mut x = line.metrics().offset;
            for run in line.runs() {
                if checked_fonts.insert(run.font().data.id())
                    && run.font().data.as_ref() != self.font.as_ref()
                {
                    return Err(Error::FontSubstitution);
                }
                if run.synthesis().any() {
                    return Err(Error::UnsupportedSynthesis);
                }
                let first_cluster = result.clusters.len();
                let first_glyph = result.glyphs.len();
                let mut pen = x;
                for cluster in run.visual_clusters() {
                    let first = result.glyphs.len();
                    for glyph in cluster.glyphs() {
                        if result.glyphs.len() >= 65536 {
                            return Err(Error::Budget);
                        }
                        if glyph.id == 0 {
                            result.diagnostics.push(Diagnostic::MissingGlyph {
                                text: cluster.text_range(),
                            });
                        }
                        result.glyphs.push(Glyph {
                            id: glyph.id,
                            position: [pen + glyph.x, line.metrics().baseline + glyph.y],
                            advance: glyph.advance,
                        });
                        pen += glyph.advance;
                    }
                    result.clusters.push(Cluster {
                        text: cluster.text_range(),
                        glyphs: first..result.glyphs.len(),
                        advance: cluster.advance(),
                        word_boundary: cluster.is_word_boundary(),
                        ligature_continuation: cluster.is_ligature_continuation(),
                    });
                }
                result.runs.push(Run {
                    line: line_index,
                    text: run.text_range(),
                    clusters: first_cluster..result.clusters.len(),
                    glyphs: first_glyph..result.glyphs.len(),
                    face_index: run.font().index,
                    font_size: run.font_size(),
                    normalized_coords: run.normalized_coords().to_vec(),
                    rtl: run.is_rtl(),
                    baseline: line.metrics().baseline,
                });
                x += run.advance();
            }
        }
        if !result.width.is_finite()
            || !result.height.is_finite()
            || result
                .glyphs
                .iter()
                .any(|g| g.position.iter().any(|v| !v.is_finite()) || !g.advance.is_finite())
        {
            return Err(Error::InvalidMetrics);
        }
        result.words = words(&result)?;
        Ok(Arc::new(result))
    }
}

fn words(layout: &Layout) -> Result<Vec<Word>, Error> {
    let mut logical: Vec<_> = (0..layout.clusters.len()).collect();
    logical.sort_by_key(|&i| (layout.clusters[i].text.start, layout.clusters[i].text.end));
    let mut words = Vec::<Word>::new();
    let mut separated = true;
    for index in logical {
        let cluster = &layout.clusters[index];
        let text = layout
            .text
            .get(cluster.text.clone())
            .ok_or(Error::InvalidMetrics)?;
        if text.chars().all(char::is_whitespace) {
            separated = true;
            continue;
        }
        if separated || cluster.word_boundary {
            words.push(Word {
                text: cluster.text.clone(),
                clusters: vec![],
                glyphs: vec![],
                pivot: [0.; 2],
            });
        }
        separated = false;
        let word = words.last_mut().unwrap();
        word.text.end = word.text.end.max(cluster.text.end);
        word.clusters.push(index);
        word.glyphs.extend(cluster.glyphs.clone());
    }
    for word in &mut words {
        let mut low = [f64::INFINITY; 2];
        let mut high = [f64::NEG_INFINITY; 2];
        for &index in &word.glyphs {
            let glyph = &layout.glyphs[index];
            for point in [
                glyph.position.map(f64::from),
                [
                    f64::from(glyph.position[0]) + f64::from(glyph.advance),
                    f64::from(glyph.position[1]),
                ],
            ] {
                for axis in 0..2 {
                    low[axis] = low[axis].min(point[axis]);
                    high[axis] = high[axis].max(point[axis]);
                }
            }
        }
        if !word.glyphs.is_empty() {
            word.pivot = [0, 1].map(|axis| ((low[axis] + high[axis]) * 0.5) as f32);
            if word.pivot.iter().any(|v| !v.is_finite()) {
                return Err(Error::InvalidMetrics);
            }
        }
    }
    Ok(words)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn logical_words_retain_cluster_mapping_across_wrapping_and_bidi() {
        let mut service = service();
        for (source, expected) in [
            ("MAKE IT MOVE", vec!["MAKE", "IT", "MOVE"]),
            ("office e\u{301}", vec!["office", "e\u{301}"]),
            ("Hello, can't!", vec!["Hello", ",", "can't", "!"]),
            ("Latin אבג mixed", vec!["Latin", "אבג", "mixed"]),
            ("A\n\nB", vec!["A", "B"]),
            (" \t\n", vec![]),
        ] {
            for width in [None, Some(45.)] {
                let layout = service.layout(source, 32., width).unwrap();
                let actual: Vec<_> = layout
                    .words
                    .iter()
                    .map(|word| &source[word.text.clone()])
                    .collect();
                assert_eq!(
                    actual, expected,
                    "word segmentation for {source:?}, width {width:?}"
                );
                let mut seen = std::collections::BTreeSet::new();
                for word in &layout.words {
                    assert!(word.pivot.iter().all(|v| v.is_finite()));
                    for &index in &word.clusters {
                        assert!(seen.insert(index), "cluster belongs to only one word");
                        let cluster = &layout.clusters[index];
                        assert!(
                            cluster.text.start >= word.text.start
                                && cluster.text.end <= word.text.end
                        );
                        assert!(
                            cluster
                                .glyphs
                                .clone()
                                .all(|glyph| word.glyphs.contains(&glyph))
                        );
                    }
                    for &index in &word.glyphs {
                        let glyph = &layout.glyphs[index];
                        assert!(glyph.position.iter().all(|v| v.is_finite()));
                    }
                }
                for (index, cluster) in layout.clusters.iter().enumerate() {
                    assert_eq!(
                        seen.contains(&index),
                        !source[cluster.text.clone()]
                            .chars()
                            .all(char::is_whitespace)
                    );
                }
            }
        }
        let single = service.layout("MAKE", 32., None).unwrap();
        assert!((single.words[0].pivot[0] - single.width * 0.5).abs() < 0.0001);
        assert_eq!(single.words[0].pivot[1], single.glyphs[0].position[1]);
    }
    fn service() -> Service {
        Service::new(
            include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
            "Noto Sans",
        )
        .unwrap()
    }
    #[test]
    fn normalized_glyphs_match_shaper_and_retain_logical_mapping() {
        let mut service = service();
        for text in [
            "MAKE IT MOVE O B 8",
            "office e\u{301}\nSecond line",
            "Latin אבג mixed",
            "A\n\nB",
            "",
        ] {
            let normalized = service.layout(text, 32., Some(180.)).unwrap();
            let mut builder = service
                .context
                .ranged_builder(&mut service.fonts, text, 1., false);
            builder.push_default(parley::StyleProperty::FontSize(32.));
            builder.push_default(parley::StyleProperty::FontStack("Noto Sans".into()));
            let mut reference = builder.build(text);
            reference.break_all_lines(Some(180.));
            for line in reference.lines() {
                for run in line.runs() {
                    assert!(
                        std::ptr::eq(run.font().data.as_ref(), normalized.font.as_ref()),
                        "shaper and retained layout share the immutable font bytes"
                    );
                }
            }
            let glyphs: Vec<_> = reference
                .lines()
                .flat_map(|line| line.items())
                .flat_map(|item| match item {
                    parley::PositionedLayoutItem::GlyphRun(run) => run
                        .positioned_glyphs()
                        .map(|g| Glyph {
                            id: g.id,
                            position: [g.x, g.y],
                            advance: g.advance,
                        })
                        .collect::<Vec<_>>(),
                    _ => panic!("No inline boxes in plain text"),
                })
                .collect();
            assert_eq!(normalized.glyphs, glyphs);
            assert_eq!(normalized.width, reference.width());
            assert_eq!(normalized.height, reference.height());
            assert_eq!(normalized.line_count, reference.lines().count());
            let mut mapped = 0;
            for cluster in &normalized.clusters {
                assert!(text.get(cluster.text.clone()).is_some());
                assert_eq!(cluster.glyphs.start, mapped);
                mapped = cluster.glyphs.end;
            }
            assert_eq!(mapped, normalized.glyphs.len());
            for run in &normalized.runs {
                assert!(text.get(run.text.clone()).is_some());
                assert_eq!(run.face_index, 0);
            }
        }
    }
    #[test]
    fn content_identity_and_font_ownership_survive_context_reuse() {
        let mut service = service();
        let original = service.layout("MAKE IT MOVE", 140., None).unwrap();
        assert!(original.diagnostics.is_empty());
        assert_eq!(
            original,
            service.layout("MAKE IT MOVE", 140., None).unwrap()
        );
        for variant in [
            service.layout("MOVE IT", 140., None).unwrap(),
            service.layout("MAKE IT MOVE", 70., None).unwrap(),
            service.layout("MAKE IT MOVE", 140., Some(500.)).unwrap(),
        ] {
            assert_ne!(original.identity, variant.identity);
            assert!(Arc::ptr_eq(&original.font, &variant.font));
        }
        drop(service);
        assert_eq!(
            original.font_hash,
            <[u8; 32]>::from(Sha256::digest(&original.font))
        );
        assert!(!original.glyphs.is_empty());
    }
    #[test]
    fn missing_fonts_glyphs_and_invalid_requests_are_explicit() {
        assert!(matches!(
            Service::new(vec![0; 32], "Missing"),
            Err(Error::InvalidFont)
        ));
        assert!(matches!(
            Service::new(
                include_bytes!("../../../resources/fonts/NotoSans-Regular.ttf").to_vec(),
                "Missing"
            ),
            Err(Error::MissingFamily)
        ));
        let mut service = service();
        let layout = service.layout("A🛸B", 32., None).unwrap();
        assert!(layout.diagnostics.iter().any(
            |d| matches!(d, Diagnostic::MissingGlyph { text } if &layout.text[text.clone()] == "🛸")
        ));
        for size in [0., -1., f32::NAN, f32::INFINITY] {
            assert_eq!(
                service.layout("Test", size, None),
                Err(Error::InvalidMetrics)
            );
        }
        assert_eq!(
            service.layout("Test", 32., Some(0.)),
            Err(Error::InvalidMetrics)
        );
        assert_eq!(
            service.layout(&"A".repeat(65537), 32., None),
            Err(Error::Budget)
        );
        assert!(service.layout("", 32., None).unwrap().glyphs.is_empty());
    }
}
