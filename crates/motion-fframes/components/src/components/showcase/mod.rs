//! Cinematic UI components.
//!
//! Each module is gated by its own Cargo feature and shares primitives from [`crate::ui`].

#[cfg(feature = "logo-assemble")]
pub mod logo_assemble;
#[cfg(feature = "text-highlight")]
pub mod text_highlight;
#[cfg(feature = "cinematic-text-swap")]
pub mod text_swap;
#[cfg(feature = "word-gather")]
pub mod word_gather;
