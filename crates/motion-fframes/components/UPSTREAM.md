# fframes-components

Vendored subset of https://github.com/Bilaal0x/fframes-components at
`ae04b1be6596b0fc51e18280392ff6ee77629c47` (MIT). See LICENSE and CREDITS.md.

Fold retains ten selected components and their shared source, fonts, and demo
images. The manifest and module/catalog declarations omit unselected components
and the CLI. The fframes dependency uses Fold’s renderer revision. The media macro is replaced by explicit font measurement bytes; Fold owns image
resolution. Logo Assemble uses its `logo_src` image instead of the upstream monogram
stand-in, retaining its geometry and timing. The obsolete embedded-image test is
removed. Other animation
implementations are upstream source; Fold integration lives in `../src/presets.rs`.
Source assets are in `resources/fframes-components` (repository root).
Fonts use the SIL Open Font License, reproduced with these assets.
