#!/usr/bin/env python3
"""Reproduce the C0 deletion-reference inventory without changing project files.

Scans tracked repository text and decodes tracked .fold archives read-only.
Matches are review obligations, not proof that an entire matching file is dead.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parents[1]
GROUPS = {
    "two_layer": {
        "phase": "C1", "owner": "Timeline and app",
        "replacement": "Sequence/Composition; preserve old provider payloads opaquely",
        "pattern": r"VideoLayers|VIDEO_LAYERS|LegacyLayers|foreground_opacity|--layers|DesktopCommand::(?:Import|Opacity)\b|\b(?:Import\(Vec<PathBuf>\)|Opacity\(f32\))|(?:workflow|media_workflow)::(?:import|active)\(",
        "whole_files": ["crates/timeline/src/video.rs"],
    },
    "timeline_schemas": {
        "phase": "C1", "owner": "Timeline",
        "replacement": "Schema 5 only; old schemas unsupported and opaque",
        "pattern": r"SEQUENCE_SCHEMA|schema_version|supports_schema",
        "scope": ["crates/timeline/"],
        "whole_files": [],
    },
    "composition_schemas": {
        "phase": "C1", "owner": "Composition",
        "replacement": "Schema 3 writer/reader for current semantics; old schemas unsupported",
        "pattern": r"schema_version|supports_schema|fn schema\(",
        "scope": ["crates/composition/"],
        "whole_files": [],
    },
    "compositor_generations": {
        "phase": "C1", "owner": "Compositor; Render for shared-kernel consumer review",
        "replacement": "Grade/Merge/Transform/Blur v2; specify box blur separately if retained",
        "pattern": r"\b(?:\w+::)*(?:Grade|Merge|Transform|Blur)\b|legacy blur|OPERATORS",
        "scope": ["crates/compositor/", "crates/app/", "crates/render/", "crates/timeline/", "crates/composition/", "demos/", "scripts/"],
        "whole_files": [],
    },
    "workspace_migrations": {
        "phase": "C1", "owner": "Platform and UI",
        "replacement": "Workspace v4 and current panel identities; reset unsupported sidecars",
        "pattern": r"workspace_migration|migration::upgrade|upgrade_legacy|migrate_workspace|fold\.ui\.animation|\[Window\]|legacy workspace",
        "scope": ["crates/platform/", "crates/ui/"],
        "whole_files": ["crates/platform/src/workspace_migration.rs"],
    },
    "archive_migrations": {
        "phase": "C1", "owner": "Project",
        "replacement": "Archive v3 only; retain opaque provider bytes/extensions, generic organization discovery",
        "pattern": r"archive\.version|const VERSION|discover_items|legacy_migration|legacy load",
        "scope": ["crates/project/"],
        "whole_files": [],
    },
    "compositor_projection": {
        "phase": "C3", "owner": "Compositor and its app consumers",
        "replacement": "Validated prepared configuration; native records remain authoritative",
        "pattern": r"OperationConfig|try_configuration\(|\.configuration\(|set_configuration\(|edit_configuration\(",
        "whole_files": [],
    },
    "vector_legacy": {
        "phase": "C7", "owner": "Render",
        "replacement": "Retained vector path after stroke/contour/transform qualification; retain shared geometry",
        "pattern": r"vector_legacy|cpu_transform_fallbacks|self\.legacy",
        "whole_files": ["crates/render/src/gpu/vector_legacy.rs", "crates/render/src/gpu/vector_legacy.wgsl"],
    },
}


def role(path):
    if path.startswith(("docs/evidence/", "docs/rewrite/evidence/", "docs/archive/")) or re.match(r"docs/phase-", path) or path == "dev-plan.md":
        return "historical_evidence_keep_dated"
    if path.startswith("docs/") or path.endswith(".md"):
        return "documentation_reconcile_current_guidance"
    if "/tests/" in path or "test" in Path(path).stem:
        return "tests_reauthor_current_behavior_keep_negative_coverage"
    if path.startswith("demos/"):
        return "demo_reauthor_through_public_tools"
    if path.startswith("scripts/") or "/examples/" in path:
        return "tool_update_callers"
    return "production_trace_and_remove_only_superseded_behavior"


def operations(value):
    if isinstance(value, dict):
        if isinstance(value.get("operation"), str) and isinstance(value.get("version"), int):
            yield (value["operation"], value["version"])
        for child in value.values():
            yield from operations(child)
    elif isinstance(value, list):
        for child in value:
            yield from operations(child)


def fixture(path):
    raw = path.read_bytes()
    entry = {"path": str(path.relative_to(ROOT)), "sha256": hashlib.sha256(raw).hexdigest(), "bytes": len(raw)}
    try:
        archive = json.loads(raw)
        entry["archive_version"] = archive["version"]
        entry["documents"] = []
        for key, document in archive["project"]["documents"].items():
            item = {"id": key, "type": document["type_id"], "schema": document["schema_version"], "phase": "C1", "action": "retain_current_fixture"}
            payload = document["payload"]
            try:
                decoded = json.loads(bytes(payload) if isinstance(payload, list) else payload)
                item["operations"] = [{"key": op, "version": version} for op, version in sorted(set(operations(decoded)))]
            except (ValueError, TypeError, UnicodeError):
                item["operations"] = []
                item["payload"] = "opaque_or_non_json"
            old = (item["type"] == "fold.timeline.video-layers"
                   or item["type"] == "fold.timeline.sequence" and item["schema"] != 5
                   or item["type"] == "fold.composition.document" and item["schema"] != 3
                   or item["type"] == "fold.motion.document"
                   or any(op["key"] in ("fold.motion.cloner", "fold.motion.distribute") and op["version"] == 1 for op in item["operations"])
                   or any(op["key"] in ("Grade", "Merge", "Transform", "Blur") and op["version"] == 1 for op in item["operations"]))
            if old:
                item["action"] = "reauthor_if_current_fixture_else_preserve_historical_evidence"
            if entry["path"].startswith("docs/"):
                item["action"] = "preserve_dated_evidence_reauthor_only_if_used_by_current_tests"
            entry["documents"].append(item)
    except (ValueError, KeyError, TypeError) as error:
        entry["parse_error"] = str(error)
        entry["action"] = "inspect_unsupported_archive_without_rewriting"
    return entry


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("output", type=Path)
    args = parser.parse_args()
    tracked = subprocess.check_output(["git", "ls-files", "-z"], cwd=ROOT).decode().split("\0")
    groups = {name: {**info, "references": []} for name, info in GROUPS.items()}
    fixtures = []
    for name in sorted(filter(None, tracked)):
        path = ROOT / name
        if not path.is_file() or name.startswith("docs/maintenance/") or name == "scripts/maintenance_inventory.py":
            continue
        if path.suffix == ".fold":
            fixtures.append(fixture(path))
            continue
        if path.suffix not in (".rs", ".py", ".md", ".sh", ".json", ".toml", ".wgsl", ".yml"):
            continue
        try:
            lines = path.read_text().splitlines()
        except UnicodeError:
            continue
        for group in groups.values():
            if "scope" in group and not any(name.startswith(prefix) for prefix in group["scope"]):
                continue
            pattern = re.compile(group["pattern"])
            for number, line in enumerate(lines, 1):
                if pattern.search(line):
                    group["references"].append({"path": name, "line": number, "role": role(name), "text": line.strip()[:240]})
    result = {
        "baseline_commit": subprocess.check_output(["git", "rev-parse", "HEAD"], cwd=ROOT, text=True).strip(),
        "scope": "Tracked repository text plus decoded .fold archives. Semantic/alias closure and dependency decisions are in c0-inventory.md; matches are not whole-file deletion instructions.",
        "groups": groups, "fixtures": fixtures,
        "manifest_disposition": "Retain all workspace crates. No third-party dependency is proven exclusive to these deletions; audit retained render/media consumers before removing a dependency.",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(result, indent=2) + "\n")
    print(json.dumps({"groups": len(groups), "references": sum(len(g["references"]) for g in groups.values()), "fixtures": len(fixtures), "parse_errors": sum("parse_error" in f for f in fixtures)}))


if __name__ == "__main__":
    main()
