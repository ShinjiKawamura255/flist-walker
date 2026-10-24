import importlib.util
from pathlib import Path
import struct
import unittest

spec = importlib.util.spec_from_file_location('analyze', Path(__file__).with_name('linux-analyze.py'))
a = importlib.util.module_from_spec(spec)


class AnalyzeTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        spec.loader.exec_module(a)

    def test_switch_identity_and_preempt_flag(self):
        b = struct.pack('<IHHIIQ', 14, 0x6000, 24, 11, 12, 500)
        self.assertEqual(a.switch_record(b), dict(pid=11, tid=12, ns=500, out=True, preempt=True))
        with self.assertRaises(ValueError):
            a.switch_record(b + b'bad')

    def test_overlap_matches_pid_tid_and_removes_full_bracket(self):
        events = [dict(pid=11, tid=12, ns=100, out=True, preempt=True), dict(pid=11, tid=12, ns=200, out=False, preempt=False)]
        frame = dict(pid=11, native_thread_id=12, monotonic_window_ns=[90,210], measurement_overhead_ns=20)
        row = a.overlap(frame, events)
        self.assertEqual(row['switch_off_core_overlap_ns'], 80)
        self.assertEqual(row['preempt_out_overlap_ns'], 80)
        frame['pid'] = 999
        self.assertIsNone(a.overlap(frame, events)['switch_off_core_overlap_ns'])

    def test_invalid_pair_does_not_become_zero_wait(self):
        events = [dict(pid=11, tid=12, ns=100, out=True, preempt=False), dict(pid=11, tid=12, ns=110, out=True, preempt=False)]
        frame = dict(pid=11, native_thread_id=12, monotonic_window_ns=[90,210], measurement_overhead_ns=0)
        self.assertIsNone(a.overlap(frame, events)['switch_off_core_overlap_ns'])

    def test_per_cpu_buffer_delivery_is_sorted_by_same_clock(self):
        events = [dict(pid=11, tid=12, ns=200, out=False, preempt=False), dict(pid=11, tid=12, ns=100, out=True, preempt=True)]
        frame = dict(pid=11, native_thread_id=12, monotonic_window_ns=[100,200], measurement_overhead_ns=0)
        self.assertEqual(a.overlap(frame, events)['switch_off_core_overlap_ns'], 100)

    def test_bounds_not_inferred_from_cpu_clock_inconsistent_with_wall(self):
        self.assertEqual(a.clock_consistency(dict(cpu_lower_ns=120,wall_ns=118)), 'INCONSISTENT')
        self.assertEqual(a.clock_consistency(dict(cpu_lower_ns=100,wall_ns=118)), 'NOT DISPROVEN; not a calibration')


if __name__ == '__main__':
    unittest.main()
