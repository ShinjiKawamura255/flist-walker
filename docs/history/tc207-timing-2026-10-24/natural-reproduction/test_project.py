import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec=importlib.util.spec_from_file_location('project_trace',Path(__file__).with_name('project.py'))
m=importlib.util.module_from_spec(spec)


def frame(wall=1000):
    return dict(schema=3,pid=1,native_thread_id=2,test='case',request_id=4,frame=0,frames_observed=1,
                wall_ns=wall,wall_under_100ms=wall<100_000_000,measurement_overhead_ns=5,
                mach_absolute_window_ns=[2000,2000+wall+5],mach_absolute_window_ticks=[2000,2000+wall+5],mach_timebase=[1,1])


class ProjectionTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):spec.loader.exec_module(m)

    def test_full_argv_accepts_only_declared_parallel_conditions(self):
        m.validate_argv(['--nocapture'])
        m.validate_argv(['--nocapture','--test-threads=48'])
        for args in [['--nocapture','--exact','case'],['--nocapture','--test-threads=1'],[]]:
            with self.assertRaises(ValueError):m.validate_argv(args)

    def test_passing_case_requires_all_dumps(self):
        with self.assertRaises(ValueError):m.admit_cases([frame()],1,{'case':'ok'},{'case':2})
        result=m.admit_cases([frame()],1,{'case':'ok'},{'case':1})
        self.assertEqual(result['missing_failed_dumps'],{})

    def test_failed_case_can_end_at_first_wall_failure_without_waiving_it(self):
        row=frame(101_000_000)
        result=m.admit_cases([row],1,{'case':'FAILED'},{'case':2})
        self.assertEqual(result['wall_failures'],[row])
        self.assertEqual(result['missing_failed_dumps'],{'case':1})
        with self.assertRaises(ValueError):m.admit_cases([row],1,{'case':'ok'},{'case':1})
        second=dict(row,mach_absolute_window_ns=[200_000_000,301_000_005],mach_absolute_window_ticks=[200_000_000,301_000_005])
        with self.assertRaises(ValueError):m.admit_cases([row,second],1,{'case':'FAILED'},{'case':2})

    def test_missing_runner_cases_or_empty_frames_are_not_success(self):
        with self.assertRaises(ValueError):m.admit_cases([],1,{'case':'ok'},{'case':1})
        with self.assertRaises(ValueError):m.admit_cases([frame()],1,{}, {'case':1})

    def test_missing_failed_case_dump_is_explicit_but_missing_passed_case_fails(self):
        result=m.admit_cases([frame()],1,{'case':'ok','other':'FAILED'},{'case':1,'other':1})
        self.assertEqual(result['missing_failed_dumps'],{'other':1})
        with self.assertRaises(ValueError):m.admit_cases([frame()],1,{'case':'ok','other':'ok'},{'case':1,'other':1})

    def test_runner_outcomes_are_exact_unique_and_summary_keeps_failure(self):
        text='test case ... ok\ntest other ... FAILED\ntest result: FAILED. 1 passed; 1 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1s\n'
        outcomes,summary=m.runner_outcomes(text,{'case':1,'other':1})
        self.assertEqual(outcomes,{'case':'ok','other':'FAILED'})
        self.assertEqual(summary['failed'],1)
        with self.assertRaises(ValueError):m.runner_outcomes(text+'test case ... ok\n',{'case':1,'other':1})

    def test_duplicate_failed_names_cannot_hide_another_case(self):
        text='test case ... ok\ntest other ... FAILED\ntest other ... FAILED\ntest result: FAILED. 1 passed; 2 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1s\n'
        with self.assertRaises(ValueError):m.runner_outcomes(text,{'case':1})

    def test_assertion_requires_exact_case_panic_context(self):
        row=frame(101_000_000)
        message='background finalization frame exceeded 100ms: request_id=4, frame=0, wall=101ms'
        wrong="thread 'other' (12) panicked at x.rs:1:1:\n"+message+'\n'
        self.assertEqual(m.assertion_receipt(wrong,row)['status'],'UNKNOWN')
        right="thread 'case' (12) panicked at x.rs:1:1:\n"+message+'\n'
        self.assertEqual(m.assertion_receipt(right,row)['status'],'CONFIRMED')
        self.assertEqual(m.assertion_receipt(right+right,row)['status'],'UNKNOWN')
        self.assertEqual(m.assertion_receipt(right.replace('frame=0','frame=1'),row)['status'],'UNKNOWN')

    def test_unmeasured_promotion_wall_failure_is_kept_separate(self):
        name='app::tests::tab_background_responses::tc_207_background_finalization_survives_promotion_to_active'
        message='promotion finalization frame exceeded budget'
        text="thread '"+name+"' (42) panicked at x.rs:1:1:\n"+message+'\n'
        rows=m.unmeasured_tc207_failures([name],text,[])
        self.assertEqual(rows[0]['status'],'STRICT_WALL_FAILURE_NO_FRAME_WINDOW')
        self.assertEqual(rows[0]['wall_lower_bound_ns'],100_000_000)
        self.assertIsNone(rows[0]['wall_ns'])
        self.assertEqual(rows[0]['trace_correlation'],'UNKNOWN')
        self.assertEqual(m.unmeasured_tc207_failures([],text,[]),[])
        self.assertEqual(m.unmeasured_tc207_failures([name],text.replace(message,'other assertion'),[])[0]['status'],'FAILED_REASON_UNKNOWN')

    def test_unreferenced_scalar_definitions_do_not_exhaust_retained_index(self):
        xml='<trace-query-result><node xpath="/trace-toc/run[1]/data/table[1]"><row><x ref="1"/></row>'
        xml+=''.join('<row><x id="'+str(i)+'">'+str(i)+'</x></row>' for i in range(1,10))
        xml+='</node></trace-query-result>'
        with tempfile.TemporaryDirectory() as td:
            path=Path(td)/'e.xml';path.write_text(xml)
            self.assertEqual(set(m.index_ids(path,max_ids=1)),{'1'})
            path.write_text(xml.replace('id="9"','id="1"'))
            with self.assertRaises(ValueError):m.index_ids(path,max_ids=1)

    def test_two_pass_ids_resolve_forward_refs_and_keep_definitions(self):
        xml='<trace-query-result><node xpath="/trace-toc/run[1]/data/table[1]"><row><x ref="1"/></row><row><x id="1">42</x></row></node></trace-query-result>'
        with tempfile.TemporaryDirectory() as td:
            p=Path(td)/'events.xml';p.write_text(xml)
            ids=m.index_ids(p,max_ids=2)
            rows=list(m.iter_rows(p))
            self.assertEqual(m.legacy.resolve(rows[0][1][0],ids).text,'42')
            self.assertEqual(rows[0][0],1)
            with self.assertRaises(ValueError):m.index_ids(p,max_ids=0)
            p.write_text(xml.replace('<x id="1">42</x>','<x id="1">42</x><x id="1">43</x>'))
            with self.assertRaises(ValueError):m.index_ids(p)


if __name__=='__main__':unittest.main()
