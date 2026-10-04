"""Enforce Fold's crate graph using Cargo package identities, not import text."""

import json
from pathlib import Path
import subprocess
import sys

ROOT = Path(__file__).resolve().parents[1]
SHARED = {"fold-animation", "fold-foundation", "fold-color", "fold-project", "fold-media", "fold-render", "fold-platform"}
CREATIVE = {"fold-timeline", "fold-compositor", "fold-motion"}
ALLOWED = {
    "fold-foundation": set(),
    "fold-animation": {"fold-foundation"},
    "fold-color": set(),
    "fold-native-video": set(),
    "fold-project": {"fold-foundation"},
    "fold-media": {"fold-foundation", "fold-native-video"},
    "fold-render": {"fold-foundation", "fold-color", "fold-media", "fold-native-video"},
    "fold-platform": SHARED - {"fold-platform"},
    "fold-ui": {"fold-animation", "fold-foundation", "fold-platform"},
    **{name: SHARED | {"fold-ui"} for name in CREATIVE},
    "fold-app": SHARED | CREATIVE | {"fold-ui"},
}


def is_ui(name):
    # Cover both binding families and their backend/sys companion crates.
    return name == "fold-ui" or "imgui" in name.lower()


def reachable(metadata, root):
    nodes = {node["id"]: node for node in metadata["resolve"]["nodes"]}
    seen = set()
    pending = [root]
    while pending:
        current = pending.pop()
        if current in seen:
            continue
        seen.add(current)
        pending.extend(dep["pkg"] for dep in nodes[current]["deps"])
    return seen


def violations(metadata, headless=False):
    packages = {package["id"]: package for package in metadata["packages"]}
    members = [packages[identity] for identity in metadata["workspace_members"]]
    errors = []
    names = {package["name"] for package in members}
    if names != ALLOWED.keys():
        errors.append(f"workspace packages differ from policy: {sorted(names ^ ALLOWED.keys())}")
    for package in members:
        name = package["name"]
        if name not in ALLOWED:
            continue
        # Inspect declarations too: optional, dev, build, and target-specific
        # workspace dependencies must obey the policy even when not resolved.
        for dependency in package["dependencies"]:
            target = dependency["name"]  # Cargo reports the original name for aliases.
            if name in CREATIVE and is_ui(target):
                if target != "fold-ui" or not dependency.get("optional", False):
                    errors.append(f"creative UI must use an optional shared SDK: {name} -> {target}")
            if target in names and target not in ALLOWED[name]:
                errors.append(f"forbidden dependency: {name} -> {target}")
        # Third-party crates must not smuggle creative implementations or UI
        # back into infrastructure. Check the resolved transitive graph too.
        if name in SHARED | CREATIVE or (headless and name == "fold-app"):
            for identity in sorted(reachable(metadata, package["id"])):
                target = packages[identity]["name"]
                forbidden_peer = name != "fold-app" and target in CREATIVE and target != name
                forbidden_ui = is_ui(target) and (name in SHARED or headless)
                if forbidden_ui or forbidden_peer:
                    errors.append(f"forbidden transitive dependency: {name} -> {target}")
    return errors


def metadata(*flags):
    output = subprocess.check_output(
        ["cargo", "metadata", "--format-version", "1", "--locked", *flags],
        cwd=ROOT,
        text=True,
    )
    return json.loads(output)


def main():
    errors = violations(metadata("--all-features"))
    errors += violations(metadata("--no-default-features"), headless=True)
    if errors:
        print("\n".join(sorted(set(errors))), file=sys.stderr)
        return 1
    print("Package boundaries and headless UI isolation passed.")
    return 0


if __name__ == "__main__":
    sys.exit(main())
