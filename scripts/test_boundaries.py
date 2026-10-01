"""Regression tests for dependency policy enforcement (no third-party tools)."""

import unittest

from check_boundaries import ALLOWED, violations


def fixture():
    return {
        "packages": [
            {"id": name, "name": name, "dependencies": []} for name in sorted(ALLOWED)
        ],
        "workspace_members": sorted(ALLOWED),
        "resolve": {"nodes": [{"id": name, "deps": []} for name in sorted(ALLOWED)]},
    }


def add_edge(data, source, destination, resolved=True, **extra):
    package = next(p for p in data["packages"] if p["id"] == source)
    package["dependencies"].append({"name": destination, **extra})
    if resolved:
        node = next(n for n in data["resolve"]["nodes"] if n["id"] == source)
        node["deps"].append({"pkg": destination})


class BoundaryTests(unittest.TestCase):
    def test_allowed_edges(self):
        data = fixture()
        for source, targets in ALLOWED.items():
            for target in targets:
                add_edge(data, source, target, optional=(target == "fold-ui"))
        self.assertEqual(violations(data), [])

    def test_infrastructure_cannot_import_creative_packages(self):
        for source in ("fold-project", "fold-render", "fold-media", "fold-platform"):
            for target in ("fold-timeline", "fold-compositor", "fold-motion"):
                with self.subTest(source=source, target=target):
                    data = fixture()
                    add_edge(data, source, target)
                    self.assertIn(
                        f"forbidden dependency: {source} -> {target}", violations(data)
                    )

    def test_peer_imports_are_rejected_even_when_aliased_or_disabled(self):
        data = fixture()
        add_edge(
            data, "fold-timeline", "fold-compositor", resolved=False,
            rename="graph", optional=True, kind="dev", target="cfg(windows)",
        )
        self.assertIn(
            "forbidden dependency: fold-timeline -> fold-compositor", violations(data)
        )

    def test_headless_rejects_transitive_ui(self):
        data = fixture()
        add_edge(data, "fold-app", "fold-timeline")
        add_edge(data, "fold-timeline", "fold-ui")
        self.assertIn(
            "forbidden transitive dependency: fold-app -> fold-ui",
            violations(data, headless=True),
        )

    def test_disabled_desktop_dependency_is_headless_safe(self):
        data = fixture()
        add_edge(data, "fold-app", "fold-ui", resolved=False, optional=True)
        self.assertEqual(violations(data, headless=True), [])

    def test_external_imgui_binding_is_rejected_transitively(self):
        data = fixture()
        for name in ("helper", "dear-imgui-sys"):
            data["packages"].append({"id": name, "name": name, "dependencies": []})
            data["resolve"]["nodes"].append({"id": name, "deps": []})
        add_edge(data, "fold-app", "helper")
        add_edge(data, "helper", "dear-imgui-sys")
        self.assertIn(
            "forbidden transitive dependency: fold-app -> dear-imgui-sys",
            violations(data, headless=True),
        )

    def test_external_bridge_cannot_hide_peer_dependency(self):
        data = fixture()
        data["packages"].append({"id": "helper", "name": "helper", "dependencies": []})
        data["resolve"]["nodes"].append({"id": "helper", "deps": []})
        add_edge(data, "fold-timeline", "helper")
        add_edge(data, "helper", "fold-compositor")
        self.assertIn(
            "forbidden transitive dependency: fold-timeline -> fold-compositor",
            violations(data),
        )

    def test_native_panel_sdk_is_optional_and_headless_excluded(self):
        data = fixture()
        add_edge(data, "fold-timeline", "fold-ui", optional=True)
        self.assertEqual(violations(data), [])
        self.assertIn("forbidden transitive dependency: fold-timeline -> fold-ui", violations(data, headless=True))
        data = fixture()
        add_edge(data, "fold-timeline", "fold-ui", optional=False)
        self.assertTrue(violations(data))
        data = fixture()
        add_edge(data, "fold-timeline", "dear-imgui-rs", resolved=False, optional=True)
        self.assertTrue(violations(data))

    def test_new_workspace_members_require_policy(self):
        data = fixture()
        data["packages"].append({"id": "extra", "name": "extra", "dependencies": []})
        data["workspace_members"].append("extra")
        self.assertTrue(violations(data))


if __name__ == "__main__":
    unittest.main()
