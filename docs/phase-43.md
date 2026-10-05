# Phase 43 — live agent access

Fold exposes one versioned local automation service, with CLI and MCP stdio adapters. Package providers own creative authoring; project and transport infrastructure do not interpret Motion, compositor or timeline models. See [research](agent-interface-research.md) for the protocol decisions and sources.

## Connect

Build `cargo build -p fold-app --features desktop --bin fold-desktop --bin fold-cli`. Launch Fold normally and select **Agent → Allow local connections**, or launch `fold-desktop --agent --project /absolute/path/project.fold`. Access is disabled by default. Disabling access closes the session and requests cancellation of its pending work.

Configure a local stdio MCP server in the chosen harness:

```json
{
  "mcpServers": {
    "fold": {
      "command": "/absolute/path/to/fold-cli",
      "args": ["agent", "mcp"]
    }
  }
}
```

For Codex, register the server with `codex mcp add fold --env XDG_RUNTIME_DIR=/run/user/1000 -- /absolute/path/to/fold-cli agent mcp`, replacing `/run/user/1000` with the app's actual runtime directory. Restart the MCP connection after changing its configuration; an already-running bridge retains its startup environment.

The containing configuration format depends on the harness. The command and arguments above are the executable interface; no configuration is installed automatically. The harness must run as the same Linux user with access to Fold's runtime directory. Discover with `fold_sessions`, then pass the selected session UUID to tools. There is no remote HTTP endpoint or embedded model dependency.

CLI equivalents:

```sh
fold-cli agent sessions
fold-cli agent call SESSION state '{}'
fold-cli agent call SESSION catalog '{"type":"fold.motion.document"}'
fold-cli agent call SESSION catalog '{"method":"batch"}'
fold-cli agent call SESSION batch - < batch.json
```

Use the type identifiers returned by discovery. `fold-cli agent serve [project.fold]` provides the same service without a native window. Rendering requires the normal Fold color/media runtime; the validation fixture used `FOLD_COLOR_ROOT` pointing to the installed OCIO package.

The portable [Fold skill](../skills/fold/SKILL.md) can be installed in a harness's skill directory. Its [executable example](../skills/fold/scripts/demo.py) uses only Python's standard library and the CLI. It constructs new editable documents; consult `--help` before running it against an existing project.

## Contract and behavior

The API uses version 1 newline-delimited JSON over a same-user Unix socket. Requests contain `version`, `method`, and `params`; responses contain `ok` and `result` or `error`. Runtime directories are owner-only and sockets mode 0600. The location is `$XDG_RUNTIME_DIR/fold-agents` when explicitly configured; otherwise it uses `/run/user/<uid>/fold-agents` if that login runtime exists, then `/tmp/fold-runtime-<uid>/fold-agents` on headless hosts. Ownership and permission checks apply to every choice. A custom app runtime directory must also be passed explicitly to the MCP bridge. This is local application access, not a sandbox for hostile programs running as the same OS user.

Discovery exposes method schemas, package operation examples, node sockets/defaults and registered commands. Methods cover state, catalog, scoped inspection, batches, undo/redo, jobs/cancellation, render, viewers/capture, navigation, save and portal screenshot. MCP additionally discovers sessions. The adapter implements explicit legacy MCP initialization for 2024-11-05 through 2025-11-25, tools/list and tools/call; it does not claim the newer draft lifecycle, subscriptions or remote authentication support.

Motion supports node and scene-object construction, wiring, parameters, animation, extraction into reusable groups, group definitions and published controls. Compositor supports its existing operators, connections, parameters, animation and output selection. Timeline operations use existing typed sequence edits, including splits, trim, move, lift, links and track changes. Project-level batch operations create, rename, remove and place documents, import media and place assets. Compositor does not currently have a reusable group document model; reusable custom interfaces are Motion's existing group facility.

A batch stages work against an immutable snapshot, validates through package providers and publishes one named undo transaction. Invalid or stale work does not partially modify the live project. Both project revision and project epoch are checked, protecting against project replacement. A transient user edit or active file operation prevents publication. Exact retries reuse the original response; changed payloads require new request IDs. Unknown extension data survives package edits.

Jobs move inspection, staging, rendering, PNG encoding, saving and capture readback off the UI thread. Acceptance is not completion: poll the returned job and inspect its nested result. Saving pins a snapshot and uses existing project serialization. Cancellation is cooperative; it cannot reverse committed edits or a file write that already completed. Undo/redo operate on the shared project history, including user actions.

Limits include 1 MiB requests, 16 MiB replies, 16 client slots, bounded ingress, four active jobs and 16 retained jobs. Batches allow 128 operations and 512 aliases. State lists use pages of at most 200 documents/assets. Scoped inspection limits output to 256 KiB and array pages to 200 entries. Retry history is capped at 512 requests/8 MiB; disable and re-enable access when it fills. Consume job results promptly; retention is bounded.

## Seeing Fold on Wayland

`viewers` reports active/focused viewer identity, displayed and requested content, exact frame identity and stale status. `capture` reads the retained GPU presentation image on a worker, supporting both RGBA8 and BC7. It returns a bounded PNG thumbnail with the displayed content identity and display/view policy. No screenshot portal is needed to see Fold's own rendered output.

A viewer capture represents the held image or crop, without application chrome, gizmos or the window's zoom/pan framing. The observed project revision is metadata about the current project, not a claim that a stale held image was rendered from that revision. `render` separately evaluates a specified committed output at exact rational time, without changing the viewer; `navigate` explicitly seeks/retargets the chosen viewer.

`screenshot` uses the interactive XDG Screenshot portal for desktop/window review. Permission denial, missing backend, decode failure, cancellation and timeout produce errors. Capture is a still image, not a continuous PipeWire ScreenCast feed. The native portal permission flow remains unverified on this machine; direct native viewer capture was verified. Remote/headless clients cannot assume they can interact with a desktop portal.

## Validation evidence

- Four focused agent integration tests passed: atomic batches, extension preservation, aliases/literal text, named undo/redo, invalid/cancelled staging, stale revisions, project replacement, socket lifecycle, replay conflicts, framing bounds and MCP initialization/tool errors.
- Existing project lifecycle tests passed outside the sandbox. Sandbox runtime-directory restrictions and worker timing required that rerun.
- Native GPU capture test passed for exact RGBA pixels, compressed presentation decoding, cancellation and budget accounting. A high-frequency BC7 fixture initially failed the existing compression quality gate; the compressed capture branch uses a compressible fixture to test readback independently.
- Native Agent menu render checks passed and were visually inspected at normal and narrow widths.
- Package boundary check, skill validation and final diff whitespace check passed.
- Final CLI/desktop builds passed. The four focused integration tests passed again after the final API edits. The focused held-versus-requested viewer metadata test passed. Final application Clippy completed with no warnings in the added automation modules; existing dependency/package warnings remain.
- The external CLI example created an animated Motion group with published controls, a compositor and a split timeline, rendered three times, and saved `/tmp/fold-agent-demo.fold`. The native desktop reopened that project successfully. Rendered frames were visually inspected.
- A separate external MCP subprocess initialized, discovered 15 tools, located the native viewer and received PNG image content. `/tmp/fold-agent-viewer.png` was visually inspected.
- A separate live CLI batch edited the open title, sought the viewer, captured the changed image, and undid the transaction. Revision advanced on edit and undo, and dirty state returned to false. `/tmp/fold-agent-live-edit.png` was visually inspected.

Desktop compilation passed. Workspace Clippy completed; new warnings were corrected, with existing warnings retained. The workspace test run passed the agent suite and earlier suites, then stopped at `media_sequence::input_output_transfer_roundtrips_every_rgb8_value`: the assertion that a mismatched graph width must fail did not hold. The color roundtrip passed, and this change does not modify that renderer path. Later workspace suites were not reached. Temporary images and project files are local validation artifacts, not committed example assets. The portable generator reproduces the example.

## Discovery correction after Codex demo setup

The connected Codex MCP process filtered out `XDG_RUNTIME_DIR`, while Fold inherited `/run/user/1000`. The original bridge searched the separate `/tmp/fold-runtime-1000` fallback, returning an empty session list and ENOENT for the known session. Process inspection confirmed the same UID, executable and visible socket, ruling out sandbox isolation. Earlier standalone MCP verification inherited the shell environment and did not cover this case.

Discovery now resolves the standard login runtime when the environment variable is absent, while preserving explicit overrides and private-directory checks. Socket errors include the attempted path. The local Codex configuration also sets `XDG_RUNTIME_DIR` explicitly. The regression command `python3 scripts/check_agent_discovery.py target/debug/fold-cli SESSION` launches a real MCP process with that variable removed and asserts both discovery and direct state access. It reproduced both original failures before the fix.

Validation: the regression failed with the original empty list and ENOENT, then passed against the same live empty session after rebuilding, with `XDG_RUNTIME_DIR` still removed. Both CLI and desktop builds passed, all four focused `agent_access` tests passed, and the diff whitespace check passed. Already-connected MCP processes still need a restart to load the new binary/configuration. No live project edits were made.
