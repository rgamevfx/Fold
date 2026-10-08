# Fold agent interface: research and recommendation

> **RETIRED — 8 October 2026.** Historical design/implementation context only. The [Motion System Rewrite Plan](Motion_System_Rewrite_Plan.md) and its linked proposal supersede this document for motion and agent work. Examples and protocol descriptions below belong to the old system and are not rewrite contracts.

Research checked 2026-10-05. This is a proposal, not an implementation commitment or development-plan change.

## Findings from current primary sources

Effective agent tools expose purposeful operations, relevant context, bounded responses, and actionable errors. Anthropic recommends testing realistic tasks and measuring success, latency, calls, and token use; blindly wrapping every internal function is not its recommendation. For Fold, this supports scoped graph queries and meaningful edits alongside general construction primitives. [Anthropic: Writing effective tools for agents](https://www.anthropic.com/engineering/writing-tools-for-agents)

Programmatic orchestration can reduce context overhead by discovering tools on demand and filtering intermediate results before returning them to the model. This is a useful pattern, not evidence that Fold needs an embedded interpreter or that every harness supports it. Agents with execution tools can orchestrate Fold externally through a CLI/API. [Anthropic: Code execution with MCP](https://www.anthropic.com/engineering/code-execution-with-mcp)

The current MCP transport specification lists stdio and Streamable HTTP. However, transport support alone does not establish protocol-version compatibility: the 2026-07-28 specification describes per-request metadata and message rules that differ from earlier protocol designs. Claude Code explicitly documents local stdio and HTTP configuration, but this does not prove every harness implements the newest protocol revision. Pin and test a supported MCP SDK/protocol version against each target harness; keep that compatibility in adapters. [MCP transports](https://modelcontextprotocol.io/specification/2026-07-28/basic/transports), [Claude Code MCP support](https://code.claude.com/docs/en/mcp)

Agent Skills define progressive loading of metadata, instructions, and supporting resources. This fits a short Fold operational skill with task-specific examples loaded later. Skills explain how to use capabilities; they should not be the only place where executable behavior exists. [Agent Skills specification](https://agentskills.io/specification)

## Recommended architecture

Build one versioned Fold automation contract shared by a JSON CLI and a thin MCP adapter. Keep timeline, compositor, and Motion behavior in their owning packages. Route external edits into the running application's coordinator, transactions, and undo history, and render immutable snapshots using normal evaluation semantics.

For Linux, start with a per-user Unix-domain socket, protected runtime discovery records, and explicit session IDs. An agent harness launches a small stdio MCP bridge, which attaches to that socket; a CLI attaches through the same client library. The bridge does not launch a second editor or rewrite the saved project behind the active editor. Make connection state visible and allow disconnecting clients. Remote HTTP access can come later with its own authentication and deployment design.

Start with these capabilities:

- Discover sessions, installed package capabilities, operation schemas, and supported node/property types.
- Inspect selected documents, graph neighborhoods, published interfaces, and timeline ranges with filtering and pagination.
- Submit typed batches with temporary references to newly created objects, validation, an expected project revision, an undo label, and retry-safe request identity.
- Return concise changes, stable IDs plus readable names, current revision, and useful validation errors.
- Track cancellable jobs and retrieve viewer images or requested renders without blocking the UI.

Separate project edits from transient viewer/selection commands. Revision checks must prevent silently overwriting intervening user edits; retries must not duplicate creations. For broad support, preserve composable operations rather than hard-coding only preselected workflows. Later, promote proven recurring workflows into shared capabilities available to both UI and agents, and document their use in skills and example projects.

The initial milestone should connect an external harness to an already-open, unsaved project, inspect and modify a graph, publish group controls, make a timeline edit, examine the resulting image, and demonstrate undo and conflict handling. Evaluate on actual demo/reference tasks before expanding the tool surface.

## Seeing output on Wayland

Implement two distinct image paths. These are engineering recommendations, not existing functionality claims:

**Fold viewer capture:** obtain the image from Fold's own rendering/presentation pipeline. Read back GPU pixels asynchronously only when requested, with bounded dimensions and pending work. Offer the active viewer's displayed image separately from `render_output`, which evaluates a specific document/time. Capture should state the viewer/document, requested and displayed revision/time/generation, dimensions, display transform/color space, alpha handling, overlays, and whether the displayed result is stale or still pending. A newly requested playhead position must not be mislabeled as the time of a held older frame. A clean render also must not be described as an exact capture of the user's zoomed/overlaid viewer. This path works without desktop screen-capture permission because Fold supplies its own content.

**Desktop/window capture:** when the agent needs application chrome or the physical monitor, use the XDG Screenshot portal for individual images or ScreenCast plus PipeWire for continued capture. ScreenCast advertises available source types and normally presents source selection. Persistent grants are conditional; restore tokens are single-use and may fail when permissions or sources change. Never promise unattended restoration across every desktop. The Screenshot `interactive=false` flag is a customization hint, not a permission bypass. Detect installed portal capabilities rather than assuming the latest documented interface is available. [XDG ScreenCast](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.ScreenCast.html), [XDG Screenshot](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Screenshot.html)

Prioritize viewer capture in the first agent milestone; add portal capture when whole-window visual verification is needed. Neither image transport grants mouse/keyboard control.
