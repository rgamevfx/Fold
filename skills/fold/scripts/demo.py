#!/usr/bin/env python3
"""Build an editable title/group → compositor → timeline through a live Fold session.
Usage: demo.py /path/to/fold-cli SESSION [new-project-copy.fold]
Uses only Python's standard library; never edits project archives directly.
"""
import argparse
import json
import subprocess
import time
import uuid

from pathlib import Path
parser = argparse.ArgumentParser(description=__doc__)
parser.add_argument("cli", help="Path to fold-cli")
parser.add_argument("session", help="Live Fold session UUID")
parser.add_argument("save", nargs="?", help="New project copy path (must not exist)")
args = parser.parse_args()
cli, session = args.cli, args.session
save_path = Path(args.save).resolve() if args.save else None
if save_path is not None and (save_path.exists() or save_path.with_suffix(".png").exists()):
    raise FileExistsError(save_path)

def call(method, params):
    result = subprocess.run([cli, "agent", "call", session, method, "-"],
                            input=json.dumps(params), text=True, capture_output=True)
    if not result.stdout:
        raise RuntimeError(result.stderr)
    reply = json.loads(result.stdout)
    if not reply["ok"]:
        raise RuntimeError(reply["error"])
    return reply["result"]

def wait(job):
    deadline = time.monotonic() + 120
    while time.monotonic() < deadline:
        result = call("jobs", {"job": job})
        if result["status"] == "complete":
            outcome = result["outcome"]
            if not outcome["ok"]:
                raise RuntimeError(outcome["error"])
            return outcome["result"]
        time.sleep(0.05)
    raise TimeoutError(f"Fold job {job} remains pending")

def batch(operations, ids=(), label="Agent edit"):
    state = call("state", {})
    accepted = call("batch", {"request_id":str(uuid.uuid4()), "revision":state["revision"],
                              "project_epoch":state["project_epoch"], "label":label,
                              "ids":list(ids), "operations":operations})
    return wait(accepted["job"])

def ref(name):
    return {"$ref":name}

def seconds(n, d=1):
    return {"numerator":n, "denominator":d}

info = {"width":640,"height":360,"rate":[24,1],"frames":120}
result = batch([
    {"op":"create","id":ref("motion"),"kind":"fold.motion.document","name":"Agent Title"},
    {"op":"edit","document":ref("motion"),"edits":[
        {"op":"info","value":info},
        {"op":"object","id":ref("text"),"kind":"fold.motion.text","name":"Editable Title"},
        {"op":"set","node":ref("text"),"input":"text","value":{"Text":"Made in Fold"}},
        {"op":"add","id":ref("size"),"kind":"fold.motion.math"},
        {"op":"node","node":ref("size"),"value":{"settings":{"operation":"Multiply"}}},
        {"op":"set","node":ref("size"),"input":"a","value":{"Scalar":36}},
        {"op":"set","node":ref("size"),"input":"b","value":{"Scalar":1}},
        {"op":"extract","node":ref("size")},
        {"op":"connect","node":ref("text"),"input":"size","source":{"node":ref("size"),"socket":"value"}},
    ]},
    {"op":"create","id":ref("composite"),"kind":"fold.compositor.composite","name":"Agent Composite"},
    {"op":"edit","document":ref("composite"),"edits":[
        {"op":"info","value":info},
        {"op":"add","id":ref("read"),"parameters":{"Read":{"source":{"Document":{"source":{"document":ref("motion"),"output":"video"},"info":info}},"start":seconds(0),"source_start":seconds(0),"duration":seconds(5)}}},
        {"op":"add","id":ref("output"),"parameters":"Output"},
        {"op":"connect","source":ref("read"),"node":ref("output"),"input":"image"},
        {"op":"output","node":ref("output")},{"op":"layout"}
    ]},
    {"op":"create","id":ref("sequence"),"kind":"fold.timeline.sequence","name":"Agent Sequence"},
    {"op":"place","source":ref("composite"),"target":ref("sequence"),"at":seconds(0)},
], ["motion","text","size","composite","read","output","sequence"], "Build editable title workflow")
ids = result["ids"]
motion = wait(call("inspect", {"document":ids["motion"]})["job"])["data"]
group_id = next(n for n in motion["graph"]["nodes"] if n["id"] == ids["size"])["settings"]["group"]
group = motion["groups"][group_id]
for port in group["inputs"]:
    port["name"] = {"a":"Base size", "b":"Size multiplier"}.get(port["id"],port["name"])
    port["section"] = "Title"
# A keyframe driver is a normal editable graph node. Exact-time keys live in its settings.
batch([{"op":"edit","document":ids["motion"],"edits":[
    {"op":"group","id":group_id,"value":{"name":"Title Size","inputs":group["inputs"]}},
    {"op":"add","id":ref("keys"),"kind":"fold.motion.keyframes"},
    {"op":"node","node":ref("keys"),"value":{"settings":{"channels":[],"keys":[
        {"time":seconds(0),"value":{"Scalar":0.5},"interpolation":"Linear"},
        {"time":seconds(1),"value":{"Scalar":1},"interpolation":"Ease"}
    ]}}},
    {"op":"connect","node":ids["size"],"input":"b","source":{"node":ref("keys"),"socket":"value"}}
]}], ["keys"], "Publish and animate title controls")
sequence = wait(call("inspect", {"document":ids["sequence"]})["job"])["data"]
clip = sequence["clips"][0]["id"]
batch([{"op":"edit","document":ids["sequence"],"edits":[
    {"Clip":{"id":clip,"linked":True,"edit":{"Split":{"at":seconds(2),"right_id":ref("right")}}}}
]}], ["right"], "Cut title at two seconds")
image = wait(call("render", {"source":{"document":ids["sequence"],"output":"video"},
                             "time":seconds(1),"dimensions":[640,360]})["job"])
import base64
import tempfile
from pathlib import Path
preview = Path(tempfile.mkdtemp(prefix="fold-agent-preview-")) / "frame.png"
preview.write_bytes(base64.b64decode(image["image"]["data"]))
for name, at in [("start", seconds(0)), ("after-cut", seconds(5, 2))]:
    frame = wait(call("render", {"source":{"document":ids["sequence"],"output":"video"},
                               "time":at,"dimensions":[640,360]})["job"])
    preview.with_name(name + ".png").write_bytes(base64.b64decode(frame["image"]["data"]))
edited_sequence = wait(call("inspect", {"document":ids["sequence"],"pointer":"/clips"})["job"])["data"]
if save_path is not None:
    path = save_path
    state = call("state", {})
    wait(call("save", {"request_id":str(uuid.uuid4()),"revision":state["revision"],
                       "project_epoch":state["project_epoch"],"path":str(path)})["job"])
    path.with_suffix(".png").write_bytes(base64.b64decode(image["image"]["data"]))
print(json.dumps({"documents":ids,"preview_file":str(preview),"preview":image["metadata"],"clips":edited_sequence,"state":call("state",{})}, indent=2))
