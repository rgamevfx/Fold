# Live SDK tools

Use for building or revising artist-editable badges, copies, drivers, or a live candidate preview. Fetch `catalog sdk=true` (SDK version 2) for declarations. Public operations also have generated schemas in the package catalog. No repository or Rust access is required.

1. Read `state`; reuse the intended session and inspect artist selection. `inspect` with `query:{mode:"construction",node:ID}` returns a Motion object's children, input bindings, drivers and references. Discover detailed signatures only where needed.
2. Begin a candidate at the observed revision/epoch. Submit JavaScript through `author`, including a stable `namespace`, candidate, generation, epoch, fresh request ID and label. Poll the job and retain its new generation.
3. Call `candidate` with `action:"preview"`, candidate, generation, epoch, fresh request ID, `source:{document:ID,output:"video"}` and exact `time`. Fold opens **Agent preview** with independent frame scrubbing, Original comparison when that document exists live, Apply and Discard. Subsequent author edits update that preview to the latest successful generation. Normal viewer playback/selection stay unchanged. `observe` remains the exact-generation multi-frame evidence tool.
4. Apply in the UI or commit through MCP when authorized. One application creates one undo entry. A changed live revision blocks application; inspect and rebuild against the artist's edits. Save the committed revision and refresh state.

## Construction example

This is an `author.source` body, not a standalone script. Use namespace `badge-demo` consistently:

```js
const d=plan.document('motion','fold.motion.document','Badge study');
d.initialize(d=>d.info({width:960,height:540,frames:240,rate:[24,1]}));
const badge=d.badge('badge',{text:'Hi Fold',position:[715,270]});
const copies=d.duplicate('copies',badge,{count:52,spacing:[-9,0],stagger:0.024});
d.oscillator('bounce',copies,'offset_y',{minimum:-60,maximum:60,duration:1.25});
d.colorRamp('palette',copies,'color',[[0,[0.38,0.025,0.12,1]],[1,[0.1,0.12,0.85,1]]]);
```

Use the returned `handles.motion` as the preview document. Badge exposes `.text` and `.background` node handles. Duplicator, Oscillator and Color Ramp are the editor's normal editable groups. Copy Progress is created independently for the duplicator ramp. `style`, `group`, `parent` (preserving placement at the supplied time), and `reference` reuse normal scene authoring. `plan.composite` and `plan.sequence` connect document outputs. The Duplicator references source content in local space: move its transform to reposition visible copies; moving the hidden source does not relocate them. `Document.edit` retains arbitrary graph/interface operations discovered through catalog.

## Creation and updates

Constructors (`document`, `object`, `text`, `node`, `badge`, `duplicate`, drivers) initialize new identities only. Constructor arguments are defaults, not subsequent patches. `.initialize(fn)` runs only for a new object. Explicit `.set`, `.settings`, `.role`, `.properties`, `.ramp`, `.connect` and `.reference` are edits every time they are called. Keep those setters out of repeatable initialization except inside `.initialize`.

Handles are scoped by namespace and document key and saved with the project, independent of display names. `author` reloads them automatically; results return all `handles` and typed `bindings`. Client-side SDK users pass those bindings back to FoldPlan. `state.authoring_namespaces` discovers existing namespaces. Conflicting declarations, duplicate namespace keys and deleted targets are errors. A renamed object retains its handle; a different namespace creates independent work. Deletion/identity repair is never automatic. To intentionally release a deleted target, use candidate edit with the public transaction operation `{op:"forget",namespace:"badge-demo",keys:["motion/spare"]}` before authoring again. This only releases those bindings; it does not delete artwork. Release every owned key when a whole document was intentionally removed, and inspect surviving artwork before recreating anything.

For “make motion gentler,” retain the namespace and modify only the existing driver:

```js
const d=plan.document('motion');
const copies=d.duplicate('copies',d.badge('badge'));
d.oscillator('bounce',copies,'offset_y').set('minimum',-20).set('maximum',20);
```

This preserves artist text, layout and palette edits. To change text deliberately, use `d.badge('badge').text.set('text','New title')`. New driver creation refuses an already-driven input; inspect/edit that driver or explicitly disconnect it first. Ramp stop changes edit its shared definition, so all instances of that definition change.

Verify representative frames, count/source propagation, artist-edit preservation and undo. A valid script is not evidence of readable artwork. Preview capture reports candidate identity and whether it shows the original; a stale capture is not current evidence.

For a full-canvas background, explicitly set the rectangle object’s transform to `[0,0]`; scene objects otherwise receive the shelf’s insertion placement. Authored colors use the project’s color policy. Legacy and managed-color projects may display identical numeric colors differently; MCP currently lacks a project color-policy edit. Compare references under the same policy.
