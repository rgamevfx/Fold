# Viewer regions and sampling

Tasks 27–29 implement navigable viewers, stable overlay feedback, zoom-aware region rendering and antialiased source scaling. They do not close the pending delivery/hardening or hardware playback checkpoints.

## User behavior

Left-background-drag and middle-drag pan; the wheel zooms about the pointer. Authoring handles own left-drag when hit. **Fit** / **F** centers the image and follows resizing until manual navigation. Image, tools and notifications are clipped to the canvas; notifications do not change its size.

Zoom requests the visible image rectangle at the current physical-pixel scale and explicit Full/Half/Quarter quality. Resolution stops increasing at native source resolution. Fully offscreen images stop requesting work. While a replacement renders, the held tile stays in its original image-space position and an overlay indicates that the preview is updating. Newly exposed areas may remain empty until the replacement arrives; no stale tile is stretched over the image.

New viewers default to Full; previously saved quality choices remain unchanged. The context menu exposes **Display sampling → Smooth / Pixel exact**. Smooth uses the existing linear backend sampler. Pixel exact switches to the backend's nearest sampler for the image, restoring linear sampling before other UI drawing. This preference is saved per viewer; navigation is transient. Neither edits the project or changes export settings.

## Region execution

`PreviewKey` and the feature-neutral `SceneRequest` carry an optional integer region in the full raster at the requested resolution. Presentation cache keys, cancellation and review compatibility include that region; review memory admission uses cropped dimensions. Export explicitly requests no crop.

The renderer walks dependencies backward, expanding blur support and inverse-mapping transform footprints, including interpolation support. It unions the required rectangles into one conservative working rectangle, translates operator and vector coordinates into it, and runs the existing CPU/GPU image semantics there. The final scene crop is extracted before display conversion and caching. This is bounded regional execution, not a per-operator tile cache. A widely spread dependency footprint may still require a full working frame.

Source formats/codecs remain an explicit full-native-frame fallback. Native source reconstruction and input color conversion can require the full source; retained EXR working inputs and native video planes remain under their existing shared budgets. Downstream effects and retained display results use regional allocations. GPU sources, region extraction and display conversion carry completion/validation dependencies and explicit leases. None of this work runs on the UI thread.

## Sampling quality

The installed dear-imgui-wgpu 0.18 renderer already defaults to linear sampling. Inspection found nearest-neighbor source reduction before that final sampler. A real FFmpeg stripe fixture reproduced the consequence: reducing alternating black/white pixels yielded white (1.0) instead of their linear-light area average (0.5).

Source resizing now occurs after input conversion in premultiplied working color. Two separable passes integrate pixel area when reducing and interpolate linearly when enlarging. Scratch rows are restricted to the requested region's source footprint. The CPU and Vulkan paths share the policy; native-size sampling is unchanged. This avoids excessive scene resolution, arbitrary sharpening and ringing. Explicit authored transform filter selections are preserved.

Backend references were inspected in the installed dear-imgui-rs/dear-imgui-wgpu sources, including sampler callbacks and default bind groups, alongside the [official Dear ImGui image-display guidance](https://github.com/ocornut/imgui/wiki/Image-Loading-and-Displaying-Examples). No dependency patch or renderer replacement is needed.

## Validation and limits

- CPU regions match full-frame crops through Gaussian/box blur, nearest/linear/cubic transforms, crop, grade, mask, opacity, over, shuffle and mix, including image edges and one-pixel regions. A separate vector regression preserves fractional curve coverage after region translation. Invalid regions and cancellation are rejected.
- Vulkan regional results match full-frame results; display textures retain crop dimensions and scene evaluation performs no working-image readback.
- On the software Vulkan adapter (llvmpipe), a 640×360 solid/blur/grade graph used 11,060,724 bytes peak tracked allocation full-frame versus 947,168 bytes for a 120×80 region plus blur padding: about 91% less. These are host allocation counters before test readback, not device-driver memory or hardware frame-rate measurements.
- Stripe energy, odd sizes, mixed-axis reduction/enlargement, regional reconstruction and native pixel identity pass. The explicit ACES/video CPU–Vulkan comparison also passes with the pinned OCIO runtime.
- UI tests cover normal/narrow headers, stable error layout, both pan buttons, handle priority, F, zoom anchoring, physical-pixel demand, quality and offscreen navigation. The sampling preference round-trips through workspace storage.
- The render crate's non-opt-in tests and the UI library suite pass (UI: 67 passed, 2 pre-existing GPU tests ignored). Targeted Vulkan/FFmpeg/ACES region and sampling tests pass (8 tests). Application all-target desktop compilation passes. Eight preview-worker routing/cancellation checks and two delivery-selection/region-isolation checks pass. The focused viewer, cache/review and workspace-persistence checks also pass after final integration changes.
- Concurrent software-Vulkan test initialization crashed once; GPU tests now serialize adapter initialization, and the complete region/sampling run passed serialized. Native desktop visual/gesture/DPI behavior and hardware playback performance remain unverified. Workspace-wide tests, packaging and Clippy are deferred to the integration checkpoint.

### User visual review

The updated desktop opened the Chroma Parade text-following-path project using the NVIDIA GeForce GTX 1070 Vulkan adapter. The user reviewed it and reported that it looks good. This confirms visual acceptance of that demo; comprehensive gesture/DPI coverage and hardware playback measurements remain pending.
