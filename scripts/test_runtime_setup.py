"""Native setup regression tests without downloading or compiling upstream code."""
import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import runtime_cache as cache
from prepare_runtimes import source_files


class RuntimeCacheTests(unittest.TestCase):
    def test_complete_cache_reuse_corruption_and_identity_change(self):
        calls = []
        def produce(work, package):
            calls.append(work)
            package.mkdir()
            (package / 'library').write_bytes(b'complete')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            package = cache.ensure(root, 'one', produce)
            self.assertEqual(package, cache.ensure(root, 'one', produce))
            self.assertEqual(len(calls), 1)
            (package / 'library').write_bytes(b'corrupt')
            cache.ensure(root, 'one', produce)
            self.assertEqual(len(calls), 2)
            self.assertNotEqual(package, cache.ensure(root, 'two', produce))

    def test_interrupted_producer_does_not_publish(self):
        def fail(work, package):
            package.mkdir()
            (package / 'partial').touch()
            raise RuntimeError('interrupted')
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            with self.assertRaisesRegex(RuntimeError, 'interrupted'):
                cache.ensure(root, 'one', fail)
            self.assertFalse((root / 'one/package').exists())

    def test_nested_cargo_does_not_inherit_parent_jobserver_or_target(self):
        with patch.dict(os.environ, {'CARGO_MAKEFLAGS': 'jobserver', 'CARGO_TARGET_DIR': '/parent',
                                     'RUSTFLAGS': 'parent flags', 'CARGO_BUILD_JOBS': '32'}):
            env = cache.build_environment()
            self.assertNotIn('CARGO_MAKEFLAGS', env)
            self.assertNotIn('CARGO_TARGET_DIR', env)
            self.assertNotIn('RUSTFLAGS', env)
            self.assertEqual(env['CARGO_BUILD_JOBS'], '4')

    def test_sdk_identity_excludes_generated_targets(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'src').mkdir()
            (root / 'src/lib.rs').touch()
            (root / 'target').mkdir()
            (root / 'target/large-build').touch()
            self.assertEqual(source_files(root), [root / 'src/lib.rs'])

    def test_pins_have_https_urls_and_sha256(self):
        pins = json.loads((cache.ROOT / 'resources/motion-context/runtime-inputs.json').read_text())
        for name in ('skia_source', 'skia_reference', 'ffmpeg'):
            self.assertTrue(pins[name]['url'].startswith('https://'))
            self.assertEqual(len(bytes.fromhex(pins[name]['sha256'])), 32)


if __name__ == '__main__':
    unittest.main()
