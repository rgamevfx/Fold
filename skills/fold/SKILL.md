---
name: fold
description: Create and edit Fold motion graphics, compositing networks, reusable groups and timeline sequences through a running Fold session. Inspect viewer output and refine editable workflows. Use for operating Fold; repository implementation work follows the repository's development instructions.
---

Connect through the Fold MCP tools or `fold-cli agent`. Enable Agent → Allow local connections in Fold, or launch with `--agent`. Discover sessions and select the intended project by its path and documents. If several sessions match, ask which one to use.

CLI discovery starts with `fold-cli agent sessions`, then `fold-cli agent call SESSION state '{}'`. Discover an operation's JSON schema with `fold-cli agent call SESSION catalog '{"method":"render"}'`. For long arguments pass `-` in place of JSON and write the JSON to stdin.

Read `state` and the relevant package `catalog` before editing. The catalog supplies current operation examples, typed node sockets and defaults. Use scoped `inspect` requests with JSON pointers and array pagination. Inspection returns a job: poll `jobs` and check the nested outcome before using results.

Submit related edits as one `batch` with the current revision and project_epoch, a meaningful label and a unique request_id. Allocate new identities through `ids` and refer to them using `{"$ref":"name"}`. Keep ordinary artwork strings literal. Reuse a request_id only when retrying the identical request after a lost response. After a conflict, inspect current state and construct a new proposal with a new request_id.

Preserve the user's construction and unknown data. Prefer node/property operations and object merges over replacing whole graphs. Distinguish editing a reusable group definition from changing an instance's inputs. Motion supports editable groups and published interfaces; the current compositor catalog states its own supported operations. Keep times as exact rational seconds and use declared color values.

Acceptance of a job is not completion. Poll its outcome; cancellation cannot undo an already committed edit. Completed jobs have bounded retention, so consume their results promptly. Global undo can include user work: inspect the current state and undo label before requesting it.

Verify artwork with `render` at representative times, including animation boundaries. To see the user's displayed result, list `viewers`, choose a viewer ID and request `capture`. Check displayed/requested identities and stale status. Viewer captures are display-transformed image thumbnails without chrome, gizmos or window zoom. `navigate` explicitly changes only the specified viewer. Use `screenshot` for whole-window/desktop review through the user-controlled Wayland portal. Portal permission or backend failure is not a black frame.

MCP `fold_jobs` returns image content directly. CLI results contain PNG base64; decode it to a file and inspect that image using the harness's image tool. Report visual verification separately from valid graph construction. Save only to the destination requested or agreed with the user; saving writes the named file.

For a complete cross-feature construction example, read or run [scripts/demo.py](scripts/demo.py). It creates new named documents, publishes an editable group interface, animates a title, connects Motion → Composite → Sequence, cuts a clip, and renders a preview. Run it only when those additions match the user's task. Adapt proven constructions into reusable Fold groups or shared commands, and add workflow guidance only after checking rendered results.
