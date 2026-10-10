# Compositor channel controls — 10 October 2026

Implemented:
- Merge v3: compact operation, A channels, B channels, output coverage, extra-channel toggle, mask/invert and animated/reference-capable mix. Input coverage does not alter the original B image used for masked/mixed bypass. Missing layers on A do not erase B-only layers.
- Cull Channels v1: replaces RemoveChannels, with Remove/Keep and four independent persisted channel selections. Layer presets and individual channels share the same selector; all four selections combine before channel removal/retention.
- Multiply Alpha/ApplyMask is deleted from registration, projection, lowering, UI and its dedicated regression. The animation example uses Crop. General render masking and Premult are independently used and retained.
- No compatibility aliases or new tooltips.

Validation:
- Optimized desktop release build passed; gallery regenerated through public discovery/propose/inspect/apply/save at revision 3, with 23 current operators. Merge v3 and Cull Channels appear first; removed operations are absent.
- `cargo check -p fold-app --features desktop --all-targets`: passed.
- Compositor UI/library: 12 passed, one native capture test ignored in ordinary runs.
- `cargo test -p fold-compositor --features ui --test channels`: 12 passed. Includes rendered Merge input coverage/mix, B-only extra layers, and Cull persistence/discovery/rendering.
- Existing native capture harness passed using llvmpipe Vulkan. Actual inspector captures reviewed at 420px and 280px: [Merge normal](merge-normal.png), [Merge narrow](merge-narrow.png), [Cull normal](cull-normal.png), [Cull narrow](cull-narrow.png).

Limits: Merge bounds are the fixed render frame, shown read-only; union/intersection/A/B data-window evaluation is not implemented. Read/Write/Shuffle prototype alignment remains outstanding. Roto/Mask changes are explicitly deferred by the user. Physical artist acceptance remains pending. The compositor-only feature build emits existing unused-field/method warnings in render GPU YUV; the full desktop check is clean.
