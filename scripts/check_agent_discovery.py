#!/usr/bin/env python3
"""Check MCP discovery with a stripped desktop environment against a live session."""
import argparse
import json
import os
import subprocess

parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("cli")
parser.add_argument("session", help="Known live session from CLI discovery")
args = parser.parse_args()
environment = os.environ.copy()
environment.pop("XDG_RUNTIME_DIR", None)
requests = [
    {"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {
        "protocolVersion": "2025-11-25", "capabilities": {},
        "clientInfo": {"name": "fold-discovery-check", "version": "1"}}},
    {"jsonrpc": "2.0", "method": "notifications/initialized"},
    {"jsonrpc": "2.0", "id": 2, "method": "tools/call", "params": {
        "name": "fold_sessions", "arguments": {}}},
    {"jsonrpc": "2.0", "id": 3, "method": "tools/call", "params": {
        "name": "fold_state", "arguments": {"session": args.session}}},
]
result = subprocess.run([args.cli, "agent", "mcp"], env=environment,
                        input="".join(json.dumps(r) + "\n" for r in requests),
                        capture_output=True, text=True, timeout=45, check=True)
responses = {r["id"]: r for r in map(json.loads, result.stdout.splitlines())}

def outcome(identifier):
    response = responses[identifier]
    assert "error" not in response, response
    content = response["result"]["content"]
    return json.loads(next(c["text"] for c in content if c["type"] == "text"))

sessions = outcome(2)
state = outcome(3)
assert sessions["ok"] and any(s["session"] == args.session for s in sessions["result"]), (
    f"MCP without XDG_RUNTIME_DIR missed live session {args.session}: {sessions}; state: {state}")
assert state["ok"], state
print(json.dumps({"session": args.session, "discovered": True,
                  "revision": state["result"]["revision"], "XDG_RUNTIME_DIR": "unset"}))
