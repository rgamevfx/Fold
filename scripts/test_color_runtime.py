"""Cache publication/recovery tests; real OCIO is exercised by fold-color tests."""
import hashlib
import json
from pathlib import Path
import tempfile
import subprocess
import unittest
from unittest.mock import patch

import build_color_runtime as color


def fake_produce(_build, package):
    library = package / "lib/fold/libfold_ocio.so"
    library.parent.mkdir(parents=True)
    library.write_bytes(b"native runtime")
    manifest = package / "share/fold/color/manifest.json"
    manifest.parent.mkdir(parents=True)
    manifest.write_text(json.dumps({"files": {
        str(library.relative_to(package)): hashlib.sha256(library.read_bytes()).hexdigest()
    }}))


class ColorCacheTests(unittest.TestCase):
    def test_yaml_header_backport_is_idempotent(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / "src"
            source.mkdir()
            emitter = source / "emitterutils.cpp"
            emitter.write_text("#include <algorithm>\nint main() { uint16_t value = 0; return value; }\n")
            for _ in range(2):
                subprocess.run(["cmake", f"-DSOURCE_DIR={root}", "-P",
                    str(color.ROOT / "resources/color/patch-yaml-cpp.cmake")], check=True)
            self.assertEqual(emitter.read_text().count("#include <cstdint>"), 1)
            subprocess.run(["c++", "-std=c++17", "-fsyntax-only", str(emitter)], check=True)

    def test_reuse_corruption_and_input_change(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(color, "fingerprint", return_value="first") as identity, \
                patch.object(color, "produce", side_effect=fake_produce) as produce:
            cache = Path(directory)
            package = color.ensure(cache)
            self.assertEqual(color.ensure(cache), package)
            self.assertEqual(produce.call_count, 1)
            (package / "lib/fold/libfold_ocio.so").write_bytes(b"broken")
            self.assertEqual(color.ensure(cache), package)
            self.assertEqual(produce.call_count, 2)
            identity.return_value = "second"
            self.assertNotEqual(color.ensure(cache), package)
            self.assertEqual(produce.call_count, 3)

    def test_failed_build_is_not_published_and_can_retry(self):
        with tempfile.TemporaryDirectory() as directory, \
                patch.object(color, "fingerprint", return_value="identity"), \
                patch.object(color, "produce", side_effect=RuntimeError("interrupted")):
            cache = Path(directory)
            with self.assertRaisesRegex(RuntimeError, "interrupted"):
                color.ensure(cache)
            self.assertFalse((cache / "identity/package").exists())
            with patch.object(color, "produce", side_effect=fake_produce):
                self.assertTrue(color.valid_cache(color.ensure(cache), "identity"))


if __name__ == "__main__":
    unittest.main()
