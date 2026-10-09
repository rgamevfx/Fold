"""Author the M5 construction using only public Fold discovery and CLI or MCP requests.

Usage: python3 demos/m5-integrated.py <fold-cli> <socket> <document-uuid> <new-report.json> [cli|mcp]
Requires an untouched native Motion starter. Replaces that starter with the demo.
Writes canonical Source/observations to the report and a public-saved project to <new-report.json>.fold.
"""
import atexit
import json
import math
import re
import subprocess
import sys
import time
import uuid
from pathlib import Path

if len(sys.argv) not in (5, 6):
    raise SystemExit(__doc__)
cli, socket, document, report_path = sys.argv[1:5]
transport = sys.argv[5] if len(sys.argv) == 6 else "cli"
if transport not in ("cli", "mcp"):
    raise SystemExit("Transport must be cli or mcp")
project_path = str(Path(report_path).absolute()) + ".fold"
if Path(report_path).exists() or Path(project_path).exists():
    raise RuntimeError("Report or project destination already exists")

def ensure(condition, message):
    if not condition:
        raise RuntimeError(message)

mcp = None
rpc_id = 0
mcp_schema = None


def close_mcp():
    if mcp is None:
        return
    mcp.stdin.close()
    try:
        mcp.wait(timeout=5)
    except subprocess.TimeoutExpired:
        mcp.kill()
        mcp.wait()


def rpc(method, params):
    global rpc_id
    rpc_id += 1
    mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "id": rpc_id, "method": method, "params": params}) + "\n")
    mcp.stdin.flush()
    line = mcp.stdout.readline()
    ensure(bool(line), "MCP process closed without a response")
    response = json.loads(line)
    ensure(response.get("id") == rpc_id, "MCP response ID mismatch")
    if "error" in response:
        raise RuntimeError(response["error"])
    return response["result"]


if transport == "mcp":
    mcp = subprocess.Popen([cli, "mcp", socket], stdin=subprocess.PIPE,
                           stdout=subprocess.PIPE, text=True)
    atexit.register(close_mcp)
    initialized = rpc("initialize", {"protocolVersion": "2025-11-25", "capabilities": {},
                                     "clientInfo": {"name": "fold-m5-authoring", "version": "1"}})
    ensure(initialized["protocolVersion"] == "2025-11-25", "Unexpected MCP version")
    mcp.stdin.write(json.dumps({"jsonrpc": "2.0", "method": "notifications/initialized"}) + "\n")
    mcp.stdin.flush()
    tools = rpc("tools/list", {})["tools"]
    mcp_schema = next(tool["inputSchema"] for tool in tools if tool["name"] == "fold_request")


def request(command):
    deadline = time.monotonic() + 120
    while True:
        if time.monotonic() >= deadline:
            raise TimeoutError("Public request polling expired; the remote job was not cancelled")
        arguments = {"version": 2, "command": command}
        if transport == "mcp":
            result = rpc("tools/call", {"name": "fold_request", "arguments": arguments})
            value = result["structuredContent"]
            ensure(result.get("isError", False) == ("error" in value), "Inconsistent MCP error status")
        else:
            response = subprocess.run([cli, "protocol", socket], input=json.dumps(arguments),
                                      text=True, capture_output=True, check=True, timeout=30)
            value = json.loads(response.stdout)
        if "error" in value:
            for diagnostic in value["error"].get("details", {}).get("diagnostics", []):
                start = diagnostic.get("start", 0)
                print(globals().get("source", "")[max(0, start-100):start+100], file=sys.stderr)
            raise RuntimeError(value["error"])
        if value.get("result", {}).get("state") != "running":
            return value
        command = {"method": "job_status", "handle": value["result"]["job"], "wait_ms": 1000}

discovery = request({"method": "discover"})
info = discovery["result"]
if mcp_schema is not None:
    ensure(mcp_schema == info["request_schema"], "MCP and discovery schemas disagree")
ensure(any(d["id"] == document and d["type"] == "fold.motion.construction" for d in info["documents"]), "Public construction prerequisite failed")
scope = {"document": document, "definition": None}
inspection = request({"method": "inspect_document", "epoch": discovery["epoch"], "revision": info["revision"], "scope": scope})
original = inspection["result"]["source"]
# Refuse existing artwork or extended starters: this workflow deliberately creates
# a complete demo, not a scoped modification of somebody else's construction.
starter_nodes = re.findall(r'^  let "([^"]+)"', original, re.M)
ensure(starter_nodes == ["Rectangle", "Placement", "Scene", "Rectangle fill"],
       "Demo requires the untouched filled-rectangle starter; discovered nodes: " + repr(starter_nodes))
ensure(re.findall(r'^  input "([^"]+)"', original, re.M) == ["Width"], "Public construction prerequisite failed")
ensure('seed 0;' in original and '"protected":true' not in original, "Public construction prerequisite failed")
ensure('presentation {"positions":{},"groups":[],"annotations":{}};' in original, "Public construction prerequisite failed")
ensure(original.endswith('extensions {};\n'), "Public construction prerequisite failed")
asset_id = re.search(r'asset "Motion" @\{"id":"([^"]+)"\}', original)[1]
root_id = re.search(r'root "Main" @\{"id":"([^"]+)"\}', original)[1]
output_id = re.search(r'output "main" @\{"id":"([^"]+)"\}', original)[1]
catalog = {(op["key"], op["mode"]): op for op in next(p["requests"]["catalog"] for p in info["scoped_proposals"] if p["type"] == "fold.motion.construction")["operations"]}
serial = 0

def uid(name):
    return str(uuid.uuid5(uuid.UUID(document), "m5/" + name))

def dump(value):
    return json.dumps(value, separators=(",", ":"))

def metadata(name):
    return "@" + dump({"id": uid(name)})

def literal(kind, value, **properties):
    return {"kind": kind, "value": value, **properties}

def value(item):
    return "value(literal(" + dump(item) + "))"

def link(name):
    global serial
    serial += 1
    return "link($" + dump(name) + " " + metadata(f"edge/{serial}") + ")"

def default(binding):
    if binding["kind"] == "literal":
        return value(binding["value"])
    if binding["kind"] == "list":
        return "list[" + ",".join(default(v) for v in binding["values"]) + "]"
    raise RuntimeError("Unsupported discovered default " + repr(binding))

def vector(items, unit="meters", space="world"):
    return literal("vector", items, unit=unit, space=space)

def duration(numerator, denominator=1):
    return literal("duration", {"numerator": numerator, "denominator": denominator})

def color(items):
    return literal("color", items, encoding="linear_rec709", alpha="straight")

inputs, nodes, node_names = [], [], []

def control(name, item):
    ty = {k: v for k, v in item.items() if k != "value"}
    if ty["kind"] == "vector":
        ty["dimensions"] = len(item["value"])
    inputs.append("  input " + dump(name) + " " + metadata("input/" + name) + " : " + dump(ty) + " = literal(" + dump(item) + ");")
    return name

def add(name, key, mode, **bindings):
    op = catalog[("fold.motion." + key, mode)]
    ensure(op["cpu_reference"], "Public construction prerequisite failed")
    ensure(set(bindings) <= set(op["parameters"]), "Public construction prerequisite failed")
    params = []
    for key, spec in op["parameters"].items():
        binding = bindings.get(key)
        if binding is None:
            ensure(spec["default"] is not None, "Public construction prerequisite failed")
            binding = default(spec["default"])
        params.append("    " + dump(key) + " " + metadata("parameter/" + name + "/" + key) + " = " + binding + ";")
    node_names.append(name)
    nodes.append("  let " + dump(name) + " " + metadata("node/" + name) + " = " + dump(op["key"]) + "." + dump(mode) + " @" + str(op["version"]) + " (\n" + "\n".join(params) + "\n  );")
    return name

def ramp(name, first, last):
    ty = {k: v for k, v in first.items() if k != "value"}
    return "ramp(" + dump({"id":uid(name), "ty":ty, "stops":[{"id":uid(name+"/0"),"position":0,"value":first},{"id":uid(name+"/1"),"position":1,"value":last}]}) + ")"

def decode(v):
    v /= 255
    return v / 12.92 if v <= .04045 else ((v + .055) / 1.055) ** 2.4

orange = color([1, decode(90), decode(54), 1])
blue = color([.02, .1, 1, 1])
for name, item in [
    ("Wording", literal("text", "MAKE IT MOVE")), ("Entrance duration", duration(3,5)),
    ("Word delay", duration(7,100)), ("Columns", literal("integer",20)), ("Rows",literal("integer",12)),
    ("Spacing",vector([.12,.12])), ("Field start",vector([-1,-.5,0])), ("Field end",vector([1,.5,0])),
    ("Field duration",duration(2)), ("Field radius",literal("distance",1,unit="meters")),
    ("Extrusion depth",literal("distance",.15,unit="meters")), ("Camera position",vector([0,1,5])),
    ("Camera target",vector([0,0,0])), ("Title color",orange),
]:
    control(name,item)
clock = add("Clock", "time", "asset")
background = add("Background", "shape", "rectangle", width=value(literal("distance",1920,unit="canvas")), height=value(literal("distance",1080,unit="canvas")))
background = add("Background paint","fill","solid",geometry=link(background),color=value(color([.015,.02,.035,1])))
background = add("Background screen","place","screen",geometry=link(background))
text = add("Title","text","packaged",text=link("Wording"))
entrance = add("Entrance","tween","cubic_out", **{"from":value(vector([0,80],"canvas","screen")),"to":value(vector([0,0],"canvas","screen")),"duration":link("Entrance duration"),"time":link(clock)})
animated = add("Word motion","text_animator","word_position",text=link(text),position=link(entrance),stagger=link("Word delay"))
paint = add("Title paint","fill","solid",geometry=link(animated),color=link("Title color"))
stroke = add("Title stroke","stroke","solid_centered_butt_miter",geometry=link(paint))
title_screen = add("Title screen","place","screen",geometry=link(stroke),position=value(vector([100,100],"canvas","screen")))
outline = add("Title outlines","outline_text","curves",text=link(animated))
solid = add("Title solid","extrude","straight_text",profile=link(outline),depth=link("Extrusion depth"),canvas_unit=value(literal("distance",.003,unit="meters")))
material = add("Title material","material","lit",geometry=link(solid),color=link("Title color"))
title_world = add("Title world","place","world",geometry=link(material),position=value(vector([-1.5,1.2,0])))
holes = add("Hole fixture","text","packaged",text=value(literal("text","O B 8")))
holes = add("Hole outlines","outline_text","curves",text=link(holes))
holes = add("Hole solid","extrude","straight_text",profile=link(holes),depth=link("Extrusion depth"),canvas_unit=value(literal("distance",.003,unit="meters")))
holes = add("Hole material","material","lit",geometry=link(holes))
holes_world = add("Holes world","place","world",geometry=link(holes),position=value(vector([-1.5,.3,0])))
box = add("Box","mesh","box")
grid = add("Grid","distribute","grid",columns=link("Columns"),rows=link("Rows"),spacing=link("Spacing"))
cloner = add("Cloner","cloner","points",prototype=link(box),points=link(grid))
center = add("Field motion","tween","linear",**{"from":link("Field start"),"to":link("Field end"),"duration":link("Field duration"),"time":link(clock)})
field = add("Field","field","radial",center=link(center),radius=link("Field radius"))
scale = add("Scale ramp","ramp","scalar_linear",position=link(field),stops=ramp("scale/ramp",literal("scalar",.2),literal("scalar",1)))
palette = add("Palette","ramp","color_linear",position=link(field),stops=ramp("palette/ramp",orange,blue))
cloner = add("Scale copies","animator","scale_multiply",geometry=link(cloner),factor=link(scale))
cloner = add("Color copies","animator","color_replace",geometry=link(cloner),color=link(palette))
random = add("Rotation variation","random","uniform",maximum=value(literal("scalar",math.tau)),stream=value(literal("text","rotation")))
cloner = add("Rotate copies","animator","rotation_z_add",geometry=link(cloner),angle=link(random))
cloner = add("Copy material","material","unlit",geometry=link(cloner))
cloner_world = add("Copies world","place","world",geometry=link(cloner),position=value(vector([0,-.8,0])))
camera = add("Camera","camera","perspective",position=link("Camera position"),target=link("Camera target"))
light = add("Light","light","directional")
scene = add("Integrated scene","scene","spatial",camera=link(camera),children="list["+",".join(link(n) for n in [background,title_world,holes_world,cloner_world,light,title_screen])+"]")
# Explicit branch lanes keep content, drivers, responses and placement readable.
# Coordinates are native presentation metadata keyed by stable operation IDs.
lanes = {
    "Clock": (0, 1),
    "Background": (0, 0), "Background paint": (1, 0), "Background screen": (2, 0),
    "Title": (0, 2), "Entrance": (1, 1), "Word motion": (2, 2),
    "Title paint": (3, 2), "Title stroke": (4, 2), "Title screen": (5, 2),
    "Title outlines": (3, 3), "Title solid": (4, 3), "Title material": (5, 3), "Title world": (6, 3),
    "Hole fixture": (0, 4), "Hole outlines": (1, 4), "Hole solid": (2, 4), "Hole material": (3, 4), "Holes world": (4, 4),
    "Box": (0, 7), "Grid": (0, 8), "Cloner": (1, 7),
    "Field motion": (1, 5), "Field": (2, 5), "Scale ramp": (3, 5), "Palette": (3, 6),
    "Scale copies": (4, 7), "Color copies": (5, 7), "Rotation variation": (5, 6),
    "Rotate copies": (6, 7), "Copy material": (7, 7), "Copies world": (8, 7),
    "Camera": (8, 2), "Light": (8, 4), "Integrated scene": (10, 4),
}
ensure(set(lanes) == set(node_names), "Every operation needs an authored layout position")
ensure(len(set(lanes.values())) == len(nodes), "Layout positions overlap")
presentation = {
    "positions": {uid("node/" + name): [column * 420, row * 360] for name, (column, row) in lanes.items()},
    "groups": [],
    "annotations": {uid("node/" + name): note for name, note in {
        "Background": "Screen background",
        "Title": "Shared title: animated Screen text and World extrusion",
        "Hole fixture": "O / B / 8 hole fixture",
        "Field motion": "Driver: moving radial field",
        "Box": "Copies: content, distribution, influence and response",
        "Integrated scene": "Ordered Screen / World / Screen output",
    }.items()},
}
source = 'motion 1;\nseed 20261008;\ncanvas '+dump({"width":1920,"height":1080,"rate":[24,1],"start":{"numerator":0,"denominator":1},"end":{"numerator":4,"denominator":1}})+';\n'
source += 'asset "Motion" @'+dump({"id":asset_id})+' {\n'+"\n".join(inputs+nodes)+'\n'
source += '  output "main" @'+dump({"id":output_id})+' : {"kind":"scene"} = $'+dump(scene)+' '+metadata("output-edge")+';\n}\n'
source += 'root "Main" @'+dump({"id":root_id})+' = "Motion" clock {"offset":{"numerator":0,"denominator":1},"rate":{"numerator":1,"denominator":1}} {};\npresentation '+dump(presentation)+';\nextensions {};\n'
guard = lambda revision: {"epoch":discovery["epoch"],"request_id":str(uuid.uuid4()),"base":revision,"scope":scope}
proposal = request({"method":"propose","guard":guard(info["revision"]),"edit":{"kind":"source","expected":original,"text":source,"mapping":{}}})
handle = proposal["result"]["handle"]
candidate = request({"method":"inspect_candidate","handle":handle})
ensure(candidate["result"]["source"].count('fold.motion.') == len(nodes), "Public construction prerequisite failed")
canonical_presentation = json.loads(re.search(r'^presentation (.*);$', candidate["result"]["source"], re.M)[1])
ensure(canonical_presentation == presentation, "Candidate changed graph presentation metadata")
apply_command = {"method":"apply","guard":guard(info["revision"]),"handle":handle,"receipt":candidate["result"]["receipt"]}
applied = request(apply_command)
ensure(request(apply_command) == applied, "Exact Apply retry changed its outcome")
revision = applied["result"]["revision"]
observed = request({"method":"observe","target":{"kind":"committed","epoch":discovery["epoch"],"revision":revision,"document":document},"output":output_id,"times":[{"numerator":n,"denominator":d} for n,d in [(0,1),(1,4),(1,2),(3,4),(1,1),(2,1),(4,1)]]})
samples = observed["result"]["samples"]
ensure(len(samples) == 7, "Missing observation times")
font = observed["result"]["provenance"]["resource_inputs"]["fonts"]
ensure(len(font) == 1 and font[0]["fingerprint"] == "sha256:b85c38ecea8a7cfb39c24e395a4007474fa5a4fc864f6ee33309eb4948d232d5", "Unexpected font provenance")
retained_rotations = None
for sample in samples:
    seconds = sample["time"]["numerator"] / sample["time"]["denominator"]
    geometries = list(sample["geometries"].values())
    ensure(sample["counts"]["placements"] == 5, "Missing ordered placements")
    title = next(g["text"] for g in geometries if "text" in g)
    solid = next(g for g in geometries if g.get("source_text", {}).get("text") == "MAKE IT MOVE")
    ensure(title["identity"] == solid["source_text"]["identity"], "Text branches reshaped independently")
    ensure(title["words"] == solid["source_text"]["words"], "Text branches disagree on word transforms")
    ensure(len(title["words"]) == 3 and solid["depth_meters"] == .15, "Typography or extrusion mismatch")
    for index, word in enumerate(title["words"]):
        phase = min(1, max(0, (seconds - index*.07)/.6))
        expected = 80 * (1-phase)**3
        ensure(abs(word["offset"][1]-expected) < 1e-10, "Word timing mismatch")
        ensure(word["pivot_kind"] == "advance_center" and
               all(abs(word["pivot"][axis] - word["layout_pivot"][axis] - word["offset"][axis]) < 1e-10 for axis in (0,1)),
               "Word pivot did not follow its group translation")
        members = [glyph for glyph in title["glyphs"] if glyph["id"] in word["glyphs"]]
        ensure(len(members) == len(word["glyphs"]) and all(glyph["offset"] == word["offset"] for glyph in members),
               "Word selection did not transform its complete glyph group")
    copies = next(g["instances"] for g in geometries if "instances" in g)
    ensure(len(copies) == 240, "Cloner count mismatch")
    rotations = {copy["id"]:copy["rotation_z"] for copy in copies}
    if retained_rotations is not None:
        ensure(rotations == retained_rotations, "Time reseeded rotation or instance IDs")
    retained_rotations = rotations
    center = [-1+min(seconds,2), -.5+min(seconds,2)*.5, 0]
    for copy in copies:
        influence = max(0, 1-math.dist(copy["position"], center))
        ensure(abs(copy["scale"]-(.2+.8*influence)) < 1e-12, "Cloner scale mismatch")
        expected = [a+(b-a)*influence for a,b in zip(orange["value"],blue["value"])]
        ensure(max(abs(a-b) for a,b in zip(copy["color"]["value"],expected)) < 1e-12, "Cloner palette mismatch")
# Exercise edits on the same construction, then restore the baseline. Every
# mutation remains a guarded proposal/inspection/Apply through the selected tool.
def frames_at(revision, times):
    captures = []
    for numerator, denominator in times:
        sample_time = {"numerator": numerator, "denominator": denominator}
        frame = request({"method": "observe_frame", "target": {"kind": "committed", "epoch": discovery["epoch"],
                         "revision": revision, "document": document}, "output": output_id,
                         "time": sample_time, "dimensions": [480, 270]})["result"]
        ensure(frame["provenance"]["revision"] == revision and frame["provenance"]["time"] == sample_time,
               "Frame provenance differs from the requested revision/time")
        ensure(frame["image"]["dimensions"] == [480, 270], "Unexpected frame extent")
        pixels = bytes.fromhex(frame["image"]["data"])
        ensure(len(pixels) == 480 * 270 * 4 and all(a == 255 for a in pixels[3::4]), "Invalid opaque display frame")
        captures.append(frame)
    return captures

frame_feedback = {"available": bool(info.get("frame_observation", {}).get("available", False))}
if frame_feedback["available"]:
    # Registered outputs use a half-open interval; preserve the CPU sample at
    # exactly four seconds, and label the final rendered frame at 95/24 explicitly.
    frame_feedback["baseline"] = frames_at(revision, [(0,1),(1,4),(1,2),(3,4),(1,1),(2,1),(95,24)])
    frame_feedback["endpoint_policy"] = "CPU observes 4 seconds; frames end at 95/24 under the half-open output interval"

baseline_source = candidate["result"]["source"]

def apply_source(expected, edited, base):
    proposed = request({"method":"propose", "guard":guard(base),
                        "edit":{"kind":"source", "expected":expected, "text":edited, "mapping":{}}})
    handle = proposed["result"]["handle"]
    reviewed = request({"method":"inspect_candidate", "handle":handle})
    result = request({"method":"apply", "guard":guard(base), "handle":handle,
                      "receipt":reviewed["result"]["receipt"]})
    return result["result"]["revision"], reviewed["result"]["source"]


def replace_control(text, name, item):
    pattern = r'^(  input ' + re.escape(dump(name)) + r' [^\n]+ = ).*;$'
    changed, count = re.subn(pattern, lambda match: match[1] + "literal(" + dump(item) + ");", text, flags=re.M)
    ensure(count == 1, "Expected exactly one named control: " + name)
    return changed


# Exercise the remaining M5 catalog modes through public Source proposals. Each
# trial restores the original construction before the normal handoff variation.
native_modes = []

def trial_source(name, key, mode, target, **bindings):
    count = len(nodes)
    add(name, key, mode, **bindings)
    declaration = nodes.pop()
    node_names.pop()
    ensure(len(nodes) == count, "Catalog trial changed the baseline node list")
    if target == name:
        pattern = r'^  let ' + re.escape(dump(name)) + r' .*?^  \);'
        edited, matched = re.subn(pattern, lambda _: declaration, baseline_source, count=1, flags=re.M | re.S)
        ensure(matched == 1, "Missing trial operation " + name)
    else:
        edited = baseline_source.replace('  output "main"', declaration + '\n  output "main"', 1)
        target_name, old_input = target
        pattern = r'(^  let ' + re.escape(dump(target_name)) + r' .*?)(^  \);)'
        def rewire(match):
            old = '$' + dump(old_input)
            ensure(old in match[1], "Missing trial input")
            return match[1].replace(old, '$' + dump(name), 1) + match[2]
        edited, matched = re.subn(pattern, rewire, edited, count=1, flags=re.M | re.S)
        ensure(matched == 1, "Missing trial consumer")
        current_presentation = json.loads(re.search(r'^presentation (.*);$', edited, re.M)[1])
        current_presentation["positions"][uid("node/" + name)] = [420, 3600]
        edited = re.sub(r'^presentation .*;$', lambda _: 'presentation ' + dump(current_presentation) + ';', edited, flags=re.M)
    return edited

trials = []
for mode in ["solid_centered_butt_round", "solid_centered_round_miter", "solid_centered_round_round"]:
    trials.append((mode, baseline_source.replace('."solid_centered_butt_miter"', '.' + dump(mode))))
trials.append(("transform_screen", trial_source("Screen pivot", "transform", "screen",
    ("Title paint", "Word motion"), geometry=link("Title outlines"),
    pivot=value(vector([400,100],"canvas","screen")), angle=value(literal("scalar",.1)),
    scale=value(vector([1.1,.9],"unitless","screen")))))
trials.append(("transform_mesh", trial_source("Mesh pivot", "transform", "world_mesh",
    ("Cloner", "Box"), geometry=link("Box"), scale=value(vector([1,2,1],"unitless")))))
trials.append(("transform_instances", trial_source("Copy pivot", "transform", "world_instances",
    ("Copy material", "Rotate copies"), geometry=link("Rotate copies"),
    pivot=value(vector([.2,0,0])), angle=value(literal("scalar",.2)))))
for mode, key in [("planar", "tessellate"), ("closed_profile", "extrude")]:
    bindings = {"profile":link("Hole outlines"), "canvas_unit":value(literal("distance",.003,unit="meters")),
                "even_odd":value(literal("bool",True))}
    if key == "extrude":
        bindings["depth"] = link("Extrusion depth")
    trials.append((key, trial_source("Hole solid",key,mode,"Hole solid",**bindings)))
for name, edited in trials:
    trial_revision, canonical = apply_source(baseline_source, edited, revision)
    observation = request({"method":"observe", "target":{"kind":"committed", "epoch":discovery["epoch"],
        "revision":trial_revision, "document":document}, "output":output_id,
        "times":[{"numerator":1,"denominator":1}]})["result"]
    ensure(observation["samples"][0]["counts"]["placements"] == 5, "Catalog trial lost scene placements")
    if name in ("tessellate", "extrude"):
        converted = next(g["mesh"] for g in observation["samples"][0]["geometries"].values()
                         if g.get("mesh", {}).get("source_geometry") is not None)
        ensure(len(converted["source_vertices"]) == len(converted["positions"]) == len(converted["uv"]),
               "Profile attribute correspondence is incomplete")
        ensure("front" in converted["regions"] and (("sides" in converted["regions"]) == (name == "extrude")),
               "Profile regions do not match the operation")
        source_profile = observation["samples"][0]["geometries"][str(converted["source_geometry"])]
        ensure(source_profile["source_text"]["text"] == "O B 8" and bool(source_profile["outlined_glyphs"]),
               "Profile conversion lost its retained curve/text provenance")
    frame = frames_at(trial_revision, [(1,1)])[0] if frame_feedback["available"] else None
    native_modes.append({"mode":name, "source":canonical, "observation":observation, "frame":frame})
    revision, canonical = apply_source(canonical, baseline_source, trial_revision)
    ensure(canonical == baseline_source, "Catalog trial changed baseline Source")

variant = replace_control(baseline_source, "Wording", literal("text", "MAKE IT FLOW"))
variant = replace_control(variant, "Columns", literal("integer", 21))
green = color([.02, 1, .1, 1])
changed_palette = []

def replace_palette(match):
    response = json.loads(match[1])
    if response["id"] == uid("palette/ramp"):
        response["stops"][0]["value"] = green
        changed_palette.append(response["id"])
    return "ramp(" + dump(response) + ")"

variant = re.sub(r'ramp\((\{[^\n]*\})\)', replace_palette, variant)
ensure(changed_palette == [uid("palette/ramp")], "Expected exactly one palette ramp")
variant_revision, variant_source = apply_source(baseline_source, variant, revision)
ensure(set(re.findall(r'"id":"([^"]+)"', variant_source)) == set(re.findall(r'"id":"([^"]+)"', baseline_source)), "Control edit replaced native identities")
variation = request({"method":"observe", "target":{"kind":"committed", "epoch":discovery["epoch"],
                     "revision":variant_revision, "document":document}, "output":output_id,
                     "times":[{"numerator":n, "denominator":1} for n in [0,1,2]]})
for sample in variation["result"]["samples"]:
    geometries = list(sample["geometries"].values())
    title = next(g["text"] for g in geometries if "text" in g)
    solid = next(g for g in geometries if g.get("source_text", {}).get("text") == "MAKE IT FLOW")
    ensure(title["text"] == "MAKE IT FLOW", "Wording control did not reach Screen text")
    ensure(title["identity"] == solid["source_text"]["identity"] and title["words"] == solid["source_text"]["words"], "Wording broke shared text branches")
    copies = next(g["instances"] for g in geometries if "instances" in g)
    ensure(len(copies) == 252, "Count control did not reach Cloner")
    rotations = {copy["id"]:copy["rotation_z"] for copy in copies}
    ensure(all(rotations.get(key) == angle for key,angle in retained_rotations.items()), "Grid growth reseeded surviving instances")
    seconds = sample["time"]["numerator"] / sample["time"]["denominator"]
    center = [-1+min(seconds,2), -.5+min(seconds,2)*.5, 0]
    for copy in copies:
        influence = max(0, 1-math.dist(copy["position"], center))
        ensure(abs(copy["scale"]-(.2+.8*influence)) < 1e-12, "Palette edit changed scale response")
        expected = [a+(b-a)*influence for a,b in zip(green["value"],blue["value"])]
        ensure(max(abs(a-b) for a,b in zip(copy["color"]["value"],expected)) < 1e-12, "Palette stop edit did not reach instance colors")
if frame_feedback["available"]:
    frame_feedback["variation"] = frames_at(variant_revision, [(1,1)])
    ensure(frame_feedback["variation"][0]["image"]["data"] != frame_feedback["baseline"][4]["image"]["data"],
           "Control variation did not change the public rendered frame")
revision, restored_source = apply_source(variant_source, baseline_source, variant_revision)
restored = request({"method":"inspect_document", "epoch":discovery["epoch"], "revision":revision, "scope":scope})
ensure(restored["result"]["source"] == baseline_source == restored_source, "Restoring controls changed unrelated Source")
if frame_feedback["available"]:
    frame_feedback["restored"] = frames_at(revision, [(1,1)])
    ensure(frame_feedback["restored"][0]["image"]["data"] == frame_feedback["baseline"][4]["image"]["data"],
           "Restored construction did not reproduce the public baseline pixels")
saved = request({"method":"save_project_copy", "guard":{"epoch":discovery["epoch"],"request_id":str(uuid.uuid4()),"base":revision}, "path":project_path})["result"]
ensure(saved["state"] == "saved" and saved["revision"] == revision, "Saved project revision differs")
report = {"native_modes":native_modes,"transport":transport,"document":document,"output":output_id,"revision":revision,"source":candidate["result"]["source"],"observations":observed["result"],"nodes":len(nodes),"presentation":canonical_presentation,"frames":frame_feedback,"discovery":discovery,"variation":{"revision":variant_revision,"source":variant_source,"observations":variation["result"]},"restored":True,"saved_project":saved}
with open(report_path, "x") as output:
    json.dump(report,output,indent=2)
print(f"Authored via {transport}: {len(nodes)} native nodes at revision {revision}; baseline, eight catalog trials and control variations complete; saved {project_path}")
