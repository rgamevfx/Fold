# Local protocol transport — M3 in progress

The replacement transport is currently Linux-only and opt-in. Start the desktop
with `fold-desktop --protocol-dir <existing-directory> [--project <file.fold>]`.
The host creates a fresh private child directory and prints its socket path to
stderr. The directory is mode 0700 and the socket is mode 0600. Both client and
server verify the peer's effective user through Linux socket credentials; the
client also checks endpoint ownership, file type and modes. There is no remote
listener. Other platforms remain unqualified rather than using a weaker transport.

`fold-cli protocol <socket>` reads one versioned request from stdin and writes one
response to stdout. For example, the integration test exercises this request:

```json
{"version":2,"command":{"method":"discover"}}
```

Transport framing is a four-byte unsigned big-endian length followed by the exact
JSON bytes. Each connection carries one request and one response. The adapter does
not normalize JSON, generate write identities, or retry requests automatically.
A lost response is an uncertain outcome; retry a write with exactly the same bytes
and request identity. Job admission returns a handle; poll `job_status` separately.
Its optional
`wait_ms` defaults to zero and is capped at 25,000. The endpoint retains valid
waiting reads on the owner, checking results each native tick without blocking
the UI. Completion/cancellation/expiry/epoch errors return immediately; reaching
the deadline returns the current running state without cancelling the job.
Direct in-process dispatch always returns immediate status.

The listener admits at most eight active connections and uses an eight-request
owner queue. Request frames are bounded at 1 MiB and response frames at 16 MiB.
Frame reads/writes have absolute deadlines, including slow partial transfers.
Server I/O stages allow five seconds each and the owner response wait allows 30
seconds, leaving scheduling margin around the 25-second long-poll bound. Overload,
malformed framing and I/O deadlines close the connection; they
do not masquerade as successful protocol outcomes. Transport threads never own
project state. The native session drains up to eight requests per tick through its
existing guarded dispatcher, including active-gesture and epoch checks.

Project replacement retains the endpoint but resets the protocol epoch and jobs.
Session teardown removes the private socket/directory, closes request ownership
and stops listener admission. Detached I/O threads have bounded waits and cannot
publish without the session. Transport access only exposes supported wire commands;
there are no file-open, resource-import, save, export or artist-unlock routes here.
Destination permission work remains required before adding file-writing commands.

Automated evidence covers a real CLI process against a native session, discovery
schema parity, wire-version rejection, exact-byte retry behavior, endpoint modes,
rejection of an insecure socket mode and teardown cleanup. Pure framing tests cover
oversized/empty/truncated input. Tests require Unix-socket permission; the sandbox
rejects socket creation, while the approved local runs pass. Different-user peer
rejection and interactive desktop launch have not been exercised directly. The complete public artist/agent handoff remains pending; MCP qualification is
recorded below.

## MCP stdio adapter

`fold-cli mcp <socket>` now exposes `fold_request` using the shared wire schema.
It implements initialization, initialized notification, ping, tools/list and
tools/call using newline-delimited JSON-RPC; stdout contains only responses.
MCP version 2025-11-25 negotiation is separate from mandatory Fold wire version 2.
The adapter preserves the raw arguments object for Fold retry fingerprints.
Responses include structured content and serialized text; Fold failures set
`isError`. Invalid MCP envelopes/methods use JSON-RPC errors. Stdin EOF shuts down.
Input lines are bounded at 1 MiB plus 64 KiB of envelope allowance; the underlying
Fold payload still has its original 1 MiB cap. Large results are bounded by the
16 MiB Fold response cap, with MCP text/structured duplication above that layer.

Fold jobs remain explicit handles. The adapter advertises no MCP task, sampling,
resource, prompt or HTTP capability. MCP notifications cannot mutate projects or
bypass guarded job cancellation. Use `cancel_job` with its Fold guard to cancel
work. `job_status.wait_ms` uses the same bounded native wait queue through MCP.

Implementation references: [MCP stdio transport](https://modelcontextprotocol.io/specification/2025-11-25/basic/transports),
[MCP lifecycle](https://github.com/modelcontextprotocol/modelcontextprotocol/blob/main/docs/specification/2025-11-25/basic/lifecycle.mdx),
and [tool schemas/results](https://modelcontextprotocol.io/specification/2025-11-25/server/tools).
The actual subprocess test qualifies a proposal → job → inspection → Apply → retry
round trip and schema equality with discovery. The complete phase handoff is open.


## Native handoff and persistence integration

`cargo test -p fold-app --test agent_handoff --locked --offline` runs the same source,
graph and key editing sequence through real CLI and MCP processes, followed by an
agent key-timing patch, explicit rebase over a concurrent artist layout edit,
inspection, observation, Apply/retry, Undo/Redo and native Save/Open. It compares
complete reopened source and schemas between transports. Both paths also reject
locked parent/override edits, same-key races, wrong-generation receipts, cancelled
candidate application and changed-content retry IDs; retry after Undo does not
reapply the edit. See
[handoff scope](editing.md#climcp-native-handoff-and-persistence--m3-in-progress)
for the assertions and remaining acceptance boundaries.
