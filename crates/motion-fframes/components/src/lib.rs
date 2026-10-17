//! # fframes-components
//!
//! An isolated, mix-and-match library of animated video components for
//! [fframes](https://github.com/dmtrKovalenko/fframes), ported from the Remotion component
//! library at <https://github.com/Remocn/remocn>.
//!
//! ## What it is
//!
//! Each component is **isolated**: it owns its own configuration struct, lives in its own module,
//! and is compiled in only when you ask for it. You then pick which ones you want, either at
//! compile time (Cargo features) or at render time (`--components`).
//!
//! ```no_run
//! use fframes_components::{ComponentsVideo, select};
//!
//! // Compile-time: only the enabled Cargo features exist in the catalog.
//! // Runtime: pick what actually renders.
//! let video = ComponentsVideo::from_spec("type-writer,text-reveal,glitch-cycle").unwrap();
//! println!("{}", video.timeline_report());
//! ```
//!
//! ## Selection
//!
//! | how | where | effect |
//! | --- | --- | --- |
//! | Cargo features | `Cargo.toml` | which components get **compiled** |
//! | `--components a,b` | the CLI | which compiled components **render**, and in what order |
//! | `--list` | the CLI | print every compiled component with its id and options |
//!
//! A component that is not compiled in does not exist: `selection()` reports it as unknown and
//! `--list` does not show it. That keeps unused components out of the binary.
//!
//! ## Components
//!
//! Every component's module carries its own `README.md` documenting every option with its type,
//! default and meaning. Start at [`catalog()`] for the list this build contains.
//!
//! ## Credit
//!
//! The animations, their timing constants, easing vocabulary and defaults are ported from the
//! Remotion component library by the **remocn** authors (<https://github.com/Remocn/remocn>, MIT).
//! This crate is a reimplementation against fframes' SVG + Rust API and is not affiliated with
//! Remotion. See `CREDITS.md`.

#![deny(missing_docs)]

pub mod blur;
pub mod core;
pub mod ctx;
pub mod meter;
pub mod motion;
pub mod text_spec;
pub mod ui;

pub mod components {
    //! One module per component, each behind its own Cargo feature.
    //!
    //! Turn on a component feature (or `full`) to compile a module in.

    /// Cinematic UI components, each behind its own feature.
    pub mod showcase;

    #[cfg(feature = "echo-stack")]
    pub mod echo_stack;
    #[cfg(feature = "glitch-cycle")]
    pub mod glitch_cycle;
    #[cfg(feature = "lines-to-text")]
    pub mod lines_to_text;
    #[cfg(feature = "soft-blur-in")]
    pub mod soft_blur_in;
    #[cfg(feature = "tracking-in")]
    pub mod tracking_in;
    #[cfg(feature = "word-flip")]
    pub mod word_flip;
}

mod catalog;

pub use crate::catalog::REGISTRY;
pub use crate::core::{
    CANVAS_FPS, CANVAS_HEIGHT, CANVAS_WIDTH, Canvas, CatalogEntry, Component, ComponentFactory,
    ComponentMeta, ComponentsVideo, Fonts, Horizontal, Landscape, Portrait, Slot, UnknownComponent,
    Vertical, catalog, find, selection,
};
pub use crate::ctx::{
    ComponentFrame, FaceMeter, FrameRef, LayoutCache, LineLayout, PlacedLine, PlacedWord,
    RenderCtx, TextMeters, centered_positions, char_centers, chars, escape_xml, font_weight_attr,
    interpolate3, layout_paragraph, svg_transform, words,
};
pub use crate::meter::TextMeter;
pub use crate::motion::{CubicBezier, Spring};
pub use crate::text_spec::TextSpec;

/// Build a landscape (16:9) video from a `--components` selection string.
///
/// A convenience alias for [`ComponentsVideo::from_spec`] on the [`Landscape`] canvas. For the
/// 9:16 canvas, annotate the binding instead:
///
/// ```no_run
/// use fframes_components::{ComponentsVideo, Vertical};
///
/// let tall: Vertical = ComponentsVideo::from_spec("text-messages").unwrap();
/// ```
pub fn video_from_spec(spec: &str) -> Result<Horizontal, UnknownComponent> {
    ComponentsVideo::from_spec(spec)
}

/// Build a portrait (9:16) video from a `--components` selection string.
pub fn vertical_video_from_spec(spec: &str) -> Result<Vertical, UnknownComponent> {
    ComponentsVideo::from_spec(spec)
}

/// Build components for a selection, in catalog order.
pub fn build_selection(spec: &str) -> Result<Vec<Box<dyn Component>>, UnknownComponent> {
    Ok(selection(spec)?.iter().map(|entry| entry.build()).collect())
}

/// Default sans family.
pub const EMBEDDED_FONT_FAMILY: &str = "DM Sans";
/// Embedded Inter family.
pub const INTER_FAMILY: &str = "Inter";
/// Embedded Montserrat family.
pub const MONTSERRAT_FAMILY: &str = "Montserrat";
/// Deterministic measurement from the same packaged faces supplied by Fold.
pub fn embedded_meters() -> TextMeters {
    let sans = include_bytes!("../../../../resources/fframes-components/DMSans-Medium.ttf").to_vec();
    let mut meters=TextMeters::from_bytes(Some(sans.clone()),Some(sans));
    let faces: &[(&str, &[u8])] = &[
        ("Inter-Bold", include_bytes!("../../../../resources/fframes-components/Inter-Bold.ttf")),
        ("Inter-Medium", include_bytes!("../../../../resources/fframes-components/Inter-Medium.ttf")),
        ("Inter-Regular", include_bytes!("../../../../resources/fframes-components/Inter-Regular.ttf")),
        ("Inter-SemiBold", include_bytes!("../../../../resources/fframes-components/Inter-SemiBold.ttf")),
        ("Montserrat-Black", include_bytes!("../../../../resources/fframes-components/Montserrat-Black.ttf")),
        ("Montserrat-Bold", include_bytes!("../../../../resources/fframes-components/Montserrat-Bold.ttf")),
        ("Montserrat-ExtraBold", include_bytes!("../../../../resources/fframes-components/Montserrat-ExtraBold.ttf")),
    ];
    for (label,bytes) in faces {if let Some(meter)=TextMeter::new(*label,bytes.to_vec()) {meters=meters.with_face(meter);}}
    meters
}
