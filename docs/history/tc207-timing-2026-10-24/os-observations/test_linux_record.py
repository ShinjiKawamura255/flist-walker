import importlib.util
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest


class RecordTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec = importlib.util.spec_from_file_location('record', Path(__file__).with_name('linux-record.py'))
        cls.r = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cls.r)

    def test_failed_command_retains_exit_and_logs(self):
        with tempfile.TemporaryDirectory() as td:
            row = self.r.command([sys.executable, '-c', 'print("evidence");raise SystemExit(7)'], Path(td) / 'x')
            self.assertEqual(row['exit'], 7)
            self.assertIn('evidence', (Path(td) / 'x.stdout').read_text())

    def test_timeout_kills_and_reaps_owned_group(self):
        with tempfile.TemporaryDirectory() as td:
            row = self.r.command([sys.executable, '-c', 'import time;time.sleep(30)'], Path(td) / 'x', timeout=.05)
            self.assertTrue(row['timeout'])
            self.assertLess(row['exit'], 0)

    def test_child_receipt_required_independently_of_perf(self):
        self.assertFalse(self.r.valid_result({'exit': 0, 'timeout': False}, None))
        self.assertFalse(self.r.valid_result({'exit': 0, 'timeout': False}, {'experiment_valid': False}))
        self.assertTrue(self.r.valid_result({'exit': 0, 'timeout': False}, {'experiment_valid': True}))

    def test_capture_is_task_scoped_and_uses_monotonic(self):
        argv = self.r.record_argv('/perf', Path('/owned'), Path('/binary'), 'control-cpu', 'cpu')
        self.assertNotIn('-a', argv)
        self.assertIn('--switch-events', argv)
        self.assertEqual(argv[argv.index('--clockid') + 1], 'mono')


if __name__ == '__main__':
    unittest.main()
