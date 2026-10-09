"""Author the requested attribute graph or a live Wrangle example through public tools.
Usage: m5b-background-spheres.py <fold-cli> <socket> <new.fold> <new-report.json> [wrangle]
The running project must contain the user's Background spheres scene object.
"""
import json
import re
import subprocess
import sys
import time
import uuid
from pathlib import Path

CLI, SOCKET, DESTINATION, REPORT = sys.argv[1:5]
WRANGLE = len(sys.argv) == 6 and sys.argv[5] == "wrangle"
assert len(sys.argv) == 5 or WRANGLE
assert not Path(DESTINATION).exists() and not Path(REPORT).exists()

def request(command):
    deadline = time.monotonic() + 180
    while True:
        answer = subprocess.run([CLI, "protocol", SOCKET], input=json.dumps({"version": 2, "command": command}), text=True, capture_output=True, check=True, timeout=35)
        answer = json.loads(answer.stdout)
        if "error" in answer:
            raise RuntimeError(answer["error"])
        if answer.get("result", {}).get("state") != "running":
            return answer
        if time.monotonic() > deadline:
            raise TimeoutError("Public job deadline")
        command = {"method": "job_status", "handle": answer["result"]["job"], "wait_ms": 1000}

def discover():
    return request({"method": "discover"})

def guard(scope=None):
    current = discover()
    result = {"epoch": current["epoch"], "request_id": str(uuid.uuid4()), "base": current["result"]["revision"]}
    if scope is not None:
        result["scope"] = scope
    return result

def inspect(document, object=None):
    current = discover()
    return request({"method": "inspect_document", "epoch": current["epoch"], "revision": current["result"]["revision"], "scope": {"document": document, "object": object, "definition": None}})["result"]["source"]

initial = discover()
catalog = {(op["key"].split(".")[-1], op["mode"]): op for op in initial["result"]["catalog"]["operations"]}
choices = []
for document in initial["result"]["documents"]:
    if document["type"] == "fold.motion.native.scene":
        record = json.loads(inspect(document["id"]))
        for obj in record["native"]["objects"]:
            if obj["name"].lower() == "background spheres":
                choices.append((document["id"], obj["id"]))
assert len(choices) == 1, "Expected one Background spheres object"
document, obj = choices[0]
original = inspect(document, obj)
# Read the public canonical language, retaining existing identities and bindings.
pattern = r'^  let ("(?:\\.|[^"\\])*") (@\{[^\n]*?\}) = ("[^\n]*?")\.("[^\n]*?") @(\d+) \(\n(.*?)^  \);'
old = {}
for match in re.finditer(pattern, original, re.M | re.S):
    name, metadata, key, mode, version, body = match.groups()
    parameters = {}
    for p in re.finditer(r'^    ("[^\n]*?") (@\{[^\n]*?\}) = (.*);$', body, re.M):
        parameters[json.loads(p[1])] = (p[2], p[3])
    old[json.loads(name)] = {"meta": metadata, "key": json.loads(key), "mode": json.loads(mode), "parameters": parameters}
# Superseded operation versions are preserved by public source as unresolved
# records. Re-author only the requested Grid/Copy contracts, keeping their IDs.
for match in re.finditer(r'^  unresolved (.*);$', original, re.M):
    record = json.loads(match[1])
    if (record["operation"], record["mode"]) not in [("fold.motion.distribute", "grid"), ("fold.motion.cloner", "points")]:
        raise RuntimeError("Unexpected unresolved operation in the selected graph")
    parameters = {}
    for name, parameter in record["parameters"].items():
        binding = parameter["value"]
        metadata = {"id": parameter["id"], **parameter.get("extensions", {})}
        if parameter.get("protected"):
            metadata["protected"] = True
        if binding["kind"] == "literal":
            expression = "value(literal(" + json.dumps(binding["value"], separators=(",", ":")) + "))"
        elif record["operation"] == "fold.motion.cloner" and name in ["prototype", "points"]:
            expression = None  # Both connections are explicitly re-authored below.
        else:
            raise RuntimeError("Preserve this nonliteral Grid parameter with a scoped manual source edit: " + name)
        parameters[name] = ("@" + json.dumps(metadata, separators=(",", ":")), expression)
    old[record["name"]] = {"meta": "@" + json.dumps({"id": record["id"], **record.get("extensions", {})}, separators=(",", ":")), "key": record["operation"], "mode": record["mode"], "parameters": parameters}
assert old, "Canonical graph could not be inspected"
old_by_key = {(v["key"].split(".")[-1], v["mode"]): name for name, v in old.items()}
dump = lambda value: json.dumps(value, separators=(",", ":"))
def uid(name):
    return str(uuid.uuid5(uuid.UUID(obj), "m5b/" + name))
def meta(name):
    return "@" + dump({"id": uid(name)})
def val(value):
    return "value(literal(" + dump(value) + "))"
def link(name):
    return "link($" + dump(name) + " " + meta("edge/" + name) + ")"
def rgb(v):
    return {"kind": "vector", "value": v, "unit": "unitless", "space": "world"}
nodes, positions, retained = [], {}, {}
def add(name, key, mode, xy, **overrides):
    contract = catalog[key, mode]
    previous_name = old_by_key.get((key, mode))
    previous = old.get(previous_name, {})
    metadata = previous.get("meta", meta("node/" + name))
    fields = []
    for key, port in contract["parameters"].items():
        prior = previous.get("parameters", {}).get(key)
        binding = overrides.get(key, prior[1] if prior else None)
        if binding is None:
            default = port["default"]
            assert default and default["kind"] == "literal", (name, key)
            binding = val(default["value"])
        fields.append("    " + dump(key) + " " + (prior[0] if prior else meta("parameter/" + name + "/" + key)) + " = " + binding + ";")
    nodes.append("  let " + dump(name) + " " + metadata + " = " + dump(contract["key"]) + "." + dump(mode) + " @" + str(contract["version"]) + " (\n" + "\n".join(fields) + "\n  );")
    positions[json.loads(metadata[1:])["id"]] = xy
    if previous_name:
        retained[previous_name] = name

if WRANGLE:
    program = """// Animated geometry generated entirely inside this Wrangle.
    let g = grid(11, 4, 0.25);
    for i in 0..count(g, "point") {
        let p = point(g, i);
        p.y = p.y + sin(time * params.speed + p.x * 3.0 + p.y * 2.0) * params.amplitude;
        set_point(g, i, p);
        let identity = element_id(g, "point", i);
        set_attribute(g, "point", "Cd", i, vec3(random(identity, 12), random(identity, 24), random(identity, 36)));
        set_attribute(g, "point", "orient", i, orient(vec3(0.0, 1.0, 0.0), time));
    }
    copy_to_points(sphere(0.08), g, "Cd")"""
    parameters = dump({"amplitude": 0.22, "speed": 2.0})
    add("Wrangle", "wrangle", "rhai", [0, 0], inputs="list[]", source=val({"kind": "text", "value": program}), parameters=val({"kind": "text", "value": parameters}), time="expr(time.asset)")
    add("Soft material", "material", "lit", [400, 0], geometry=link("Wrangle"))
else:
    add("Sphere", "mesh", "sphere", [0, 0])
    add("Grid", "distribute", "grid", [0, 260])
    add("Attribute Randomize", "attribute_randomize", "uniform", [320, 260], geometry=link("Grid"), minimum=val(rgb([0.03, 0.03, 0.03])), maximum=val(rgb([1.0, 1.0, 1.0])))
    add("Copy to Points", "cloner", "points", [640, 0], prototype=link("Sphere"), points=link("Attribute Randomize"), attributes=val({"kind": "text", "value": "Cd"}))
    add("Soft material", "material", "lit", [960, 0], geometry=link("Copy to Points"))
assert len(re.findall(r'^asset ', original, re.M)) == 1, "Expected one object asset"
header = original[:original.index("asset ")]
asset = re.search(r'^asset (.*) \{$', original, re.M)[1]
output = re.search(r'^  output ("[^\n]*?") (@\{[^\n]+?\}) :', original, re.M)
root = original[original.index("\nroot "):original.index("\npresentation ")]
presentation = json.loads(re.search(r'^presentation (.*);$', original, re.M)[1])
positions[json.loads(output[2][1:])["id"]] = [1280, 0]
presentation["positions"] = positions
presentation["groups"] = []
presentation["annotations"] = {k: v for k, v in presentation.get("annotations", {}).items() if k in positions}
extensions = re.search(r'^extensions (.*);$', original, re.M)[1]
source = header + "asset " + asset + " {\n" + "\n".join(nodes) + "\n  output " + output[1] + " " + output[2] + ' : {"kind":"geometry"} = $"Soft material" ' + meta("output") + ";\n}\n" + root + "\npresentation " + dump(presentation) + ";\nextensions " + extensions + ";\n"
scope = {"document": document, "object": obj, "definition": None}
def apply_source(expected, edited):
    start = time.monotonic()
    proposal = request({"method": "propose", "guard": guard(scope), "edit": {"kind": "source", "expected": expected, "text": edited, "mapping": {}}})
    handle = proposal["result"]["handle"]
    review = request({"method": "inspect_candidate", "handle": handle})
    request({"method": "apply", "guard": guard(scope), "handle": handle, "receipt": review["result"]["receipt"]})
    return inspect(document, obj), time.monotonic() - start
committed, author_seconds = apply_source(original, source)
def frame(n, d=1):
    current = discover()
    composition = next(x["id"] for x in current["result"]["documents"] if x["type"] == "fold.composition.document")
    return request({"method": "observe_frame", "target": {"kind": "committed", "epoch": current["epoch"], "revision": current["result"]["revision"], "document": composition}, "output": "video", "time": {"numerator": n, "denominator": d}, "dimensions": [960, 540]})
if WRANGLE:
    frames = [frame(n, d) for n, d in [(0, 1), (1, 2), (2, 1), (0, 1)]]
    assert frames[0]["result"]["image"]["data"] == frames[3]["result"]["image"]["data"], "Random seeking changed results"
    assert frames[0]["result"]["image"]["data"] != frames[1]["result"]["image"]["data"], "Animation did not change pixels"
    def replace_parameter(source, name, expression):
        pattern = r'(^    ' + re.escape(dump(name)) + r' @\{[^\n]*?\} = ).*;$'
        result, count = re.subn(pattern, lambda m: m[1] + expression + ";", source, flags=re.M)
        assert count == 1, (name, count)
        return result
    changed_params = val({"kind": "text", "value": dump({"amplitude": 0.5, "speed": 2.0})})
    start = time.monotonic()
    variant, _ = apply_source(committed, replace_parameter(committed, "parameters", changed_params))
    variant_frame = frame(1, 2)
    edit_to_frame_seconds = time.monotonic() - start
    assert variant_frame["result"]["image"]["data"] != frames[1]["result"]["image"]["data"], "Parameter edit did not change pixels"
    restored, _ = apply_source(variant, committed)
    assert frame(1, 2)["result"]["image"]["data"] == frames[1]["result"]["image"]["data"], "Restore changed results"
    # Compile different source live, retain an artist comment, and verify identical output.
    final_source, source_edit_seconds = apply_source(restored, replace_parameter(restored, "source", val({"kind": "text", "value": program + "\n// Live edit: no restart or rebuild."})))
    assert frame(1, 2)["result"]["image"]["data"] == frames[1]["result"]["image"]["data"]
else:
    frames = [frame(n) for n in [0, 2, 3]]
    final_source = committed
    variant_frame = None
    edit_to_frame_seconds = source_edit_seconds = None
saved = request({"method": "save_project_copy", "guard": guard(), "path": str(Path(DESTINATION).absolute())})
Path(REPORT).write_text(json.dumps({"source": final_source, "frames": frames, "variant_frame": variant_frame, "saved": saved, "timings_seconds": {"author": author_seconds, "parameter_edit_to_public_frame": edit_to_frame_seconds, "source_edit": source_edit_seconds}}, indent=2))
print(json.dumps({"saved": saved["result"], "timings_seconds": {"author": author_seconds, "parameter_edit_to_public_frame": edit_to_frame_seconds, "source_edit": source_edit_seconds}}))
