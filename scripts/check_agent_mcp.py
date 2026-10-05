#!/usr/bin/env python3
"""Exercise real MCP stdio discovery and capture against an explicit live session."""
import argparse
import base64
import json
import subprocess
import time
from pathlib import Path

p = argparse.ArgumentParser()
p.add_argument("cli")
p.add_argument("session")
p.add_argument("output")
a = p.parse_args()
process = subprocess.Popen([a.cli,"agent","mcp"],stdin=subprocess.PIPE,stdout=subprocess.PIPE,text=True)
serial = 0

def request(method, params):
    global serial
    serial += 1
    process.stdin.write(json.dumps({"jsonrpc":"2.0","id":serial,"method":method,"params":params})+"\n")
    process.stdin.flush()
    response = json.loads(process.stdout.readline())
    assert response["id"] == serial and "error" not in response, response
    return response["result"]

def tool(name, args):
    result = request("tools/call", {"name":"fold_"+name,"arguments":{"session":a.session,**args}})
    assert not result.get("isError"), result
    reply = json.loads(next(c["text"] for c in result["content"] if c["type"]=="text"))
    assert reply["ok"], reply
    return reply["result"], result["content"]

try:
    init = request("initialize", {"protocolVersion":"2025-11-25","capabilities":{},"clientInfo":{"name":"fold-integration-check","version":"1"}})
    assert init["protocolVersion"] == "2025-11-25"
    process.stdin.write(json.dumps({"jsonrpc":"2.0","method":"notifications/initialized"})+"\n")
    process.stdin.flush()
    tools = request("tools/list", {})["tools"]
    assert any(t["name"] == "fold_capture" for t in tools)
    state,_ = tool("state", {})
    schema,_ = tool("catalog", {"type":"fold.motion.document","query":"math"})
    assert schema["catalog"]["nodes"]
    views,_ = tool("viewers", {})
    viewer = next((v for v in views if v["active"]), views[0])
    accepted,_ = tool("capture", {"viewer":viewer["viewer"]})
    deadline = time.monotonic()+30
    while time.monotonic()<deadline:
        job,content = tool("jobs", {"job":accepted["job"]})
        if job["status"] == "complete":
            assert job["outcome"]["ok"], job
            image = next(c for c in content if c["type"]=="image")
            Path(a.output).write_bytes(base64.b64decode(image["data"]))
            print(json.dumps({"protocol":init["protocolVersion"],"tool_count":len(tools),"revision":state["revision"],"capture":job["outcome"]["result"]["metadata"],"image":a.output},indent=2))
            break
        time.sleep(.05)
    else:
        raise TimeoutError("Capture did not finish")
finally:
    process.stdin.close()
    process.wait(timeout=5)
