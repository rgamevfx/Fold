# Native edit service — M2/M3 implementation boundary

`fold_motion::editing` prepares a complete native result against an immutable
committed project snapshot. The native desktop panels and application provider now
use this validation gate too. It is not yet a public agent transport. The service
uses the shared project coordinator and imports no application state.

A candidate owns its original document, proposed document and project revision.
These fields are private. Preparation validates scope, structure, protection and
operation/graph semantics; replacement retains project-wrapper metadata. Apply
requires the same project revision and original document, and commits one named
`Edit Motion construction` undo group. Stale apply fails without mutation. Explicit rebasing is now implemented as
described below. Protocol generations and transport idempotency remain M3 work;
the library candidate is not itself a complete protocol/session lifecycle.

Document scope permits a complete document proposal. Definition scope permits only
replacement of that definition under the same UUID. `source::canonical_fragment`
and `source::parse_fragment` project and parse one asset in retained document
context, including external asset references. `prepare_fragment` reconciles a
retained parsed fragment with an explicit `reconcile::IdentityMap`.

Mappings are simultaneous and one-to-one. Sources must belong to the parsed asset;
targets must be records of the same kind in the selected previous asset. Unmapped
records keep their IDs, so ID-free fragments stay fresh unless explicitly mapped.
The resulting asset must retain the selected definition UUID. Known references,
track/key IDs and source spans follow the map; opaque extension data is not rewritten.
A candidate exposes source spans keyed by its reconciled native IDs.

Unrelated assets and surviving presentation metadata remain identical. Removing a
selected-asset record removes only its position, annotation and group membership.
Other layout is retained. Locked-group/member deletion fails the final protection
check. No name-based identity guessing or automatic group deletion occurs.

`SourceBuffer::propose_fragment` accepts mappings for UUIDs in the submitted source.
For an ID-free fragment, callers parse once, inspect that retained fragment's IDs,
and pass it with the explicit map to `prepare_fragment`; they do not reparse and
try to reuse stale generated IDs.

`SourceBuffer` retains the submitted text, parser diagnostics and edit-validation
error separately from the project. Proposing does not mutate the project. Invalid
text, type failures, lock failures and scope failures therefore leave the last
committed construction available to rendering. A returned valid candidate must be
applied explicitly through the coordinator. This service does not move viewer
time or selection. The initial native source editor uses the same validation gate; agent
transport and generation enforcement remain unimplemented.

## Protection policy implemented here

Checks compare the final native records against the base snapshot, independently
of how source or structured edits produced them:

- Protected inputs and parameters retain their entire records and owning context.
  A protected parameter cannot move to another owner/key or acquire a different
  operation key, mode or version under the same node ID.
- Locked groups retain their records and recursively guard member records,
  positions and annotations. These are native-record locks, not a promise that all
  evaluated pixels remain unchanged when independent upstream work changes.
- Instance overrides cannot change protected definition inputs, including inputs
  protected by a locked group or definition. Existing overrides are compared with
  the base rather than rejected merely because they already exist.
- Instances leading to protected definitions retain their IDs, definition targets
  and parent ownership. This follows ancestor asset definitions. Replacing a parent
  route while leaving the original definition disconnected fails protection.
- Adding protection is permitted. Clearing existing protection is rejected by this
  ordinary edit path. The trusted `unlock_for_artist` service below clears only
  selected flags; agent callers do not receive an arbitrary bypass flag.

The implementation compares borrowed native records rather than serializing and
copying complete subgraphs for its protection index. Errors identify the protected
native ID and reason. This does not yet provide all M3 conflict/property diagnostics.

The apply gate currently requires fully resolved semantic qualification. Unresolved
operation records remain losslessly parseable and persistable through native
storage, but proposals still containing them cannot pass this execution-qualified
edit service. No unvalidated fallback is used.

Automated evidence lives in `crates/motion/tests/reference.rs`: invalid buffer
retention, scoped rejection, protected parent/override/context/group cases,
wrapper-preserving source apply, single-group undo/redo and stale apply. Native
artist/agent handoff and the complete slice oracle remain outstanding.


## Explicit rebase — M3 service implementation

`Candidate::rebase(current_snapshot)` compares the retained base, proposed native
construction and current native construction. It returns a **new** candidate and
never mutates or implicitly applies the old one. Apply still requires an unchanged
revision/document. The future session dispatcher must give the new candidate a new
generation and enforce inspection of that exact generation.

Known native collections merge by persistent record ID. Native maps/properties
merge only when their expected base values permit it. Changes to different inputs,
parameters, keys and presentation entries can coexist. A key is an atomic edit
unit; two changes to the same key conflict. Identical independent writes to the
same property also conflict; request-id retry handling will be a separate layer.
Parent removal conflicts with descendant edits. Identity/contract changes conflict
with concurrent semantic edits within their context; a display-name-only change
can coexist with a contract change.

Independent record insertions merge with deterministic UUID tie-breaking. Changes
to the relative order of existing records conflict when both branches changed that
order. Track keys are ordered by their exact rational times and then pass ordinary
track validation, including duplicate-time rejection. Scene-list bindings remain
atomic ordered values. Unknown extension payloads, literals and inline expressions
are atomic; the merger does not guess their internal ownership semantics.

The merged result goes through the ordinary scoped preparation, protection and
semantic validation against the **current** snapshot. A newly locked control still
blocks the proposal. Merged missing dependencies return a rebase conflict rather
than an invalid committed graph. Structured conflict records carry the path, native
identity, expected base value, current value, proposed value and reason.

Original source spans are invalidated after a rebase because the merged document
includes new native edits; source inspection must regenerate them from the new
candidate. This is library/API evidence only. Native GUI and CLI/MCP callers are
not yet connected, and no candidate-generation protocol is advertised as complete.


## Semantic inspection — M3 service implementation

`diff::construction(before, after)` reports deterministic changes using the same
native property shapes as merge. `Candidate::diff()` compares the candidate's
private base and result, including after explicit rebase. The returned changes
carry UUID-based paths, the owning native ID, complete before/after values and a
category: addition, removal, value, connection, protection or order.

Added and removed parents include their complete payload without repeating every
child. Collection order is reported separately as UUID sequences, including when
insertion or removal changes the sequence. Key edits carry the complete key.
Reference changes inside expressions and binding lists are identified as connection
changes. Ordinary labels, literals and layout remain value changes. Unknown
operation parameters and opaque extension payloads are atomic; keys named `locked`
inside vendor JSON are not confused with native protection.

The diff is inspection data, not a patch language or authorization to apply. It does
not yet implement transport inspection receipts or generation enforcement. Tests
verify deterministic identity paths, whole-key changes, connection classification,
opaque payload handling, and exclusion of preserved artist edits after rebase.


## Structured patches — M3 service implementation

`editing::prepare_patches(snapshot, document, scope, patches)` is the structured
entry point into the same candidate validation, rebase and apply services used by
source proposals. Every expected value is compared against the supplied snapshot;
patches do not use the preceding patch's result as their base. An expected-value
failure reports the patch index, native path, expected slot and actual slot.

`patch::Patch` has generated JSON Schema and three required fields: `path`,
`expected`, and `value`. Paths use `merge::Part`: `field` names, `element` UUIDs,
and terminal `order`. Collection indices are not supported. Presence is explicit:
`{"kind":"absent"}` removes a property, while
`{"kind":"present","value":null}` writes JSON null. Unknown Patch/Slot fields
are rejected. For example, the native service representation of a seed edit is:

```json
{
  "path": [{"kind":"field","value":"seed"}],
  "expected": {"kind":"present","value":20261008},
  "value": {"kind":"present","value":42}
}
```

This is a service payload example, not an advertised MCP/CLI endpoint. Document
scope is required for that seed edit. Collection element replacements must carry
the UUID in their path. New records append unless an explicit order is included;
orders run after record mutations and must contain every final UUID exactly once.
Adding a key in the middle therefore requires a final time-ordered key sequence.
The final candidate gate rejects invalid key times and dependencies.

A batch contains 1–128 patches and at most 1 MiB of serialized patch data; paths
have 1–32 parts. Duplicate and ancestor/descendant paths are rejected. Missing
parents are errors. Keys, literals, expressions, binding lists and opaque extension
payloads cannot be addressed below their atomic boundary. Unknown operation
parameters remain atomic. Final structure, selected scope, current protection and
execution semantics still pass the ordinary candidate gate. Definition deletion
patches must include the same deterministic presentation cleanup required by that
scope; they cannot modify unrelated presentation data.

## Explicit local artist unlock

`editing::unlock_for_artist(project, expected_revision, document, targets)` clears
only selected input/parameter protection flags and group locks. It accepts 1–128
unique native IDs, requires the exact current revision, rejects unknown/non-lockable
targets without committing anything, and returns NoChange if all selected flags are
already clear. Other flags and all content remain identical. Clearing one flag does
not clear protection inherited from an unselected locked group.

The action commits one `Unlock Motion controls` undo group. It preserves unresolved
operation payloads because changing lock flags does not change execution semantics.
This is a trusted local artist service, not an agent capability or a bypass option
on proposals. The inspector's protection menu now locks/unlocks selected controls,
node parameters and instance overrides, plus named groups and main-instance
control overrides. Locking uses the ordinary construction transaction. Unlocking
uses `DesktopCommand::UnlockControls` and a separate document-provider method;
neither is registered in the authoring command catalog. The generic host checks
revision, provider ownership and resulting document identity before committing one
`Unlock controls` undo entry. Motion's provider shares the flag-only service above.
The UI verifies the resulting construction before reporting success. Inherited
locks remain until the artist explicitly clears their owning controls/groups.

Automated tests cover desktop routing, selective flags, ordinary-edit rejection,
stale/invalid requests, command-catalog exclusion and undo. The future public
dispatcher still needs an explicit exclusion test. Menu screenshot/artist
interaction verification remains pending.


## Application and initial native panels — M3 in progress

The application now registers `fold.motion.native` and
`fold.motion.construction` schema 1. Project creation constructs an editable
Width input → Rectangle → Placement → Scene using qualified native operations.
No video provider is registered. Old `fold.motion` payloads remain unsupported.

The package contributes `fold.motion.native.edit` to the in-process desktop command
registry. This is **not** a public agent endpoint. Its complete-construction payload
passes `editing::validate_document`, shared with source/structured candidates,
before the feature-neutral host previews or commits a transaction. The host checks
revision and command size. Source/patch authoring and inspection are available
through the generic document-provider service; session epochs and inspected proposal
generations are still needed before exposing the new public protocol.

The native editor provides Graph, Keys and Source tabs. Its inspector
edits scalar, distance, Boolean, vector, integer, text, rational duration, color and rotation
literals through shared property controls.
Port labels come from the operation registry. Non-literal bindings identify driven,
keyframed or unresolved state; asset-reference literals still use Source. Protected records and descendants of locked groups are disabled.
Unresolved operations remain visible and are not presented as editable executable
controls. Rendering is explicitly shown as unavailable.

Numeric gestures preview through the host's transient edit service, commit once on
release, and cancel through Escape. Source drafts are retained per document and are
not overwritten by external revision changes. A stale draft cannot Apply. Rebase validates it against its retained base snapshot,
uses the native candidate merge against the current snapshot, and replaces only
the draft for review. It does not commit or change selection/time. Same-property
conflicts, changed dependencies and current locks retain the original draft and
show the failure. Apply remains a separate revision-checked transaction. Reload
explicitly discards the draft and projects the committed construction. The initial Keys tab now supports scalar/distance tracks as described below.
Lock/unlock actions are available through the inspector protection menu.

Document-provider ownership is explicit and globally unique by type ID; it no
longer assumes the type ID begins with the package ID. Commands and panels still
use their package namespace. Navigation permits a supported non-video document to
open for editing while leaving rendering unavailable. These are generic host
capability rules, with no Motion model imported into shared infrastructure.

Automated tests draw the graph editor and inspector using ImGui at normal and
narrow sizes, and exercise gesture commit/undo/cancel plus source-draft preservation.
These are draw-data and service checks, not screenshot review or artist interaction
acceptance. Full source/control/key visual review, remaining typed keys and the
M3 handoff are outstanding.


## Native scalar key editing — M3 in progress

`keyed::channels` projects direct, resolved scalar and distance node tracks into the
shared curve editor. Track and key IDs remain native UUIDs, times remain exact
rationals, and distance values retain their unit when written back. The projection
is not an alternate evaluator or stored animation model. `keyed::replace` checks
channel identity and metadata, rejects keys moved between track identities,
preserves existing key/track extensions, and validates protection and native
execution semantics. The document provider exposes this through its standard
animation-channel service.

The native Keys tab reuses Fold's shared Track view/Curve Editor, including
selection, key movement, value changes, insert/delete, copy/paste, frame snapping,
retiming, undo and cancel lifecycle. Its playhead is explicitly asset-local to the
selected definition; it does not pretend to drive an unavailable video viewer.
Only Linear/Hold interpolation is offered, and unsupported Auto Key/tangent tools
are hidden. Inserting inside a Hold segment retains Hold in this restricted mode.
Other shared-editor consumers retain their existing defaults.

The animation menu beside a scalar/distance literal parameter can create native
animation. `keyed::animate` creates a Keyframes node and explicit connection,
retaining the original parameter ID and metadata. It does not replace an existing
input/link/expression with a sampled constant. Exposed definition-input defaults
are still literals; this fast path applies to node parameters. Graph/source views
see the same newly authored node. The new key uses the asset-local key playhead,
or zero if no playhead has been set for that definition.

Limits remain explicit: non-scalar typed tracks and tracks nested in lists or
instance overrides are not projected into this initial editor. Source retains
their full data. Shared curve range validation may also reject extreme rational
intervals that cannot be represented by its editing operations. At least one key
must remain in a native track. Bézier, tangent edits, empty tracks and invalid or
locked results are rejected, never silently converted. Visual and artist-input
acceptance of the complete Motion editor remains pending.


## Native graph editing — M3 in progress

The Graph tab projects native controls, operations, asset instances and outputs
through the shared UI SDK graph canvas. Asset selection and instance navigation
retain the same document; selecting an input exposes that control in the inspector.
The creation menu covers qualified CPU operations. Placed Rectangle creates the
required geometry and placement together, retaining both as editable nodes.

Wiring edits use typed native references, never references guessed from vendor
JSON. Reconnection preserves edge IDs and extension data, including references
inside expressions. Scene lists support ordered append/removal without changing
other edge identities. Rewiring a required input is atomic. Invalid types, cycles,
protected changes and deletion that leaves dependencies dangling are rejected by
the same transaction gate as Source. Layout drags preview locally and commit one
undo entry; cancellation restores committed positions. Native presentation data
outside the edited records is retained.

A missing source is reported visibly without supplying an invalid wire to the
canvas. Unresolved operations remain visible; out-of-range display coordinates
produce a Source repair diagnostic. Asset override expression references can be
rewired individually; editing declarations, instance timing, zero-reference override
expressions, and advanced bindings still uses Source. Track inputs use Keys.
Graph context actions and Inspector controls now edit groups and annotations;
the implementation and visual evidence are recorded below.

Graph tests cover reconnection identity/metadata, expression retention, type/cycle
and lock rejection, ordered lists, missing-source display, operation creation and
layout undo/cancel. Source tests cover disjoint rebase, explicit Apply, undo and
conflict retention. Normal/narrow ImGui draw-data checks are automated; screenshot
review and native artist interaction remain unverified.

## Immutable candidate lifecycle — M3 in progress

`candidates::Store` owns a fresh session epoch and at most four live immutable
generations. Inserting or rebasing creates a new UUID generation; a rebase leaves
the old proposal unchanged. Each generation retains its exact base revision.
Inspection returns canonical source, semantic diff and a generation-specific
receipt. Apply requires that receipt and the expected base revision, then uses the
existing candidate transaction gate. A failed/stale apply does not consume the
proposal. Observation can borrow the proposed document without acquiring a receipt
or touching artist selection/time.

Live generations expire after fifteen idle minutes. Successful inspection,
observation access and rebase refresh their access times. Capacity rejects new
admission instead of replacing a live proposal. Cancellation releases live state;
committed generations report AlreadyCommitted, including after artist undo.
Terminal metadata is bounded to 64 entries and ten minutes; forgotten/expired
handles return Expired rather than pretending to cancel or reapply work. These
terminal records do not substitute for request-UUID idempotency records.

This service is not yet exposed as public endpoints. The application session owner
must instantiate a new store on project replacement. Wire-version/envelope checks,
request retries, job scheduling, cancellation tokens, shared CLI/MCP dispatch and
public handoff verification remain required before protocol acceptance.

## Request retry tracking — M3 in progress

The replacement application protocol service `agent::retry::Tracker` reserves a
request UUID before execution. It compares the original request byte length and
SHA-256 fingerprint, including JSON whitespace, and returns Pending for a duplicate
still running or the exact cached response bytes for a completed request. Success
and error response bytes use the same cache. Changed bytes return
IdempotencyConflict. Private completion tickets prevent a different tracker or
second completion from replacing an outcome.

The tracker admits at most 64 pending requests and retains at most 4096 request
identities per session. It caches at most 64 responses / 32 MiB for ten minutes,
with the frozen 1 MiB request and 16 MiB response limits. Eviction releases response
bytes but retains a fingerprint tombstone: identical retries return Expired and
changed retries still conflict. Pending requests do not expire or get evicted.
Pending-capacity rejections retain their fingerprint and rejection; they do not
become executable later. Once the session identity table is full, new admission
stays closed rather than forgetting whether a write ran.

An oversized completion stays reserved until its owner records a bounded error or
explicitly abandons the uncertain result. Abandonment leaves an Expired tombstone;
normal cancellation should record its known response. Response expiration and
candidate expiration remain separate. The job/dispatcher owner must ensure every
reservation is finalized, including interrupted workers, and reject invalid wire
versions/envelopes before admission. This service is not yet a public transport.

Automated checks exercise exact replay, same-length changed payloads, pending
retry, response count/byte/age eviction, identity and active capacity, epoch/ticket
isolation and oversized-result recovery. A combined native test applies a candidate,
undoes it as an artist, and proves that a retry replays the original response
without reapplying the edit, including after the response expires.

## Protocol job ownership — M3 in progress

`agent::jobs::Jobs` retains up to 64 active/completed jobs under a session epoch.
Starting a job returns an unforgeable completion ticket and a cooperative
cancellation token. A cancellation request marks active work without freeing its
slot or pretending it has stopped. Only acknowledged completion changes the status.
A response associated with a committed revision remains AlreadyCommitted when
cancellation races completion; a second completion cannot replace it.

The initial execution deadline is explicit and bounded to fifteen minutes. Host
maintenance requests cancellation at the deadline, including when no client polls;
active records remain owned until workers acknowledge completion. Dropping the
session requests cancellation for every active token. These mechanisms do not
claim that an uncooperative worker has terminated: the runtime owner must supervise
workers, finalize errors/panics, and tick maintenance independently of requests.
Observation supervision is now implemented below; native session ownership and
preparation-worker integration remain pending.

Completed results expire after ten minutes and may be evicted earlier for the
64-job or 32 MiB retained-result budget. Active work is never an eviction candidate.
Each response is limited to 16 MiB; an oversized completion leaves its ticket active
so the owner can publish a bounded error. Forgotten handles return Expired. Job
result storage and request retry storage are separate; transports must report
expired results explicitly even if the original job-admission response is replayed.
The 25-second long-poll bound is defined, but no waiting transport is implemented.

Tests cover cancellation acknowledgment and commit races, active capacity through
deadlines, completed count/byte/age eviction, epoch/ticket isolation, oversized
response recovery and cancellation on session teardown. No worker scheduling,
public endpoint, or CLI/MCP acceptance is claimed by these ownership tests.

## Shared in-process request dispatch — M3 in progress

`agent::dispatch::Dispatcher` accepts strict version-2 JSON requests described by
`agent::wire::schema()`. This is an in-process service, not a shipped socket/CLI/MCP
endpoint. Discovery returns document IDs/support status, the same generated request
schema, operation contracts and admission limits. Frames, asynchronous preparation
and transports remain explicitly unavailable. Worker-backed CPU observations are
described below.

The implemented request commands are `discover`, `inspect_document`,
`inspect_candidate`, `propose`, `rebase`, `apply`, `cancel_candidate` and `observe`. Writes
require epoch, request UUID, base revision and explicit document/asset scope.
Structured patches retain native expected-value checks. Source proposals require
the exact inspected source as their precondition; whole-document source and asset
fragments both use the native source/candidate services, including identity mapping
for fragments. Source diagnostics include byte spans. Proposals enforce the 10,000
operation limit, publish immutable generations and do not apply themselves.

Apply checks candidate scope and the generation's inspection receipt. Rebase
creates a new generation for review. Retired candidates retain scope metadata so
cancellation can validate its target and truthfully report AlreadyCommitted.
Responses carry the session epoch; candidate inspection includes scope, base,
source and semantic diff. Replay is checked before current revision guards, so
artist undo cannot turn a repeated Apply into a new mutation. Recognizable request
identities are checked before full command-schema validation; changed malformed
payloads cannot bypass idempotency conflict detection. Duplicate/unknown fields,
legacy versions and artist-unlock requests are rejected.

Tests exercise the dispatcher from raw bytes through native project transactions:
source/patch proposals, scoped fragments, diagnostics, locked edits, scope/receipt
rejection, disjoint rebase, apply/undo/retry, changed malformed retries and terminal
cancellation. This does not yet qualify the public handoff. The runtime still needs
transport/security integration, fully classified
native validation diagnostics and public-tool examples.

## CPU observations — M3 in progress

The in-process dispatcher now implements `observe` for a committed document at an
exact project revision or an immutable candidate handle. Requests specify a root
asset output UUID and 1–64 rational times. Candidate access does not mint an
inspection receipt, apply an edit, or move artist selection/playback. Each sample
includes scene and root-asset time, typed primitive values or native scene/
placement/geometry measurements, stable element IDs, counts and exact 2D bounds.
Geometry tables deduplicate shared geometry identities within a sample. Geometry
bounds are local; placement bounds include translation. These are CPU reference
measurements, not raster visibility or camera-projected bounds.

Provenance includes epoch/document/revision/candidate/output, seed, CPU quality profile,
source-build/target configuration, and SHA-256 construction/resource/configuration
fingerprints. The qualified CPU subset uses no external font or asset resources;
the response declares empty resource inputs and fingerprints those empty lists.
Camera and raster output size are explicitly null, and frames remain unavailable.

A request shares one million evaluation steps and one million generated elements
equally across its samples. Proposal and observation admission enforce the protocol's
64-level asset nesting bound using a longest-path check without expanding instances;
cycles and missing asset definitions fail explicitly. A separate bounded measurement walk prevents repeated
shared geometry from causing unbounded reporting work. Limits are explicit in the
configuration, failures are reported rather than dropping geometry, and cancellation
is checked during evaluation and measurement. Non-finite transformed bounds fail
before JSON serialization. Observation workers receive the job owner's cancellation token. Host polling
maintains deadlines and adopts completed responses without changing project state.

Automated observation tests verify animated bounds at negative/fractional times,
repeatable identities and fingerprints, revision guards, candidate provenance and
lack of apply authority, unchanged project state, non-finite-bound rejection,
cooperative cancellation and measurement-budget failure. Rendering, native artist
interaction and public transport acceptance remain open.


## Supervised observation workers — M3 in progress

`observe` now returns a job handle and its document scope immediately. Two lazy
CPU workers admit at most eight queued/running tasks, with a two-result channel
providing completion backpressure. Tasks capture immutable document data and have
no project mutation capability. Evaluation, measurement and bounded response
encoding all run on workers. Panics become bounded error responses, and cancelled
queued tasks are acknowledged without invoking their work closure.

`job_status` returns running/cancellation-requested state while work is active,
then returns the retained observation response bytes directly. It does not parse
and reserialize a large completed response on the host thread. `cancel_job` uses
epoch/request/base/scope guards and the shared retry tracker; retries retain their
original cancellation outcome. A request can cancel pending publication of an
observation but cannot relabel a completed project commit. Result expiry remains
explicit. Workers cannot publish into a replacement dispatcher after its result
receiver is gone; session teardown cancels active tokens without blocking on joins.

`Dispatcher::poll(now)` is nonblocking and must be called by the native session
owner independently of client requests. It checks deadlines, adopts completed
results, and releases expired job metadata. Request dispatch also invokes it. The
waiting socket/CLI/MCP transports are still pending; this is not yet native
artist or public-tool acceptance.

Tests use actual worker threads to verify bounded admission, panic supervision,
queued cancellation and completion ownership. Raw-request tests prove snapshot
provenance survives intervening artist edits, old handles fail in another session,
job cancellation enforces scope, and cancellation retries preserve their outcome.


## Supervised proposal preparation — M3 in progress

Source/patch proposals and rebases now share the bounded CPU pool. Each accepted
request returns a job handle, captures the admission revision's immutable snapshot,
and retains its guarded scope. Workers prepare and validate native candidates;
only the dispatcher publishes a successful generation. Publication does not apply
an edit. Artist changes during preparation leave the proposal's original base
intact, requiring explicit rebase and inspection before Apply. Exact request
retries return the original job handle without starting another preparation.

Rebase holds an immutable shared candidate while working against the captured
current snapshot. Original generations are unchanged. Candidate capacity is
checked at publication; a full store reports capacity without evicting existing
generations or changing the project. Cancellation is checked before and after
bounded native preparation and again at publication, so a result completed after
cancellation cannot enter the candidate store. Parsing and native validation do
not yet have cancellation checkpoints inside their individual bounded operations.

Tests cover source/patch/rebase through actual jobs, artist edits before publication,
explicit rebase preserving those edits, stable retry handles, full-store rejection,
and a controlled worker returning a valid candidate after cancellation. Existing
observation and native candidate lifecycle tests still pass. Public CLI/MCP/socket acceptance remains open.


## Supervised inspection — M3 in progress

Document and candidate inspections now return jobs and share the same bounded
worker pool. Document inspection captures the exact admitted revision and scope;
canonical source generation and response encoding run on the worker. An artist
edit while inspection is queued does not change the retained result's source or
revision. Candidate inspection retains an immutable generation, builds canonical
source and semantic diff, and encodes the bounded response before publication.

The candidate service separates inspection preparation from receipt activation.
A private proof is created only after successful projection; the dispatcher checks
job cancellation and candidate liveness before activating the receipt and publishing
the encoded response. Oversized responses return a budget error without activating
a receipt. Concurrent inspections share a generation's stable receipt. Observing
or merely retaining a candidate never grants Apply authority. Late inspection
results for cancelled, expired or committed generations are rejected explicitly.
Cancellation checks surround bounded projection and serialization; these individual
operations do not yet contain finer cancellation checkpoints.

Automated tests cover publication-only authority, concurrent receipt stability,
retired-generation rejection and immutable document inspection across artist edits.
This is in-process worker/service evidence. Socket/CLI/MCP integration and the complete artist/agent handoff still require qualification.


## Native session owner — M3 in progress

The application `Session` now owns the protocol dispatcher and polls it on each
native host tick, independently of client job-status reads. `dispatch_agent` routes
raw versioned requests through that same dispatcher against the committed project.
Successful Apply refreshes desktop dirty/undo/navigation metadata using the existing
session path. Protocol observations and proposals do not change playback or selection.

Creating a new project or successfully opening a project replaces the dispatcher,
changes its epoch, cancels active job tokens and drops old completion receivers.
Saving, artist edits and undo retain the current epoch and retry history. Apply
checks for an active artist gesture inside guarded dispatch and returns a conflict
without clearing its overlay. This result participates in byte-exact retry retention;
a fresh request is required after the gesture ends. Read/proposal workers can still
inspect committed snapshots during a gesture.

Session tests exercise inspected Apply, active-gesture rejection and preservation,
retry stability, undo without repeated application, desktop metadata refresh, and
new/open epoch replacement with old candidate/job rejection. This is automated
native-owner evidence; external transport permissions and interactive artist handoff
are not qualified by these tests.

The [local transport](local-transport.md) now connects Linux socket/CLI requests to this owner. The MCP adapter now uses the same socket and schema; the complete public handoff remains pending.


## Nonblocking job waits — M3 in progress

`job_status` accepts optional `wait_ms` from zero through 25,000, exposed in the
shared schema and discovery limits. Local endpoints retain at most eight waiting
reads and recheck them each host tick. Waiting never holds the project or UI loop.
Finished/cancelled/expired jobs and replacement epochs complete the read immediately;
a wait deadline returns current running/cancellation-requested status without
cancelling or evicting the job. Invalid requests still pass through normal dispatch.
Direct in-process calls return immediate status, while CLI and MCP use the same
endpoint wait queue. The transport's separate owner-response timeout is 30 seconds
so ordinary scheduling overhead does not consume the 25-second job wait budget.


## Group and note editing — M3 in progress

The graph's existing context menu now offers **Group selection**. It groups the
current selection when the context item belongs to it, otherwise just that item.
Membership is limited to graph items in the active asset; IDs and computation stay
unchanged. The Inspector exposes group names, membership checkboxes (including
nested groups), group notes and Ungroup. A selected graph item's Notes field edits
its native annotation. Graph tooltips include direct group names and item notes.
These are presentation groups, not reusable asset definitions or evaluation nodes.

Text controls use the shared edit lifecycle: preview while editing, one commit on
deactivation, Escape to cancel. Membership changes commit as discrete edits.
Ungroup preserves contents, expands membership in parent groups, and removes only
the retired group's presentation entries. Existing structure/protection validation
rejects cycles, missing members and locked edits. Locks disable inspector controls;
explicit unlock remains on the separate artist route. Unknown group metadata is
retained when editing names, notes and membership.

Tests verify group/note commit, undo/redo, cancelled drafts, nested ungrouping,
locked annotation rejection, unchanged computation and normal/narrow draw data.
Two ImGui tests initially collided during parallel context creation; they now share
a test lock, matching the shared UI test pattern. The optional native renderer test
writes a review capture; [normal/narrow Inspector capture](evidence/m3-presentation-inspector.png)
was inspected for labels, spacing and clipping at 400- and 260-pixel panel widths.
This verifies layout, not native mouse/keyboard acceptance or enclosing graph frames.


## Additional typed Inspector controls — M3 in progress

Integer input uses the native signed 64-bit scalar widget, preserving values beyond
floating-point integer precision. Text uses a multiline editor. Duration uses an
exact signed numerator and positive unsigned denominator in seconds; edits rebuild
the normalized rational time without a floating-point conversion. Color exposes
64-bit authored channels with the encoding and alpha mode beside them. RGB remains
unclamped for negative and HDR values; alpha stays in its native zero-to-one range.
Premultiplied RGB is disabled and zeroed at zero alpha, matching the native literal
contract. There is no unqualified display-space color conversion or preview swatch.

These controls use the existing Inspector transaction/gesture path and inherited
locks. An untouched draw preserves exact integers, rational times, text, color
precision and encoding/alpha metadata. [Rendered Inspector capture](evidence/m3-literal-inspector.png)
was reviewed at 400- and 260-pixel widths: full integer values, duration fields,
labels, text and color-channel controls fit without clipping. Additional track types and native input acceptance remain
open; rotation and asset-reference controls are described below. This layout/readback evidence does not qualify a complete color UI or M4.


## Rotation Inspector — M3 in progress

Rotation literals expose an axis and angle in degrees, alongside their declared
Screen, World or Camera-plane space. The editor retains an axis/angle draft so
axis choice at zero angle survives a no-op gesture, and temporarily invalid axes
can be corrected without changing the last valid quaternion. A zero-length axis
shows an inline error and prevents gesture completion; Escape uses normal preview
cancellation. External revisions and cancellation clear drafts. Untouched controls
preserve the exact stored quaternion, including its sign, and never round-trip it
through the display representation.

Edits normalize the axis with scaled arithmetic, then form a unit quaternion.
Finite extreme axis magnitudes remain supported; zero/nonfinite axes and nonfinite
angles are rejected. This edits orientation, not an Euler-order or multi-turn
animation channel. The existing shared gesture/transaction/lock path owns commits.
[Normal/narrow rotation capture](evidence/m3-rotation-inspector.png) was rendered and
reviewed for visible space, angle units, axis labels and clipping. Native interaction
acceptance and broader key editing remain open.


## Asset-reference Inspector — M3 in progress

Asset-reference literals provide a native asset picker using persistent definition
IDs. Choices refresh from the committed construction when its revision changes;
duplicate names receive an ordinal label while retaining distinct IDs. Selection
uses the shared discrete edit response and existing transaction, undo and lock path.
Missing definitions and unavailable external resources remain visible and retain
their exact references until explicitly replaced. External resource import is not
qualified by this picker.

The untouched-control test covers missing definition and external resource
references at normal and narrow widths. The [rendered asset picker](evidence/m3-asset-inspector.png)
was reviewed at 400- and 260-pixel panel widths. This verifies layout and value
preservation; native pointer/keyboard selection acceptance remains open.


## CLI/MCP native handoff and persistence — M3 in progress

`crates/app/tests/agent_handoff.rs` now runs one identical construction through
actual CLI and MCP subprocesses connected to the native Session owner. Source
rename, graph arrangement/annotation, timeline key edits and an input lock use the
same native services/provider as the panels. A whole-key agent patch changes timing;
an artist graph move while the candidate exists requires explicit rebase to a new
immutable generation. Inspection checks the complete expected construction before
apply. Candidate CPU observation leaves artist frame and selection unchanged.

The same transport sequence rejects lock bypass through both a root instance
override and whole-definition replacement, preserving the complete artist state
and revision. Changing a key value while the candidate changes that same key's time
produces a structured conflict identifying the key; it leaves the candidate
available for a subsequent disjoint rebase. A receipt from the original generation
cannot apply the rebased generation, and a cancelled original candidate cannot apply.

An exact Apply retry is identical, reuse of its request ID with a changed handle
fails with `idempotency_conflict`, and one Undo restores precisely the artist state.
Retry after Undo returns the original result without reapplying or clearing Redo;
Redo restores the agent timing edit. Native Save, New and Open commands then exercise
asynchronous project persistence. Public inspection after reopen preserves the full
construction: semantic IDs, tracks, keys, graph positions, annotations, locks, root
input overrides and unknown document/override metadata. Both transports return the
same request schema and complete canonical source after reopening.

This test verifies the native owner/services and external adapters. It does not
simulate physical panel input or replace the frozen animated-shape/ramp/field
handoff, full slice oracle, or later rendering acceptance.


## Native instance override Inspector — M3 in progress

Selected asset instances now expose their inputs in the Inspector; the root
instance is available under the Main instance disclosure on the root asset.
Inherited values display as Asset default and stay disabled until the input's
context menu creates an override. Override for this instance copies the typed
asset default into a new native parameter; Use asset default removes that override.
Existing literal edits retain the override parameter ID, protection and unknown
metadata. Driven/keyframed bindings are identified without flattening them.

The shared literal widgets and edit response feed the existing preview/commit/cancel
transaction path. Definition-input, override-parameter and inherited instance locks
disable both the value and menu. Resetting an override clears its local rotation
draft so an invalid intermediate axis cannot prevent returning to the asset default.
Local instance timing controls are described below.

The optional [normal/narrow renderer capture](evidence/m3-instance-inspector.png)
checks overridden distance, inherited multiline text and locked integer controls at
400- and 260-pixel widths, asserting that drawing changes no construction data.
The native panel draw test also selects the root instance at normal/narrow sizes.
The `instance_menu_input_creates_resets_and_undoes_one_override` test additionally
uses simulated ImGui mouse input to open the actual menu and keyboard navigation to
activate its action at 260-pixel width. It creates one native override, drags its
value through a preview without changing the committed revision, and verifies one
commit on release with the same parameter ID. Reset and successive Undo operations
restore the exact edited override, original override and inherited state. The same
input sequence cannot create an override for a locked definition input. The test
feeds the widget's edit response into the native State preview/commit path.

This is simulated input plus native transaction evidence. Physical desktop input,
Escape during an override drag and resetting an invalid rotation draft remain
unverified; the broader native acceptance gate remains open.


## Exact instance timing Inspector — M3 in progress

The Local timing disclosure now edits each instance's native offset and speed.
Offset is expressed in seconds and speed is a dimensionless multiplier; both expose
signed 64-bit numerators and positive 32-bit denominators without float conversion.
The rational widget is shared with Duration controls. The clock applies
`(parent time - offset) * speed`; the disclosure tooltip explains this order.
Zero speed visibly reports Frozen and negative speed reports Reverse.

These controls share the instance transaction/gesture path and are disabled by an
instance group lock. The focused native State test verifies exact reverse-time
mapping, preview without publication, cancellation, commit, Undo/Redo and rejection
of a timing change after the instance is locked. The renderer capture verifies
untouched fractional offset and reverse rate alongside existing input overrides:
[normal/narrow timing Inspector](evidence/m3-clock-inspector.png). The 400- and
260-pixel layouts were inspected for units, state and clipping. Physical timing
input acceptance remains open; repeat/ping-pong are not part of this native Clock
model and are not advertised by this control.


## Typed key Inspector — M3 in progress

Native node Track bindings now expose per-key Inspector controls: exact rational
time, the matching typed literal editor, Linear/Hold interpolation and removal.
The first key is expanded initially; persistent key IDs own widget state while time
edits reorder the key list. The last key cannot be removed. Duplicate times display
a validation error and prevent gesture completion until corrected or cancelled.
Existing track/key identities, combination, base and unknown metadata remain intact.

Add key at playhead inserts at the current asset-time cursor, with duplicate times
and the 4096-key limit disabled. It samples the authored contribution before applying
an Add/Multiply base, avoiding double application of the base. Existing keys retain
their IDs/metadata; the inserted key receives a new ID and inherits the containing
segment's interpolation. A Hold segment therefore remains held after insertion,
instead of acquiring an unintended ramp. Animate now accepts every
native interpolatable type, including vector, color and rotation, and retains the
target parameter ID and metadata when connecting the new native keyframes node.

The shared scalar/distance curve editor remains available. Other typed node tracks
are selected there or in Graph and edited in the Inspector; the former Source-only
notice now points to that workflow. Instance override Track bindings and editing a
track's combination/base remain outside these controls. This does not introduce
Bezier tangents or unsupported interpolation types.

Tests verify contribution sampling for an additive vector track, exact preservation
of existing keys and duplicate-insertion rejection, plus native animation creation
for vector/color/rotation values with parameter identity and metadata preservation.
The [normal/narrow capture](evidence/m3-typed-key-inspector.png) was reviewed for exact
time fields, vector units, interpolation and add/remove controls; drawing preserves
the complete track. Physical input and typed-key gesture acceptance remain open.


Typed-key follow-up verification compares samples before and after insertion for
both Hold and Linear interpolation under Replace, Add and Multiply, including
samples outside the keyed interval. Existing keys and unknown metadata remain exact.
An ImGui draw test verifies that a duplicate-time draft prevents completion, and
that correcting the duplicate restores completion without mutating the track during
an untouched draw. This checks validation-state handling; physical correction and
Escape interaction remain part of native acceptance.


## Native semantic diagnostics — M3 in progress

Native semantic validation now has a detailed result carrying a category, message
and implicated native IDs. Candidate validation preserves this result through the
editing error and asynchronous worker boundary. The dispatcher maps node/reference
cycles to `cycle`, absent or out-of-scope references to `missing_resource`, registry
resolution failures to `unsupported`, dependency-depth exhaustion to `budget`, and
inference failures to `type`. Responses include `details.native_ids`; a cycle lists
its dependency path. Nested inference retains the originating diagnostic while
unwinding the registry's string-error callback, without classifying message text.

Regression proposals verify cycles, unsupported operations and inference errors
through both Source and structured patches. A missing node reference is tested
through patches because canonical Source requires scope-resolved symbolic
references. Each rejection preserves the complete committed document and revision.
Native UI source rebase errors retain the semantic message. A merged semantic
failure still becomes an explicit rebase conflict.

This is not complete diagnostic qualification. Structural validation and several
binding/expression checks still return generic type errors, parse diagnostics still
use the Source diagnostic path, and execution-time observation classification remains
to be completed. These paths must be audited before the protocol gate closes.


Observation preparation now calls the detailed immutable evaluator constructor.
Proposal and observation consumers share one native-diagnostic-to-protocol mapping;
preparation no longer converts the native category back into a generic type error.
Regression jobs observe structurally persistable constructions with a node cycle,
missing reference, unsupported operation version or inference type mismatch, and
verify both the reported native IDs and unchanged document/revision. The ordinary
string-returning evaluator constructor delegates to this same validation path.
Execution-time budgets/cancellation and arithmetic failures remain a separate audit;
this change qualifies preparation diagnostics only.


## Runtime budget and cancellation causes — M3 in progress

The immutable CPU evaluator now exposes a detailed execution result with Invalid,
Budget and Cancelled causes. The expression budget records whether it stopped on
its step limit or cancellation callback; the evaluator records generated-element
and depth exhaustion at their originating checks. Observe maps those causes directly
to protocol codes. It no longer matches error-message strings or polls cancellation
again to classify an already completed failure. String-returning native callers
use the same execution path and receive its message.

Regression tests distinguish zero-step and zero-element budgets, invalid geometry,
and cancellation messages that deliberately equal former budget-error strings.
They verify that a later cancellation cannot relabel a budget failure and that a
failed request does not poison the prepared snapshot. Remaining diagnostic work
includes structural/binding/expression validation detail and native context for
runtime arithmetic/resource failures; this does not close the full protocol gate.


Runtime node failures now retain the innermost executing operation, its definition,
full native instance path and exact asset-local time. The first failing node records
this context; unwinding Scene/Place consumers cannot overwrite it. Observation error
details expose `native_ids`, `definition`, `instance_path` and `asset_time`. Failures
before any node runs have no fabricated operation context.

The observation regression uses a nested asset with a fractional offset and negative
rate, then verifies the failing rectangle, both instance IDs and exact negative local
time while preserving document/revision. Native budget tests distinguish immediate
pre-node exhaustion (no context) from Scene entry reservation exhaustion and
subsequent shape-geometry exhaustion, reporting the actual operation in each case.
Instance/clock failures before node execution and structural diagnostic detail still
need separate qualification; this does not close M3.

Structural validation now has a detailed native result shared by semantic validation,
candidate validation and atomic patch admission. Asset-dependency and presentation-group
cycles retain their cycle IDs; missing instance definitions retain the instance and
definition IDs. Duplicate identities, input-default type mismatches and object/depth
limits retain explicit categories and implicated IDs. String-returning persistence
callers continue to receive the same messages from this validation path.

Dispatcher regressions verify asset cycles, group cycles and missing definitions through
patch jobs, including unchanged document and revision after rejection. The native edit
regression now requires a typed input mismatch with the input ID instead of a generic
patch error. All 53 native tests and ten dispatcher tests pass. Source parsing still
maps structural failures to source diagnostics; remaining structural checks and binding/
expression failures do not yet all carry native identity context. These are not claimed
as fully qualified diagnostics.

Source parsing now retains the detailed structural cause inside its source diagnostic.
The first implicated identity with a recorded source range supplies the marked span;
the complete native ID list remains on the cause. SourceBuffer preserves that cause
as a validation failure, and proposal errors expose its native category/IDs alongside
the existing flat source diagnostic ranges. Syntax/name-resolution failures retain
their source diagnostics. A recursive presentation-group Source regression verifies
the cycle code, group identity, relevant marked source and unchanged committed state.
All 53 native tests and eleven dispatcher tests pass. This closes structural-cause
loss after parsing, not the remaining binding/expression diagnostic qualification.

Instance override tracks now use the shared typed-key Inspector: exact key times,
typed values, interpolation and removal retain the parameter/track/key identities.
Override reset and inherited input/instance locks use the existing controls and
transaction path. Nested overrides insert keys at the containing asset's cursor;
their keys are explicitly labelled as parent-asset time. Root override keys are
labelled as scene time and omit playhead insertion because this Inspector has no
scene-time cursor. Combination/base editing and creating a track from the override
menu remain unfinished.

The transaction test exposed that native validation/evaluation previously rejected
Track overrides despite their persistent representation. These now validate against
the input type and sample at parent time before applying the child clock, charging
the bounded key work to the evaluation budget. Regression samples cover scene-time
root overrides and a scaled parent with a frozen child. Transaction checks cover
preview/cancel, single commit, Undo/Redo, retained metadata and input-lock rejection.
The normal/narrow renderer capture preserves the complete untouched construction;
physical key gestures and full artist acceptance remain unverified.

The shared typed-key Inspector now exposes Combination and the typed Base for Add
and Multiply tracks. Supported modes follow native type arithmetic; unsupported
choices are disabled. Entering Add/Multiply from Replace creates a neutral base,
preserving sampled values and all key identities. Switching between combined modes
retains the authored base; Replace removes it without rewriting keys. These controls
participate in the existing preview/commit/cancel response path for both node and
instance tracks. Four focused key tests pass, and an additive-vector capture was
reviewed at normal/narrow widths. Physical menu/base editing remains unverified.

Instance input menus now create animation directly from an inherited default or a
literal override. Nested assets start at the parent-asset cursor; the root action is
explicitly named Animate at scene start and starts at zero scene time. Existing
override IDs/metadata survive conversion; unsupported types, driven bindings and
already keyed inputs cannot start another track. A shared validated constant-track
constructor serves node and instance animation creation.

Simulated ImGui mouse/keyboard input creates both inherited and existing-override
tracks and verifies Undo, unchanged definition defaults and retained parameter ID.
The two-item menu required explicit Home navigation in the test; the old single-item
Down/Enter sequence selected the new second item. A separate test preserves metadata
and exact negative fractional parent time. The final UI suite passed 43 unit and
31 integration tests. The optional menu capture opens the popup programmatically;
it qualifies readable contents, not physical popup positioning or artist acceptance.
Root tracks still require an explicit insertion-time UI for adding further keys.

Root tracks now expose Add key after last when no scene-time cursor is available.
It inserts exactly one second after the final key using rational arithmetic; artists
can then edit the new key's exact Time. Overflow disables insertion. The shared
insertion routine retains key identity, samples the authored contribution and
preserves Hold/Linear interpolation. A simulated mouse test at 260 pixels verifies
one completion on release, unchanged existing keys and no change while disabled.
Fractional-time and overflow regressions pass; normal/narrow captures show the
button within the Inspector. Physical artist acceptance remains open.
