import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('promotion_trace',Path(__file__).with_name('promotion.py'))
m=importlib.util.module_from_spec(spec)


class PromotionProofTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):spec.loader.exec_module(m)

    def data(self):
        target='tc_207_background_finalization_survives_promotion_to_active'
        states=[dict(pid=1,tid=2,start=190_000_000,end=250_000_000,state='Preempted'),
                dict(pid=1,tid=2,start=250_000_000,end=260_000_000,state='Preempted'),
                dict(pid=1,tid=2,start=260_000_000,end=299_000_000,state='Preempted'),
                dict(pid=1,tid=2,start=310_000_000,end=400_000_000,state='Terminated')]
        stacks=[dict(pid=1,tid=2,time=r['start'],stack=[target,'poll_index_response_with_budget_for_test']) for r in states[:-1]]
        return dict(pid=1,test=target,matched_tids=[2],strict_promotion_failure_confirmed=True,states=states,transition_stacks=stacks)

    def test_interval_ending_after_latest_start_is_same_failed_call(self):
        result=m.prove(self.data())
        self.assertEqual(result['observed_preempted_ns'],109_000_000)
        self.assertEqual(result['state_edge_1ms_sensitivity_ns'],103_000_000)
        self.assertEqual(result['status'],'RECORDED_PREEMPTION_SUFFICIENT')
        self.assertIsNone(result['wall_ns'])
        self.assertIsNone(result['cpu_total_ns'])

    def test_earlier_poll_or_wrong_stack_is_not_attributed(self):
        data=self.data();data['states'].insert(0,dict(pid=1,tid=2,start=100_000_000,end=180_000_000,state='Preempted'))
        data['transition_stacks'].append(dict(pid=1,tid=2,time=100_000_000,stack=[data['test'],'poll_index_response_with_budget_for_test']))
        data['transition_stacks'][1]['stack']=['different_function']
        result=m.prove(data)
        self.assertEqual(result['observed_preempted_ns'],99_000_000)
        self.assertEqual(result['status'],'RECORDED_PREEMPTION_CONTRIBUTION')

    def test_missing_end_or_ambiguous_identity_is_unknown(self):
        data=self.data();data['matched_tids']=[2,3]
        self.assertEqual(m.prove(data)['status'],'UNKNOWN')
        data=self.data();data['states']=data['states'][:-1]
        self.assertEqual(m.prove(data)['status'],'UNKNOWN')
        data=self.data();data['strict_promotion_failure_confirmed']=False
        self.assertEqual(m.prove(data)['status'],'UNKNOWN')

    def test_no_matching_transition_witness_is_unknown(self):
        data=self.data();data['transition_stacks']=[]
        self.assertEqual(m.prove(data)['status'],'UNKNOWN')
        data=self.data()
        for row in data['transition_stacks']:row['stack']=['unrelated']
        self.assertEqual(m.prove(data)['status'],'UNKNOWN')

    def test_overlap_and_foreign_pid_rejected(self):
        data=self.data();data['states'][1]['start']-=1
        with self.assertRaises(ValueError):m.prove(data)
        data=self.data();data['transition_stacks'][0]['pid']=7
        with self.assertRaises(ValueError):m.prove(data)


if __name__=='__main__':unittest.main()
