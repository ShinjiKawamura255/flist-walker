"""Synthetic schema controls only. No row here is performance evidence."""
import sys
from pathlib import Path
prior=Path(__file__).with_name('test_victim_final_collector_regression.py')
exec(compile(prior.read_text().split('results=[]',1)[0],str(prior),'exec'))
if len(sys.argv)>1:COLLECTOR=HERE/sys.argv[1]
for rr in rows:
 rr.update(actual_warm_removal_events=[],warm_removal_observer_limit=128,warm_removal_observer_overflow=False)
 for qq in rr['index_requests']:
  qq.update(actual_cause_kind=None,invalidating_mutation_ms=None,followup_preempt_ms=None)
  qq.setdefault('expected_root',{1:'/owned/A',2:'/owned/B',3:'/owned/C'}[qq['tab_id']])
 for ee in rr['actual_preemption_events']:ee['warm_removal_mutation_ms']=None
q.update(actual_cause_kind='direct-preempt',invalidating_mutation_ms=5)
base=copy.deepcopy(rows)
def warm(marker=None):
 rr=copy.deepcopy(base);row=rr[1];qq=row['index_requests'][0];edge=row['declared_eviction_edges'][0]
 row['actual_warm_removal_events']=[dict(mutation_ms=4,removed_request_id=6,previous_warm_tab=2,replacement_warm_tab=3,route_tab=2)]
 edge.update(expected_active_root='/owned/A',actual_switch_ack=dict(at_ms=4.5,tab_id=1,root='/owned/A',pending_activation_tab_id=None))
 qq.update(actual_cause_kind='warm-replacement-removal',invalidating_mutation_ms=4,followup_preempt_ms=marker)
 if marker is None:row['actual_preemption_events']=[]
 else:row['actual_preemption_events'][0].update(prior_latest_request_id=None,mutation_ms=marker,warm_removal_mutation_ms=4)
 return rr
results=[]
with tempfile.TemporaryDirectory(prefix='warm-removal-collector-',dir=HERE) as tmp:
 tmp=Path(tmp)
 def check(name,rr=None,mm=None,summary=SUMMARY,expected=False,actual_log=None):
  log=tmp/(name+'.log');out=tmp/(name+'.json')
  if actual_log:log=HERE/actual_log
  else:log.write_text('INDEX_PERF_META '+json.dumps(meta if mm is None else mm)+'\n'+''.join('INDEX_PERF_SAMPLE '+json.dumps(x)+'\n' for x in (base if rr is None else rr))+summary+'\n')
  p=subprocess.run(['python3',str(COLLECTOR),str(log),'--rows','2','--output',str(out)],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert (p.returncode==0)==expected,(name,p.stdout)
  assert out.exists()==expected,name
  results.append(dict(name=name,exit_code=p.returncode,accepted=expected))
 check('valid-direct-some-victim',expected=True)
 check('valid-standalone-warm-removal',warm(),expected=True)
 check('valid-marker-before-offer',warm(5),expected=True)
 check('valid-marker-after-offer-and-body',warm(9),expected=True)
 for before_declaration in [True,False]:
  rr=warm(9);row=rr[1];a=request(5,1)
  a.update(terminal_role='completed-predecessor',latest_generation_required=False,latest_measured_request=False,completed_predecessor=True,completed_predecessor_observed_ms=1.5,permission_ms=None,permission_reason=None,terminal_kind='finished',actual_terminal_offer_kind='finished',terminal_publish_ms=.5 if before_declaration else 1.25,terminal_offered_ms=.5 if before_declaration else 1.25,request_processing_returned_ms=3,bookkeeping_released_ms=3,admitted_root_matches=True,actual_admitted_root='/owned/A',actual_started_root='/owned/A',expected_root='/owned/A',started_source='FileList',actual_cause_kind='warm-replacement-removal',invalidating_mutation_ms=2,followup_preempt_ms=None)
  row['index_requests'].append(a)
  for k in ['allocated_request_ids','planned_request_ids','released_request_ids']:row[k].append(5)
  row['actual_warm_removal_events'].append(dict(mutation_ms=2,removed_request_id=5,previous_warm_tab=1,replacement_warm_tab=2,route_tab=1))
  row['declared_eviction_edges'].append(dict(victim_request_id=5,victim_tab=1,old_warm_tab=1,trace_tabs=[1,2,3],stage=1,expected_active_tab=3,expected_current_warm_tab=2,incoming_request_ids=[7],declared_ms=1,closed_ms=3,permission_ms=None,permission_reason=None,expected_active_root='/owned/C',actual_switch_ack=dict(at_ms=2.5,tab_id=3,root='/owned/C',pending_activation_tab_id=None)))
  check('valid-observed-finished-'+str(before_declaration),rr,expected=True)
  rr[1]['index_requests'][-1]['completed_predecessor_observed_ms']=None
  check('missing-observed-finished-'+str(before_declaration),rr)
 mutants=[
  ('lone-none-marker',lambda r:r[1].update(actual_warm_removal_events=[])),
  ('duplicate-removal',lambda r:r[1]['actual_warm_removal_events'].append(copy.deepcopy(r[1]['actual_warm_removal_events'][0]))),
  ('overflow',lambda r:r[1].update(warm_removal_observer_overflow=True)),
  ('wrong-removed-id',lambda r:r[1]['actual_warm_removal_events'][0].update(removed_request_id=5)),
  ('wrong-route',lambda r:r[1]['actual_warm_removal_events'][0].update(route_tab=1)),
  ('wrong-oldwarm',lambda r:r[1]['actual_warm_removal_events'][0].update(previous_warm_tab=1)),
  ('wrong-newwarm',lambda r:r[1]['actual_warm_removal_events'][0].update(replacement_warm_tab=2)),
  ('missing-ack',lambda r:r[1]['declared_eviction_edges'][0].update(actual_switch_ack=None)),
  ('wrong-ack-tab',lambda r:r[1]['declared_eviction_edges'][0]['actual_switch_ack'].update(tab_id=2)),
  ('wrong-ack-root',lambda r:r[1]['declared_eviction_edges'][0]['actual_switch_ack'].update(root='/owned/foreign')),
  ('pending-ack',lambda r:r[1]['declared_eviction_edges'][0]['actual_switch_ack'].update(pending_activation_tab_id=1)),
  ('future-mutation',lambda r:r[1]['actual_warm_removal_events'][0].update(mutation_ms=5)),
  ('ack-after-window',lambda r:r[1]['declared_eviction_edges'][0]['actual_switch_ack'].update(at_ms=11)),
  ('permission-after-removal',lambda r:r[1]['index_requests'][0].update(permission_ms=7)),
  ('wrong-stage',lambda r:r[1]['declared_eviction_edges'][0].update(stage=1)),
  ('incoming-wrong-root',lambda r:r[1]['index_requests'][2].update(expected_root='/owned/foreign')),
  ('incoming-wrong-id',lambda r:r[1]['actual_preemption_events'][0].update(pending_active_request_id=9)),
  ('marker-before-removal',lambda r:r[1]['actual_preemption_events'][0].update(mutation_ms=3)),
  ('marker-wrong-link',lambda r:r[1]['actual_preemption_events'][0].update(warm_removal_mutation_ms=2)),
  ('offer-before-removal',lambda r:r[1]['index_requests'][0].update(terminal_offered_ms=3)),
  ('cause-flag-alone',lambda r:r[1]['index_requests'][0].update(actual_cause_kind='direct-preempt')),
  ('bad-marker-request-link',lambda r:r[1]['index_requests'][0].update(followup_preempt_ms=8)),
  ('missing-body',lambda r:r[1]['index_requests'][0].update(request_processing_returned_ms=None)),
  ('required-A-canceled',lambda r:r[1]['index_requests'][2].update(terminal_kind='canceled')),
  ('canceled-A-mislabeled-predecessor',lambda r:r[1]['index_requests'][2].update(terminal_kind='canceled',terminal_role='revoked-predecessor',latest_measured_request=False,latest_generation_required=False)),
  ('finished-B-oldseed-fallback',lambda r:r[1]['index_requests'][0].update(terminal_kind='finished')),
  ('corrupt-full-seed',lambda r:r[1]['declared_retained_victims'][0].update(actual_all_signature='foreign')),
 ]
 for name,mutate in mutants:
  rr=warm(9);mutate(rr);check(name,rr)
 check('double-summary',warm(),summary=SUMMARY+'\n'+SUMMARY)
 check('zero-tests',warm(),summary=SUMMARY.replace('1 passed','0 passed'))
 check('partial-one-row',warm()[:1])
 mm=copy.deepcopy(meta);mm['supported_cells'][0]['source']='Walker';check('wrong-meta-cell',warm(),mm=mm)
 for name in ['selector-accepted-tabchain-target.log','selector-stall-diagnostic-tabchain-target.log']:
  check('actual-retired-'+name,actual_log=name)
 packet=dict(purpose='synthetic raw schema controls only; not performance evidence',collector_sha256=hashlib.sha256(COLLECTOR.read_bytes()).hexdigest(),results=results,passed=len(results))
 (HERE/'warm-removal-collector-guards.json').write_text(json.dumps(packet,indent=2)+'\n')
print('PASS',len(results),'collector checks')
