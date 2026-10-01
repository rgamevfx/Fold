# Cache display results, not every working image

Accepted with the user: the SDR viewer hot cache stores display-transformed,
GPU-resident RGBA8 textures at the explicitly requested preview resolution. Image
operations still execute in their declared working space; export never reads the
viewer cache. This avoids retaining large linear RGBA16F/32F buffers for playback
and avoids image-file encoding/decoding on cache hits.

Use a byte-budgeted LRU, protect the displayed texture and submitted GPU work,
and schedule only the latest preview request. Keys describe source content,
operations, exact time, resolution/quality, and view transform—not the whole
project revision. Draw UI overlays after the cached image. Apply the output
transfer exactly once. CPU evaluation/upload remains the correctness fallback;
GPU-resident caching does not imply GPU evaluation or hardware video decoding.

Start with a 256 MiB GPU texture budget and explicit full/half/quarter resolution.
No automatic full-resolution storage for draft previews. Decode and intermediate
caches are separate from the presentation cache; only retain them when reuse
justifies their memory, preferring decoder-native planes for decoded video.
BC7 may later provide a lossy secondary tier after encoding-latency, quality, and
GPU-support measurements. Persistent disk caches, proxies, and inter-frame video
preview caches are not part of this first implementation. Temporary pinned media
copies are source ownership, not a rendered-frame cache.
