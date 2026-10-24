import importlib.util
from pathlib import Path
import unittest
from unittest.mock import Mock, patch
import tempfile
import signal
import json
import gzip
import subprocess
import sys

spec = importlib.util.spec_from_file_location('observe', Path(__file__).with_name('observe.py'))
observe = importlib.util.module_from_spec(spec)
spec.loader.exec_module(observe)


class CorrelationTests(unittest.TestCase):
    def frame(self):
        return dict(pid=7, native_thread_id=8, monotonic_window_ns=[100, 300],
                    measurement_overhead_ns=10, wall_ns=101_000_000,
                    wall_under_100ms=False)

    def row(self, start=120, end=130, state=3, cpu=100):
        return dict(pid=7, tid=8, start_ns=start, end_ns=end, state=state, cpu_ns=cpu)

    def test_exact_identity_and_inner_window_only(self):
        rows = [self.row(), self.row(start=99), self.row(end=291),
                dict(self.row(), pid=9), dict(self.row(), tid=9)]
        r = observe.correlate(self.frame(), rows)
        self.assertEqual(r['state_sample_counts'], {'3': 1})
        self.assertEqual(r['sample_count'], 1)
        self.assertFalse(r['wall_under_100ms'])
        self.assertNotIn('scheduler_wait_ns', r)

    def test_running_and_waiting_are_counts_not_durations(self):
        r = observe.correlate(self.frame(), [self.row(state=1), self.row(state=3, cpu=200)])
        self.assertEqual(r['state_sample_counts'], {'1': 1, '3': 1})
        self.assertEqual(r['sampled_cpu_advance_ns'], 100)

    def test_missing_or_uncertain_window_has_no_state_verdict(self):
        for change in (dict(monotonic_window_ns=None), dict(native_thread_id=None),
                       dict(measurement_overhead_ns=200)):
            r = observe.correlate(dict(self.frame(), **change), [self.row()])
            self.assertEqual(r['sample_count'], 0)
            self.assertIsNone(r['sampled_cpu_advance_ns'])

    def test_cpu_reset_is_unavailable_not_zero(self):
        r = observe.correlate(self.frame(), [self.row(cpu=200), self.row(start=140,end=150,cpu=100)])
        self.assertIsNone(r['sampled_cpu_advance_ns'])
        self.assertTrue(r['cpu_counter_retrograde'])

    def test_observer_error_still_resumes_kills_reaps_owned_child(self):
        child = Mock(pid=777777, returncode=-9)
        child.poll.return_value = None
        observer = Mock()
        observer.snapshot.side_effect = RuntimeError('observation failed')
        with tempfile.TemporaryDirectory() as d, \
             patch.object(observe, 'ProcObserver', return_value=observer), \
             patch.object(observe.subprocess, 'Popen', return_value=child), \
             patch.object(observe.os, 'kill') as kill:
            with self.assertRaisesRegex(RuntimeError, 'observation failed'):
                observe.run(Path('/inert-test-binary'), Path(d), 'cleanup')
            kill.assert_called_once_with(child.pid, signal.SIGCONT)
            child.kill.assert_called_once()
            child.wait.assert_called_once_with(timeout=5)
            self.assertTrue((Path(d)/'cleanup.lifecycle.json').exists())
            self.assertEqual(json.loads(gzip.decompress((Path(d)/'cleanup.states.json.gz').read_bytes())), [])

    def test_invalid_experiment_exits_nonzero_after_summary(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            binary = root/'inert'
            binary.write_bytes(b'inert')
            output = root/'results'
            with patch.object(sys, 'argv', ['observe','--binary',str(binary),'--output',str(output)]), \
                 patch.object(observe, 'run', return_value={'experiment_valid':False}), \
                 patch.object(observe.platform, 'platform', return_value='test-fixture'), \
                 patch.object(observe.subprocess, 'check_output', return_value='source\n'):
                with self.assertRaises(SystemExit) as exc:
                    observe.main()
            self.assertEqual(exc.exception.code, 1)
            self.assertFalse(json.loads((output/'summary.json').read_text())['all_experiments_valid'])

    def test_profiler_timeout_retains_partial_states_and_lifecycle(self):
        child = Mock(pid=777777, returncode=0)
        child.poll.side_effect = [None,0,0]
        profiler = Mock(pid=777778, returncode=-9)
        profiler.poll.return_value = None
        profiler.wait.side_effect = [subprocess.TimeoutExpired('sample',3),-9]
        observer = Mock()
        observer.snapshot.return_value = [self.row()]
        with tempfile.TemporaryDirectory() as d, \
             patch.object(observe, 'ProcObserver', return_value=observer), \
             patch.object(observe.subprocess, 'Popen', side_effect=[child,profiler]), \
             patch.object(observe.os, 'kill'):
            with self.assertRaises(Exception):
                observe.run(Path('/inert'),Path(d),'timeout',stacks=True)
            self.assertTrue((Path(d)/'timeout.lifecycle.json').exists())
            self.assertEqual(json.loads(gzip.decompress((Path(d)/'timeout.states.json.gz').read_bytes())),[self.row()])
            self.assertTrue((Path(d)/'timeout.failure.json').exists())

    def test_child_cleanup_failure_does_not_skip_profiler_cleanup(self):
        child = Mock(pid=777777,returncode=None)
        child.poll.return_value = None
        profiler = Mock(pid=777778,returncode=0)
        observer = Mock()
        observer.snapshot.side_effect = RuntimeError('original observation failure')
        with tempfile.TemporaryDirectory() as d, \
             patch.object(observe,'ProcObserver',return_value=observer), \
             patch.object(observe.subprocess,'Popen',side_effect=[child,profiler]), \
             patch.object(observe,'cleanup',side_effect=[subprocess.TimeoutExpired('child',5),None]) as cleanup:
            with self.assertRaisesRegex(RuntimeError,'original observation failure'):
                observe.run(Path('/inert'),Path(d),'independent-cleanup',stacks=True)
            self.assertEqual([call.args[0] for call in cleanup.call_args_list],[child,profiler])
            lifecycle=json.loads((Path(d)/'independent-cleanup.lifecycle.json').read_text())
            self.assertEqual(lifecycle['cleanup_errors'][0]['stage'],'child-cleanup')
            self.assertTrue((Path(d)/'independent-cleanup.states.json.gz').exists())

    def test_startup_interrupt_after_spawn_is_guarded(self):
        child=Mock(pid=777777,returncode=-9)
        child.poll.return_value=None
        with tempfile.TemporaryDirectory() as d, \
             patch.object(observe,'ProcObserver'), \
             patch.object(observe.subprocess,'Popen',return_value=child), \
             patch.object(observe.time,'monotonic',side_effect=SystemExit(143)), \
             patch.object(observe.os,'kill'):
            with self.assertRaises(SystemExit):
                observe.run(Path('/inert'),Path(d),'startup')
            child.kill.assert_called_once()
            child.wait.assert_called_once_with(timeout=5)
            self.assertTrue((Path(d)/'startup.lifecycle.json').exists())

    def test_sample_cap_fails_without_discarding_existing_rows(self):
        rows=[self.row()]
        with self.assertRaisesRegex(RuntimeError,'sample row cap'):
            observe.extend_samples(rows,[self.row()],limit=1)
        self.assertEqual(rows,[self.row()])

    def test_cleanup_is_idempotent_after_owned_reap(self):
        child=Mock(pid=777777,returncode=-9)
        child.poll.side_effect=[None,-9]
        with patch.object(observe.os,'kill') as kill:
            observe.cleanup(child)
            observe.cleanup(child)
        kill.assert_called_once_with(child.pid,signal.SIGCONT)
        child.kill.assert_called_once()
        child.wait.assert_called_once_with(timeout=5)


if __name__ == '__main__':
    unittest.main()
