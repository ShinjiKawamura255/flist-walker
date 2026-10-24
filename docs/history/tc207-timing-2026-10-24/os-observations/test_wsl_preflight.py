import importlib.util
from pathlib import Path
import unittest

spec=importlib.util.spec_from_file_location('preflight',Path(__file__).with_name('wsl-preflight.py'))
p=importlib.util.module_from_spec(spec)
spec.loader.exec_module(p)


class InventoryTests(unittest.TestCase):
    def test_permission_failure_is_unknown_not_missing(self):
        self.assertEqual(p.classify_events([dict(status='UNAVAILABLE',errno=13)])['status'],'UNKNOWN')

    def test_exact_three_scheduler_events_required(self):
        result=p.classify_events([dict(status='READ',content='sched:sched_switch\nsched:sched_wakeup\n')])
        self.assertEqual(result['status'],'MISSING')
        self.assertEqual(result['missing'],['sched:sched_wakeup_new'])

    def test_readable_inventory_does_not_claim_capture_permission(self):
        result=p.classify_events([dict(status='READ',content='sched:sched_switch\nsched:sched_wakeup\nsched:sched_wakeup_new\n')])
        self.assertEqual(result['status'],'AVAILABLE')
        self.assertNotIn('capture_permission',result)


if __name__=='__main__':
    unittest.main()
