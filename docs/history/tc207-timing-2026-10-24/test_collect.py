"""POSIX lifecycle regression; fake child, never timing/performance evidence."""
import os
from pathlib import Path
import signal
import subprocess
import sys
import tempfile
import unittest


class StoppedChildCleanupTests(unittest.TestCase):
    def check_interruption(self, signum):
        collector = Path(__file__).with_name("collect.py").resolve()
        with tempfile.TemporaryDirectory(prefix="tc207-cleanup-") as temporary:
            root = Path(temporary)
            pid_file = root / "child.pid"
            fake = root / "fake-libtest"
            fake.write_text("#!/usr/bin/env python3\n"
                            "import os,signal\n"
                            "from pathlib import Path\n"
                            "Path(os.environ['TC207_CLEANUP_PID_FILE']).write_text(str(os.getpid()))\n"
                            "os.kill(os.getpid(),signal.SIGSTOP)\n")
            fake.chmod(0o700)
            # Trigger the signal only after run() has observed WIFSTOPPED and
            # entered its known stopped interval. No sleep-based race guessing.
            driver = """
import importlib.util,os,signal,sys
from pathlib import Path
spec=importlib.util.spec_from_file_location('collector',sys.argv[1])
mod=importlib.util.module_from_spec(spec)
spec.loader.exec_module(mod)
if hasattr(mod,'install_termination_handler'):
    mod.install_termination_handler()
original_sleep=mod.time.sleep
def interrupt_stopped_interval(seconds):
    if seconds==0.150:
        print('CLEANUP_STOP_OBSERVED',flush=True)
        os.kill(os.getpid(),int(sys.argv[4]))
    else:
        original_sleep(seconds)
mod.time.sleep=interrupt_stopped_interval
mod.run(Path(sys.argv[2]),Path(sys.argv[3]),'interrupted-stop','stop')
"""
            env = os.environ.copy()
            env["TC207_CLEANUP_PID_FILE"] = str(pid_file)
            process = subprocess.Popen(
                [sys.executable, "-c", driver, str(collector), str(fake), str(root), str(signum)],
                env=env, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                text=True, start_new_session=True,
            )
            try:
                stdout, stderr = process.communicate(timeout=10)
                self.assertIn("CLEANUP_STOP_OBSERVED", stdout, stderr)
                self.assertTrue(pid_file.exists(), "fake child never started")
                child_pid = int(pid_file.read_text())
                with self.assertRaises(ProcessLookupError,
                                       msg=f"stopped child {child_pid} survived parent signal {signum}"):
                    os.kill(child_pid, 0)
                if signum == signal.SIGTERM:
                    self.assertEqual(process.returncode, 128 + signal.SIGTERM, stderr)
                else:
                    self.assertNotEqual(process.returncode, 0)
            finally:
                # Also clean the known owned process group when exercising the
                # broken collector: it may have orphaned its stopped child.
                for cleanup_signal in [signal.SIGCONT, signal.SIGKILL]:
                    try:
                        os.killpg(process.pid, cleanup_signal)
                    except ProcessLookupError:
                        pass
                process.wait(timeout=5)

    def test_sigterm_after_observed_stop_resumes_kills_and_reaps_child(self):
        self.check_interruption(signal.SIGTERM)

    def test_sigint_after_observed_stop_reaps_child(self):
        self.check_interruption(signal.SIGINT)


if __name__ == "__main__":
    unittest.main()
