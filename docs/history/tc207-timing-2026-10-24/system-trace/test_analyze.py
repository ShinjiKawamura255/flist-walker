import importlib.util
from pathlib import Path
import unittest
import xml.etree.ElementTree as E
import tempfile

spec=importlib.util.spec_from_file_location('analyze_trace',Path(__file__).with_name('analyze.py'))
m=importlib.util.module_from_spec(spec)


class AnalysisTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):spec.loader.exec_module(m)

    def test_calibration_keeps_quantization_and_both_anchors(self):
        anchors=[dict(mono_before_ns=1100,mono_after_ns=1110,trace_ns=100),dict(mono_before_ns=2105,mono_after_ns=2115,trace_ns=1100)]
        self.assertEqual(m.calibrate(anchors,tick=2),[1003,1012])
        anchors[1]['trace_ns']=500
        with self.assertRaises(ValueError):m.calibrate(anchors,tick=2)

    def test_gap_is_unknown_and_interrupted_is_separate(self):
        rows=[dict(start=10,end=20,state='Runnable'),dict(start=22,end=30,state='Interrupted')]
        totals=m.integrate(rows,10,30)
        self.assertEqual(totals,dict(Runnable=10,Interrupted=8,UNKNOWN=2))

    def test_overlapping_states_rejected(self):
        with self.assertRaises(ValueError):m.integrate([dict(start=0,end=20,state='Running'),dict(start=10,end=30,state='Blocked')],0,30)

    def test_uncertainty_removes_edges_before_state_attribution(self):
        f=dict(monotonic_window_ns=[1100,1200],measurement_overhead_ns=5)
        self.assertEqual(m.frame_window(f,[1000,1010]),[105,185])

    def test_missing_xml_reference_fails_closed(self):
        with self.assertRaises(ValueError):m.resolve(E.fromstring('<thread ref="missing"/>'),{})

    def test_native_mach_offset_uses_integer_timebase_not_monotonic(self):
        rows=[dict(mach_before=100,mach_after=102,timebase_numer=125,timebase_denom=3,trace_ns=1000),
              dict(mach_before=200,mach_after=202,timebase_numer=125,timebase_denom=3,trace_ns=5166)]
        self.assertEqual(m.mach_calibrate(rows),[3125,3292])

    def test_native_frame_window_does_not_use_unadjusted_mono_offset(self):
        f=dict(monotonic_window_ns=[99999,100999],mach_absolute_window_ns=[1100,1200],measurement_overhead_ns=5)
        self.assertEqual(m.frame_window(f,[1000,1010],clock_field='mach_absolute_window_ns'),[105,185])

    def test_schema3_missing_mach_never_falls_back_to_monotonic(self):
        with self.assertRaises(ValueError):m.clock_bridge([dict(schema=3,mach_absolute_window_ns=None)],[])

    def test_native_raw_ticks_and_timebase_must_match_ns_and_anchors(self):
        anchors=[dict(mach_before=100,mach_after=102,timebase_numer=125,timebase_denom=3,trace_ns=1000),
                 dict(mach_before=200,mach_after=202,timebase_numer=125,timebase_denom=3,trace_ns=5166)]
        f=dict(schema=3,mach_absolute_window_ticks=[100,102],mach_absolute_window_ns=[4166,4250],mach_timebase=[125,3])
        self.assertEqual(m.clock_bridge([f],anchors),('mach_absolute_window_ns',[3125,3292]))
        f['mach_absolute_window_ns']=[4167,4250]
        with self.assertRaises(ValueError):m.clock_bridge([f],anchors)
        f['mach_absolute_window_ns']=[4166,4250];f['mach_timebase']=[1,1]
        with self.assertRaises(ValueError):m.clock_bridge([f],anchors)

    def test_context_profile_does_not_duplicate_primary_cpu_samples(self):
        self.assertEqual(m.profile_sample_kind({'context-switch-sampling':'0'},'Running'),'cpu')
        self.assertEqual(m.profile_sample_kind({'context-switch-sampling':'1'},'Running'),'supplemental')
        self.assertEqual(m.profile_sample_kind({'context-switch-sampling':'0'},''),'supplemental')

    def test_frame_integrity_rejects_missing_and_inconsistent_gate(self):
        f=dict(schema=3,pid=1,native_thread_id=2,test='case',request_id=4,frame=0,frames_observed=1,
               wall_ns=101000000,wall_under_100ms=False,measurement_overhead_ns=5,mach_absolute_window_ns=[1000,101001005])
        m.validate_frames([f],1)
        f['wall_under_100ms']=True
        with self.assertRaises(ValueError):m.validate_frames([f],1)
        with self.assertRaises(ValueError):m.validate_frames([{'schema':3}],1)

    def test_frame_sequence_and_dump_count_are_required(self):
        f=dict(schema=3,pid=1,native_thread_id=2,test='case',request_id=4,frame=1,frames_observed=2,
               wall_ns=1000,wall_under_100ms=True,measurement_overhead_ns=5,mach_absolute_window_ns=[2000,3005])
        with self.assertRaisesRegex(ValueError,'sequence'):m.validate_frames([f],1)
        first=dict(f,frame=0)
        restart=dict(first,mach_absolute_window_ns=[4000,5005])
        with self.assertRaisesRegex(ValueError,'before previous dump completed'):m.validate_frames([first,restart],1)

    def test_empty_frames_and_negative_or_inconsistent_brackets_are_rejected(self):
        with self.assertRaises(ValueError):m.validate_frames([],1)
        with self.assertRaises(ValueError):m.clock_bridge([],[])
        f=dict(schema=3,pid=1,native_thread_id=2,test='case',request_id=4,frame=0,frames_observed=1,
               wall_ns=1000,wall_under_100ms=True,measurement_overhead_ns=5,mach_absolute_window_ns=[2000,3005])
        for change in [dict(measurement_overhead_ns=-1),dict(measurement_overhead_ns=True),
                       dict(mach_absolute_window_ns=[2000,2990]),dict(mach_absolute_window_ns=[2000,4000])]:
            with self.assertRaises(ValueError):m.validate_frames([dict(f,**change)],1)

    def test_distinct_complete_dumps_share_identity_but_not_clock_windows(self):
        f=dict(schema=3,pid=1,native_thread_id=2,test='case',request_id=4,frame=0,frames_observed=1,
               wall_ns=1000,wall_under_100ms=True,measurement_overhead_ns=5,mach_absolute_window_ns=[2000,3005])
        second=dict(f,mach_absolute_window_ns=[4000,5005])
        m.validate_frames([f,second],1,expected={'case':2})
        with self.assertRaises(ValueError):m.validate_frames([f,f],1,expected={'case':2})
        with self.assertRaises(ValueError):m.validate_frames([f],1,expected={'case':2})
        with self.assertRaises(ValueError):m.validate_frames([f],1,expected={'case':1,'absent':1})

    def test_dedicated_records_reject_empty_noise_and_partial_json(self):
        for text in ['', 'noise\n', 'TC207_FRAME {} trailing\n', 'TC207_FRAME {}\nextra\n', 'TC207_FRAME []\n']:
            with self.assertRaises(ValueError):m.parse_frames(text,dedicated=True)
        self.assertEqual(m.parse_frames('TC207_FRAME {"frame":0}\n',dedicated=True),[{'frame':0}])

    def test_dedicated_log_never_falls_back_or_reads_another_pid(self):
        with tempfile.TemporaryDirectory() as td:
            root=Path(td);(root/'frame-logs').mkdir();(root/'test.stderr').write_text('wrong fallback')
            (root/'frame-logs/frames-999.jsonl').write_text('foreign record')
            receipt=dict(pid=11,dedicated_frames=True)
            with self.assertRaises(ValueError):m.frame_text(root,receipt)
            (root/'frame-logs/frames-11.jsonl').write_text('owned record')
            self.assertEqual(m.frame_text(root,receipt),'owned record')


if __name__=='__main__':unittest.main()
