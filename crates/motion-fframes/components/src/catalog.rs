//! The compile-time component registry.
//!
//! Every entry is gated on its own Cargo feature, so a build contains exactly the components you
//! asked for. The list order is the default playback order and the order `--components` uses.
//!
//! Each entry is emitted inside its own block expression. That scoping matters: without it every
//! entry would define a module-level `ENTRY` static, so enabling a second component feature would
//! fail with "the name `ENTRY` is defined multiple times".

use crate::core::{CatalogEntry, Component};

macro_rules! entries {
    ($($feature:literal => $ty:path;)*) => {
        /// Every component compiled into this build, in default playback order.
        pub static REGISTRY: &[&CatalogEntry] = &[
            $(
                #[cfg(feature = $feature)]
                {
                    /// The catalog entry for this component.
                    static ENTRY: CatalogEntry = CatalogEntry {
                        meta_fn: <$ty>::meta_static,
                        build_fn: || Box::new(<$ty>::default()),
                    };
                    &ENTRY
                },
            )*
        ];
    };
}

entries! {
    "soft-blur-in" => crate::components::soft_blur_in::SoftBlurIn;
    "tracking-in" => crate::components::tracking_in::TrackingIn;
    "echo-stack" => crate::components::echo_stack::EchoStack;
    "word-flip" => crate::components::word_flip::WordFlip;
    "glitch-cycle" => crate::components::glitch_cycle::GlitchCycle;
    "lines-to-text" => crate::components::lines_to_text::LinesToText;
    "cinematic-text-swap" => crate::components::showcase::text_swap::TextSwap;
    "word-gather" => crate::components::showcase::word_gather::WordGather;
    "text-highlight" => crate::components::showcase::text_highlight::TextHighlight;
    "logo-assemble" => crate::components::showcase::logo_assemble::LogoAssemble;
}

// Assert the shape the registry relies on, so a missing `meta_static` or `Default` is a clear
// error rather than a confusing one at the macro expansion site.
#[allow(dead_code)]
fn _assert_component_shape<T: Component + Default>() {}
