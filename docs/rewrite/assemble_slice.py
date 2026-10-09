#!/usr/bin/env python3
"""Verify and combine actual captures; never synthesize missing acceptance fields.

Usage: assemble_slice.py NATIVE.json TRANSPORT.json EXPIRY.json CLI.json MCP.json OUTPUT.json
CLI/MCP inputs are integrated handoff captures. See acceptance.md for capture commands.
"""
import hashlib
import json
from pathlib import Path
import sys

from verify_slice import require, verify, verify_errors, verify_native


def assemble(paths):
    captures = [json.loads(path.read_text()) for path in paths]
    native, transport, expiry, cli, mcp = captures
    verify_native(native)
    require(cli == mcp, "integrated transport captures diverge")
    require(cli["source_revision"] == expiry["source_revision"], "mixed application builds")
    require(cli["build"] == expiry["build"], "mixed target/profile")
    require(cli["schema"] == transport["cli_schema"] == transport["mcp_schema"],
            "mixed public schemas")
    require(transport["cli_seek_runs"] == transport["mcp_seek_runs"],
            "public seek results diverge")
    for side in ("before", "after"):
        properties = cli["handoff"][side]
        for name in ("ramp", "artist_key", "count_lock", "graph_layout", "asset_overrides"):
            require(properties[name], "empty integrated capture: " + name)
        require(properties["asset_overrides"] == cli[side]["root"]["overrides"],
                "override projection differs from complete construction")
        require(properties["graph_layout"] == cli[side]["presentation"],
                "layout projection differs from complete construction")
    errors = {}
    for name in ("cli_errors", "mcp_errors"):
        captured = dict(transport[name])
        captured["expired_candidate"] = expiry["expired_candidate"]
        verify_errors({"errors": captured})
        errors[name] = captured
    require(errors["cli_errors"] == errors["mcp_errors"], "rejection captures diverge")
    result = dict(native)
    result.update({
        "source_revision": cli["source_revision"], "build": cli["build"],
        "capture_scope": "Native services plus actual CLI/MCP; expiry uses controlled monotonic time. Physical artist acceptance is separate.",
        "handoff": cli["handoff"],
        "retry_undo_groups_added": cli["retry_undo_groups_added"],
        "errors": errors["cli_errors"],
        "cli_schema": transport["cli_schema"], "mcp_schema": transport["mcp_schema"],
        "capture_inputs": [{"name": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()}
                           for path in paths],
    })
    for name, capture in (("cli", cli), ("mcp", mcp)):
        result[name + "_handoff"] = {
            "animated_shape": transport[name + "_handoff"],
            "integrated_before": capture["before"], "integrated_after": capture["after"],
        }
    verify(result)
    return result


if __name__ == "__main__":
    if len(sys.argv) != 7:
        raise SystemExit(__doc__)
    result = assemble([Path(arg) for arg in sys.argv[1:6]])
    with Path(sys.argv[6]).open("x") as destination:
        json.dump(result, destination, indent=2, sort_keys=True)
        destination.write("\n")
    print("Verified combined automated slice; physical native acceptance remains separate.")
