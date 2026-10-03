# Cache display results, not every working image

Accepted with the user: the SDR viewer hot cache stores display-transformed,
GPU-resident display textures at the explicitly requested preview resolution.
The user-approved phase-18 refinement uses RGBA8 for immediate presentation and
BC7 for background retention when the device and image-quality checks permit. Image
operations still execute in their declared working space; export never reads the
viewer cache. This avoids retaining large linear RGBA16F/32F buffers for playback
and avoids image-file encoding/decoding on cache hits.

Use a byte-budgeted LRU, protect the displayed texture and submitted GPU work,
and keep bounded, independent viewer demands. Keys describe source content,
operations, exact time, resolution/quality, and view transform—not the whole
project revision. Draw UI overlays after the cached image. Apply the output
transfer exactly once. CPU evaluation/upload remains the correctness fallback;
GPU-resident caching does not imply GPU evaluation or hardware video decoding.

Start with a shared 256 MiB retention budget. Full/half/quarter quality is capped
to the fitted viewer image area in physical pixels, never upscaled past document
resolution. Panel resize, DPI, quality, and display processing affect cache keys.
Do not automatically retain source-resolution images for a small viewer. Decode and intermediate
caches are separate from the presentation cache; only retain them when reuse
justifies their memory, preferring decoder-native planes for decoded video.
BC7 encoding runs on a worker using the shared GPU service, after an image is no
longer held for presentation. It must not gate the current frame. Compressed
textures are sampled directly, with edge padding excluded by UVs; they are not
video-codec streams. Count staging, scratch, and in-flight leases in the host
budget. Preserve opaque display alpha and retain RGBA8 if any channel differs
by more than 24 SDR codes. This quality fallback deliberately favors the image
over guaranteed compression of every frame; measurements and limits belong in
`docs/phase-18.md`. Persistent disk caches, proxies, and inter-frame video
preview caches are not part of this implementation. Temporary pinned media
copies are source ownership, not a rendered-frame cache.
