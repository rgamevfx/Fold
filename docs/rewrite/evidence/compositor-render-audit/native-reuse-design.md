# Native HEVC allocation/import reuse: ownership design

Research only, 10 October 2026. No runtime edits or performance claim. Reviewed `crates/native-video/src/{gpu,helper,readiness}.rs`, `crates/render/src/gpu/native_video.rs`, and pinned wgpu 27.0.1 / wgpu-hal 27.0.4. The complete change is larger than adding a free-list. Recommendation: defer this optimization until the complete gateway and helper lifecycle can be implemented and qualified together. Do not introduce an extra Vulkan frame copy simply to simplify the pool unless measured import/allocation savings exceed that cost.

## Why the present API cannot safely recycle

`PendingBuffer::acquire` transfers the raw allocation into `Buffer::from_raw_managed`; `ReadyBuffer.buffer` is public. `NativeInput` separately retains a reservation, and acquire submission retains a cloned buffer/reservation until completion. This correctly supports today's immutable allocations. But a caller can retain `wgpu::Buffer`, a bind group, or an encoded but unsubmitted command buffer after dropping `NativeInput`. `Arc::strong_count(input) == 1`, cache eviction, or the last known submission completing therefore does **not** prove the resource can be overwritten. Making the field private and returning `&wgpu::Buffer` still permits a clone; exposing a bind group or binding resource has the same problem. [Current native allocation API](../../../../crates/native-video/src/gpu.rs), [render cache](../../../../crates/render/src/gpu/native_video.rs)

Pinned HAL rules matter: `from_raw_managed` transfers exclusive buffer/backing-memory ownership until HAL cleanup. HAL destruction destroys `VkBuffer` and frees the supplied memory. `from_raw` omits backing-memory ownership, but **still destroys the VkBuffer**. Neither buffer constructor offers a destruction callback or a safe reclaim-to-raw operation. A texture's optional drop callback does not imply buffers have one. Do not drop a managed import and then reuse its saved raw handles. [Pinned HAL buffer constructors](https://github.com/gfx-rs/wgpu/blob/v27.0.1/wgpu-hal/src/vulkan/mod.rs), [HAL destruction implementation](https://github.com/gfx-rs/wgpu/blob/v27.0.1/wgpu-hal/src/vulkan/device.rs) (also verified against local registry 27.0.4).

`Buffer::as_hal` supplies a destruction-lock guard, not a GPU fence or proof of unique ownership. It forbids destruction unless no other reference/use remains. `CommandEncoder::as_hal_mut` permits native recording inside its callback but forbids simultaneously using the wgpu encoder or ending/destroying its command buffer. These APIs are interop escape hatches whose obligations stay inside the native module. [Pinned buffer API](https://github.com/gfx-rs/wgpu/blob/v27.0.1/wgpu/src/api/buffer.rs), [encoder API](https://github.com/gfx-rs/wgpu/blob/v27.0.1/wgpu/src/api/command_encoder.rs)

## Implementable safe boundary

A reusable source must be opaque from allocation through encoding and submission. Recommended types are conceptual, not a finished public API:

- `NativePool` owns slots, the consuming device/queue identity, bounded accounting and helper session identity.
- `NativeFrame` is a cloneable immutable lease for a specific `(slot, generation)` with metadata only. It cannot expose a wgpu buffer, raw handle, bind group, binding resource or callback receiving those objects.
- `NativeReadBatch` owns the encoded source-read command buffers plus all their frame leases. It has no extraction method. Its consumption through the pool's submit gateway moves leases into the completion callback; dropping an unsubmitted batch drops its commands before its leases.
- The native module records the source-reading operation itself. A narrow YUV-to-working-output conversion method can accept validated parameters and a destination texture, build private bindings and return `NativeReadBatch`. A public method that accepts an arbitrary caller encoder is insufficient: its source-reading command buffer could outlive the lease. Similarly, a closure with `&mut CommandEncoder` can move/replace that encoder.
- Other already encoded scene commands may be supplied to the gateway as explicit before/after batches and submitted in the same `Queue::submit` call. The output texture can use ordinary wgpu ownership because it does not alias the reusable source. This adds a command-buffer boundary, not necessarily another queue submission; benchmark its cost.

This moves native source consumption behind the ownership boundary, but need not move the whole compositor. If moving YUV conversion is rejected, the smaller safe operation is a private source-to-ordinary-buffer copy followed by existing YUV conversion; it adds a device copy and must demonstrate a net win. Returning a general buffer with an informal “keep this lease alive” rule is not an acceptable safe API.

The current managed import must not become a means of borrowing/reclaiming ownership between generations. One viable implementation is a long-lived private slot owning its allocation, with HAL-imported buffer lifetime wholly contained by the slot and all external access serialized through the interop owner. Using caller-owned backing (`from_raw`) makes that ownership explicit, but requires a carefully ordered shutdown: no external writer, all native-read/release work complete, destroy the wgpu buffer, and only then release backing allocation/accounting. A persistent managed import instead requires confirming that its exclusive-ownership contract covers the intended externally synchronized access; do not assume public clone tracking solves that obligation. This is an implementation review gate, not an established qualification.

## Slot lifecycle and synchronization

Use an explicit generation-bearing state machine:

`ExternalIdle → CudaWriting → CudaComplete → VulkanAcquireQueued → Readable → VulkanReleaseQueued → ExternalIdle`.

`Readable` includes cached leases, prepared/unsubmitted read batches and submitted consumers. Recycling starts only when all generation leases are gone and all registered read completions have retired. A slot-state lock must make lease acquisition and the transition to release mutually exclusive. A generation cannot be resurrected by an old handle.

For a slot previously owned by Vulkan, record an explicit release from the consuming queue family to `VK_QUEUE_FAMILY_EXTERNAL`, covering the actual prior read stages/accesses and buffer range. Submit it after all reads and wait for its completion before authorizing CUDA overwrite. Preserve the host-mediated handshake initially. CUDA completion acknowledgment follows stream completion. Then record the matching external-to-Vulkan acquire before any read. A host wait does not replace ownership transfer. Fresh first-use ownership requires its own documented initialization path. [Vulkan queue-family ownership and external synchronization specification](https://docs.vulkan.org/spec/latest/chapters/synchronization.html#synchronization-queue-transfers)

Keep wgpu's tracked buffer usages consistent via `transition_resources` as well as native ownership barriers; raw HAL commands do not automatically tell wgpu which resource to retain. The pool/gateway explicitly retains slot and exact reservation through acquire, read and release completion. Never invoke eviction callbacks while holding HAL destruction guards. [Current acquire implementation](../../../../crates/native-video/src/gpu.rs), [wgpu transition API](https://github.com/gfx-rs/wgpu/blob/v27.0.1/wgpu/src/api/command_encoder.rs)

Pinned wgpu 27 provides queue-level `on_submitted_work_done`, not newer encoder-level completion helpers. The submit gateway registers completion while retaining ownership; it cannot rely on an API from newer wgpu docs. Avoid calling pool mutation or blocking helper operations from completion callbacks: enqueue retirement notifications and process them on the owning worker. [Pinned queue implementation](https://github.com/gfx-rs/wgpu/blob/v27.0.1/wgpu/src/api/queue.rs)

## Persistent CUDA import protocol

The helper currently imports an FD, maps it, clears/copies planes, synchronizes its stream, unmaps and destroys the import on every request. Reuse needs an explicit bounded helper-side slot table, keyed by session identity and allocation ID rather than file descriptor integers. Registration transfers the FD once and validates device UUID, exact allocation bytes, dedicated-allocation flag and plane layout. Write requests identify slot plus monotonic generation; acknowledgments identify both. Unregister drains outstanding copy work, unmaps, destroys the CUDA import and acknowledges retirement before Vulkan backing memory is freed. [Current helper](../../../../crates/native-video/src/helper.rs), [transport](../../../../crates/native-video/src/lib.rs)

Keep the engine-owned copy: it releases scarce codec surfaces before downstream processing/cache retention. Remove the full-buffer memset only if all shader-accessible bytes, including alignment padding, are proved initialized by copy or targeted padding initialization; this is independent of pooling.

## Failure, cancellation and teardown

- Before issuing a writer ticket, cancellation returns an idle slot directly. After issuing one, cancellation must drain or terminate/reap the helper before reuse/free. An unanswered request is not proof CUDA stopped.
- A duplicate, stale or mismatched acknowledgment poisons that generation; never publish or recycle it based on timeout alone.
- Submission errors, device loss and helper restart make outstanding slots unavailable. Retire accounting and memory only through a teardown path that establishes no external writer and no executing device use; never put uncertain slots back on a free-list.
- Bound registered helper imports and total actual Vulkan allocation sizes, including idle slots. Cache eviction releases frame leases, not necessarily allocation bytes. Pressure trimming must unregister idle imports and destroy slots; it cannot evict live/encoded/submitted generations.
- A destructor must not require unbounded GPU/helper waiting. Owning services perform explicit bounded shutdown; failed drain poisons and routes to supervised device/helper teardown. Keep pending resources owned throughout that teardown.

## Qualification before claiming a safe optimization

Tests must retain a frame through cache eviction; retain encoded-but-unsubmitted batches; submit multiple consumers; abandon batches; request cancellation during copy; inject stale acknowledgments/helper exit; exercise memory pressure, device loss and shutdown. A compile-fail/API inspection check should prove no safe public path extracts the pooled buffer/bindgroup/command buffer. Native validation must cover several hundred reuse cycles, exact frame identity after random/reverse seeks, P010 stride/padding and CPU-reference color parity. Report allocation/import counts and native allocate/copy/submit timing alongside frame latency. Until this boundary and those checks exist, keep the current one-shot immutable allocation behavior.
