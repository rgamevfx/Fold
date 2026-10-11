"""Build an editable image-driven cloner through Fold's public v2 protocol.
Usage: image-sampling.py <fold-cli> <socket> <new.fold> <new-report.json>
Uses a native two-color Motion output as the image. No external media is needed.
"""
import json
from pathlib import Path
import re
import subprocess
import sys
import time
import uuid

CLI, SOCKET, DESTINATION, REPORT = sys.argv[1:]
assert not Path(DESTINATION).exists() and not Path(REPORT).exists()
def request(command):
    deadline = time.monotonic() + 120
    while True:
        result = subprocess.run([CLI, "protocol", SOCKET], input=json.dumps({"version": 2, "command": command}), capture_output=True, text=True, check=True, timeout=30)
        response = json.loads(result.stdout)
        if "error" in response:
            raise RuntimeError(response["error"])
        if response.get("result", {}).get("state") != "running":
            return response
        assert time.monotonic() < deadline, "Job timeout"
        command = {"method": "job_status", "handle": response["result"]["job"], "wait_ms": 1000}
def discover():
    return request({"method": "discover"})
def guard(document):
    found = discover()
    return {"epoch": found["epoch"], "request_id": str(uuid.uuid4()), "base": found["result"]["revision"], "scope": {"document": document, "definition": None}}
inspections = []
def edit(document, change):
    candidate = request({"method": "propose", "guard": guard(document), "edit": change})["result"]["handle"]
    inspected = request({"method": "inspect_candidate", "handle": candidate})["result"]
    assert inspected["scope"]["document"] == document and inspected["changes"]
    inspections.append(inspected)
    request({"method": "apply", "guard": guard(document), "handle": candidate, "receipt": inspected["receipt"]})
def inspect(document):
    found = discover()
    return request({"method": "inspect_document", "epoch": found["epoch"], "revision": found["result"]["revision"], "scope": {"document": document, "definition": None}})["result"]["source"]
def create(name):
    document = str(uuid.uuid4())
    edit(document, {"kind": "project", "edits": [{"op": "create", "document": document, "type_id": "fold.motion.construction", "name": name}]})
    return document
found = discover()
catalog = {(op["key"].split(".")[-1], op["mode"]): op for provider in found["result"]["scoped_proposals"] if provider["type"] == "fold.motion.construction" for op in provider["requests"]["catalog"]["operations"]}
assert ("image_texture", "output") in catalog and ("sample_image", "color") in catalog
compact = lambda value: json.dumps(value, separators=(",", ":"))
def literal(kind, value, **extra):
    return "value(literal(" + compact({"kind": kind, "value": value, **extra}) + "))"
def vector(values, unit="meters", space="world"):
    return literal("vector", values, unit=unit, space=space)
def color(values):
    return literal("color", values, encoding="linear_rec709", alpha="straight")
class Graph:
    def __init__(self, document):
        self.document = document
        self.original = inspect(document)
        self.nodes = []
        self.serial = 0
    def meta(self, key):
        return "@" + compact({"id": str(uuid.uuid5(uuid.UUID(self.document), key))})
    def link(self, name):
        self.serial += 1
        return "link($" + compact(name) + " " + self.meta("edge/" + str(self.serial)) + ")"
    def add(self, name, key, mode, **arguments):
        op = catalog[key, mode]
        parameters = []
        for parameter, spec in op["parameters"].items():
            value = arguments.get(parameter)
            if value is None:
                default = spec["default"]
                assert default, (name, parameter)
                if default["kind"] == "expression":
                    assert default["expression"] == {"kind": "time", "scope": "asset"}
                    value = "expr(time.asset)"
                else:
                    assert default["kind"] == "literal", (name, parameter)
                    value = "value(literal(" + compact(default["value"]) + "))"
            parameters.append("    " + compact(parameter) + " " + self.meta(name + "/" + parameter) + " = " + value + ";")
        self.nodes.append("  let " + compact(name) + " " + self.meta("node/" + name) + " = " + compact(op["key"]) + "." + compact(mode) + " @" + str(op["version"]) + " (\n" + "\n".join(parameters) + "\n  );")
        return name
    def publish(self, name, kind):
        original = self.original
        header = original[:original.index("asset ")]
        header = re.sub(r"canvas .*;", 'canvas '+compact({"width":64,"height":32,"rate":[24,1],"start":{"numerator":0,"denominator":1},"end":{"numerator":5,"denominator":1}})+';', header)
        asset = re.search(r"^asset (.*) \{$", original, re.M)[1]
        output = re.search(r'^  output "main" (@\{[^\n]+?\}) :', original, re.M)[1]
        root = original[original.index("\nroot "):original.index("\npresentation ")]
        source = header + "asset " + asset + " {\n" + "\n".join(self.nodes) + '\n  output "main" ' + output + " : " + compact({"kind": kind}) + " = $" + compact(name) + " " + self.meta("output-edge") + ";\n}\n" + root + '\npresentation {"positions":{},"groups":[],"annotations":{}};\nextensions {};\n'
        edit(self.document, {"kind": "source", "expected": original, "text": source, "mapping": {}})
        return json.loads(output[1:])["id"]

source_document = create("Two-color image")
g = Graph(source_document)
shape = g.add("Rectangle", "shape", "rectangle", width=literal("distance", 32, unit="canvas"), height=literal("distance", 32, unit="canvas"))
placements = []
for index, rgba in enumerate(([1,0,0,1], [0,1,0,1])):
    fill = g.add("Color " + str(index), "fill", "solid", geometry=g.link(shape), color=color(rgba))
    placements.append(g.add("Place " + str(index), "place", "screen", geometry=g.link(fill), position=vector([index*32,0], "canvas", "screen")))
scene = g.add("Image", "scene", "ordered", children="list["+", ".join(g.link(p) for p in placements)+"]")
source_output = g.publish(scene, "scene")
cloner_document = create("Image-driven cloner")
g = Graph(cloner_document)
texture = g.add("Image Texture", "image_texture", "output", document=literal("text", source_document), output=literal("text", source_output))
# Exact index-to-scalar conversion uses the existing rational duration arithmetic.
duration = 'literal('+compact({"kind":"duration","value":{"numerator":1,"denominator":1}})+')'
uv = 'expr(vector("unitless", "world", ((element.index * '+duration+') / '+duration+'), 0.5))'
sampled = g.add("Sample Color", "sample_image", "color", image=g.link(texture), uv=uv)
points = g.add("Points", "distribute", "grid", columns=literal("integer",2), rows=literal("integer",1))
prototype = g.add("Sphere", "mesh", "sphere", radius=literal("distance",0.04,unit="meters"))
copies = g.add("Copies", "cloner", "points", points=g.link(points), prototype=g.link(prototype))
colored = g.add("Image Colors", "animator", "color_replace", geometry=g.link(copies), color=g.link(sampled))
material = g.add("Material", "material", "unlit", geometry=g.link(colored))
placed = g.add("Placement", "place", "world", geometry=g.link(material))
camera = g.add("Camera", "camera", "orthographic", position=vector([0,0,1]), vertical_size=literal("distance",0.18,unit="meters"))
scene = g.add("Scene", "scene", "spatial", camera=g.link(camera), children="list["+g.link(placed)+"]")
output = g.publish(scene, "scene")
found = discover()
observation = request({"method":"observe","target":{"kind":"committed","epoch":found["epoch"],"revision":found["result"]["revision"],"document":cloner_document},"output":output,"times":[{"numerator":n,"denominator":1} for n in [2,0,1]]})["result"]
identities = None
for sample in observation["samples"]:
    collections = [item for item in sample["geometries"].values() if "instances" in item]
    assert len(collections) == 1
    instances = collections[0]["instances"]
    for item, expected in zip(instances, [[1,0,0,1],[0,1,0,1]], strict=True):
        assert all(abs(a-b) < 1e-5 for a,b in zip(item["color"]["value"], expected))
    current = [item["id"] for item in instances]
    if identities is not None:
        assert current == identities
    identities = current
assert observation["provenance"]["resource_inputs"]["assets"]
frame = None
if found["result"]["frame_observation"]["available"]:
    from PIL import Image
    frame = request({"method":"observe_frame","target":{"kind":"committed","epoch":found["epoch"],"revision":found["result"]["revision"],"document":cloner_document},"output":output,"time":{"numerator":1,"denominator":1},"dimensions":[640,320]})["result"]
    pixels = frame.pop("image")
    frame["file"] = REPORT + ".png"
    Image.frombytes("RGBA",tuple(pixels["dimensions"]),bytes.fromhex(pixels["data"])).save(frame["file"])
saved = request({"method":"save_project_copy","guard":{"epoch":found["epoch"],"request_id":str(uuid.uuid4()),"base":found["result"]["revision"]},"path":str(Path(DESTINATION).absolute())})
Path(REPORT).write_text(json.dumps({"source_document":source_document,"cloner_document":cloner_document,"inspections":inspections,"observation":observation,"frame":frame,"saved":saved},indent=2))
print("Saved", DESTINATION)
