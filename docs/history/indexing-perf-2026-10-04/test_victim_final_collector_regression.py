"""Synthetic schema checks only; these are never performance evidence."""
import copy
import hashlib
import json
from pathlib import Path
import subprocess
import tempfile

HERE = Path(__file__).resolve().parent
COLLECTOR = HERE / 'summarize_extensions.py'
POLICY = 'full independent oracle after tentative t3; no extra frame/input before output'
SUMMARY = 'test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out; finished in 1.0s'
meta = dict(runner='extended', pairs=1, expected_rows=2,
            supported_cells=[dict(case='T1-A-B-C-A', source='FileList')])

def request(i, tab):
    return dict(request_id=i, tab_id=tab, terminal_role='required-latest-success',
                terminal_kind='finished', terminal_source='FileList',
                latest_generation_required=True, latest_measured_request=True)

def row(control):
    return dict(comparison='T1-A-B-C-A', source='FileList', pair=0,
                case='B0' if control else 'T1-A-B-C-A', position=0 if control else 1,
                order='AB', correct=True, contention_eligible=True,
                sample_entries=100000, comparison_kind='feature-cost',
                measurement_kind='headless-GUI-actual-workers', data_publish_end_ms=1,
                terminal_publish_ms=2, index_ready_ms=20, results_ready_ms=21,
                index_sender_load_at_t2=dict(queued=0, inflight=0),
                allocated_request_ids=[8], planned_request_ids=[8], released_request_ids=[8],
                index_requests=[request(8,1)], actual_preemption_events=[],
                declared_eviction_edges=[], declared_retained_victims=[],
                preemption_observer_limit=128, preemption_observer_overflow=False,
                retained_seed_validation_policy=POLICY)

rows = [row(True), row(False)]
r=rows[1]
q=request(6,2)
q.update(terminal_role='declared-Warm-eviction-retained-last-good',
         terminal_kind='canceled', latest_generation_required=False,
         planned_revoked_by=None, completed_predecessor=False, terminal_source=None, expected_root='/owned/B',
         actual_admitted_root='/owned/B', actual_started_root='/owned/B',
         permission_reason='switch-evicts-previous-Warm', permission_ms=3,
         terminal_offered_ms=6, terminal_publish_ms=6, request_processing_returned_ms=7,
         actual_terminal_offer_kind='canceled', terminal_offer_current=False,
         admitted_ms=1, started_ms=2, skipped_closed_before_start=False,
         bookkeeping_released_ms=8, entries_emitted=58368, started_source='FileList',
         admitted_root_matches=True, emitted_workload_semantics='actual partial canceled work')
r.update(allocated_request_ids=[6,7,8], planned_request_ids=[6,7,8],
         released_request_ids=[6,7,8], index_requests=[q,request(7,3),request(8,1)])
r['actual_preemption_events']=[dict(victim_request_id=6,victim_tab=2,
    prior_latest_request_id=6,replacement_request_id=0,actual_active_tab=1,
    actual_warm_tab=3,pending_active_request_id=8,latest_active_request_id=8,
    queued_active_request_ids=[8],actual_inflight_count=2,mutation_ms=5)]
r['declared_eviction_edges']=[dict(victim_request_id=6,victim_tab=2,old_warm_tab=2,
    trace_tabs=[1,2,3],stage=2,expected_active_tab=1,expected_current_warm_tab=3,
    incoming_request_ids=[8],permission_reason='switch-evicts-previous-Warm',
    declared_ms=2,permission_ms=3,closed_ms=10)]
r['declared_retained_victims']=[dict(request_id=6,tab_id=2,
    full_seed_oracle_after_t3=True,seed_request_id=3,actual_seed_request_id=3,
    seed_root='/owned/B',actual_seed_root='/owned/B',seed_source='FileList',
    actual_seed_source='FileList',seed_all_count=100000,actual_all_count=100000,
    seed_visible_count=100000,actual_visible_count=100000,
    seed_all_signature='full-independent-seed',actual_all_signature='full-independent-seed',
    seed_visible_signature='full-independent-seed',actual_visible_signature='full-independent-seed',
    partial_emitted_entries=58368,workload_semantics='not 100k indexing throughput')]

results=[]
with tempfile.TemporaryDirectory(prefix='victim-collector-',dir=HERE) as tmp:
    tmp=Path(tmp)
    def check(name, rr=None, mm=None, summary=SUMMARY, expected=False, actual_log=None):
        log=tmp/(name+'.log');out=tmp/(name+'.json')
        if actual_log:
            log=HERE/actual_log
        else:
            log.write_text('INDEX_PERF_META '+json.dumps(meta if mm is None else mm)+'\n'+
                ''.join('INDEX_PERF_SAMPLE '+json.dumps(x)+'\n' for x in (rows if rr is None else rr))+
                summary+'\n')
        p=subprocess.run(['python3',str(COLLECTOR),str(log),'--rows','2','--output',str(out)],
                         text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
        assert (p.returncode==0)==expected,(name,p.stdout)
        assert out.exists()==expected,name
        results.append(dict(name=name,exit_code=p.returncode,accepted=expected))
    check('synthetic-valid-declared-victim',expected=True)
    check('duplicate-summary',summary=SUMMARY+'\n'+SUMMARY)
    check('zero-test',summary=SUMMARY.replace('1 passed','0 passed'))
    check('failed-run',summary=SUMMARY.replace('ok. 1 passed; 0 failed','FAILED. 0 passed; 1 failed'))
    mm=copy.deepcopy(meta);mm['supported_cells'][0]['source']='Walker';check('wrong-meta-cell',mm=mm)
    mm=copy.deepcopy(meta);mm['supported_cells'].append(dict(case='T1-active-warm',source='Walker'));check('missing-meta-cell',mm=mm)
    for name, mutate in [
        ('wrong-cause',lambda x:x[1]['actual_preemption_events'][0].update(pending_active_request_id=9)),
        ('wrong-warm',lambda x:x[1]['actual_preemption_events'][0].update(actual_warm_tab=2)),
        ('wrong-seed',lambda x:x[1]['declared_retained_victims'][0].update(actual_seed_request_id=4)),
        ('corrupt-full-seed',lambda x:x[1]['declared_retained_victims'][0].update(actual_all_signature='corrupt')),
        ('no-full-oracle',lambda x:x[1]['declared_retained_victims'][0].update(full_seed_oracle_after_t3=False)),
        ('missing-body-return',lambda x:x[1]['index_requests'][0].update(request_processing_returned_ms=None)),
        ('required-A-canceled',lambda x:x[1]['index_requests'][2].update(terminal_kind='canceled')),
        ('finished-victim-seed-fallback',lambda x:x[1]['index_requests'][0].update(terminal_kind='finished')),
        ('overflow',lambda x:x[1].update(preemption_observer_overflow=True)),
        ('partial-rows',lambda x:x.pop()),
    ]:
        rr=copy.deepcopy(rows);mutate(rr);check(name,rr=rr)
    for name in ['tabchain-stall-diagnostic.log','final-current-extended-pilot.log']:
        check('retired-'+name,actual_log=name)

packet=dict(purpose='synthetic collector guards, not performance evidence',
            collector_sha256=hashlib.sha256(COLLECTOR.read_bytes()).hexdigest(),
            results=results,passed=len(results))
(HERE/'victim-final-collector-regression.json').write_text(json.dumps(packet,indent=2)+'\n')
print('PASS',len(results),'collector checks; no partial output accepted')
