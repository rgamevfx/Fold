import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class MeasurementTests(unittest.TestCase):
    def run_probe(self, root, code, timeout='5'):
        return subprocess.run([
            sys.executable, str(Path(__file__).with_name('measure_process.py')),
            '--report', str(root / 'report.json'), '--scratch', str(root / 'scratch'),
            '--timeout', timeout, '--', sys.executable, '-c', code,
        ], capture_output=True, timeout=10)

    def test_records_process_tree_and_dedicated_scratch(self):
        with tempfile.TemporaryDirectory() as path:
            root = Path(path)
            result = self.run_probe(root, "import os,pathlib,time; pathlib.Path(os.environ['TMPDIR'],'scratch').write_bytes(b'x'*4096); time.sleep(.4)")
            self.assertEqual(result.returncode, 0, result.stderr)
            report = json.loads((root / 'report.json').read_text())
            self.assertFalse(report['timed_out'])
            self.assertGreater(report['sampled_tree_rss_bytes'], 0)
            self.assertGreaterEqual(report['sampled_scratch_bytes'], 4096)

    def test_timeout_stops_command_and_records_failure(self):
        with tempfile.TemporaryDirectory() as path:
            root = Path(path)
            result = self.run_probe(root, 'import time; time.sleep(30)', '.1')
            self.assertNotEqual(result.returncode, 0)
            report = json.loads((root / 'report.json').read_text())
            self.assertTrue(report['timed_out'])
            self.assertNotEqual(report['exit_code'], 0)

    def test_existing_report_is_not_overwritten(self):
        with tempfile.TemporaryDirectory() as path:
            root = Path(path)
            (root / 'report.json').write_text('keep')
            self.assertNotEqual(self.run_probe(root, 'pass').returncode, 0)
            self.assertEqual((root / 'report.json').read_text(), 'keep')
            self.assertFalse((root / 'scratch').exists())


if __name__ == '__main__':
    unittest.main()
