import importlib.util
from pathlib import Path
import os
import signal
import subprocess
import sys
import tempfile
import json
import unittest
from unittest.mock import Mock

spec = importlib.util.spec_from_file_location('record_trace', Path(__file__).with_name('record.py'))
m = importlib.util.module_from_spec(spec)


class LifecycleTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec.loader.exec_module(m)

    def test_cleanup_reaps_actual_stopped_owned_child(self):
        child = subprocess.Popen([sys.executable, '-c', 'import os,signal,time;os.kill(os.getpid(),signal.SIGSTOP);time.sleep(60)'])
        try:
            m.wait_stopped(child, timeout=2)
            m.cleanup([child])
            self.assertIsNotNone(child.returncode)
        finally:
            if child.poll() is None:
                child.kill();child.wait()

    def test_cleanup_attempts_other_owners_after_first_error(self):
        a, b = Mock(), Mock()
        a.poll.side_effect = OSError('probe failed')
        b.poll.return_value = 0
        b.wait.return_value = 0
        with self.assertRaises(OSError):
            m.cleanup([a,b])
        b.wait.assert_called_once()

    def test_exit_before_stop_is_an_error_with_status_preserved(self):
        child = subprocess.Popen([sys.executable, '-c', 'raise SystemExit(7)'])
        with self.assertRaises(RuntimeError):
            m.wait_stopped(child, timeout=2)
        self.assertEqual(child.returncode,7)

    def test_trace_limit_must_cover_child_and_startup_budget(self):
        m.validate_budgets(240,300,45)
        with self.assertRaises(ValueError):m.validate_budgets(240,60,45)

    def test_frame_stderr_is_separate_from_parallel_stdout(self):
        script='''import sys,threading,json
def noise():
 for _ in range(100):sys.stdout.write("noise"*1000+"\\n");sys.stdout.flush()
t=threading.Thread(target=noise);t.start()
for i in range(50):sys.stderr.write(json.dumps({"frame":i})+"\\n");sys.stderr.flush()
t.join()
'''
        with tempfile.TemporaryDirectory() as td:
            out,err=Path(td)/'stdout',Path(td)/'stderr'
            with out.open('wb') as o,err.open('wb') as e:
                child=m.spawn_target([sys.executable,'-c',script],os.environ.copy(),o,e)
                self.assertEqual(child.wait(timeout=5),0)
            self.assertEqual([json.loads(l)['frame'] for l in err.read_text().splitlines()],list(range(50)))


if __name__ == '__main__':
    unittest.main()
