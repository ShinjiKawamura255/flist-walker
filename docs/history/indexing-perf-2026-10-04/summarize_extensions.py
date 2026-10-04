"""Evidence-only collector: accept a complete successful Rust runner log."""
import argparse, collections, hashlib, json, re, statistics
from pathlib import Path

p=argparse.ArgumentParser();p.add_argument('log',type=Path);p.add_argument('--rows',type=int,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
t=a.log.read_text(); metas=[json.loads(s.split('INDEX_PERF_META ',1)[1]) for s in t.splitlines() if 'INDEX_PERF_META ' in s];rows=[json.loads(s.partition(' ')[2]) for s in t.splitlines() if s.startswith('INDEX_PERF_SAMPLE ')]
summaries=re.findall(r'^test result:.*$',t,re.M)
assert len(summaries)==1 and re.match(r'^test result: ok\. 1 passed; 0 failed; 0 ignored;',summaries[0]),'runner did not pass exactly one test'
assert len(metas)==1 and metas[0]['expected_rows']==a.rows and len(rows)==a.rows, 'metadata/row count mismatch'
assert all(r['correct'] is True and r['contention_eligible'] is True for r in rows), 'incorrect/noneligible sample'
def validate_victim_contract(r):
 assert r['preemption_observer_overflow'] is False and r['preemption_observer_limit']==128
 events=r['actual_preemption_events']; edges=r['declared_eviction_edges']; victims=r['declared_retained_victims']
 reqs={q['request_id']:q for q in r['index_requests']}
 assert len(reqs)==len(r['index_requests']) and set(reqs)==set(r['allocated_request_ids'])
 assert len(events)<=128 and len({e['victim_request_id'] for e in events})==len(events)
 assert len({e['victim_request_id'] for e in edges})==len(edges)
 assert len({v['request_id'] for v in victims})==len(victims)
 retained={v['request_id']:v for v in victims}; declared={e['victim_request_id']:e for e in edges}
 if events or edges or victims:
  assert r['comparison']=='T1-A-B-C-A' and r['case']!='B0'
 assert r['retained_seed_validation_policy']=='full independent oracle after tentative t3; no extra frame/input before output'
 for e in events:
  edge=declared[e['victim_request_id']];q=reqs[e['victim_request_id']]
  assert e['victim_tab']==q['tab_id']==edge['victim_tab']==edge['old_warm_tab']
  assert e['prior_latest_request_id']==e['victim_request_id'] and e['replacement_request_id']==0
  tabs=edge['trace_tabs'];assert len(set(tabs))==3 and edge['stage'] in (1,2)
  victim,active,warm=(tabs[0],tabs[2],tabs[1]) if edge['stage']==1 else (tabs[1],tabs[0],tabs[2])
  assert (e['victim_tab'],e['actual_active_tab'],e['actual_warm_tab'])==(victim,active,warm)
  assert (edge['expected_active_tab'],edge['expected_current_warm_tab'])==(active,warm)
  incoming=e['pending_active_request_id'];assert incoming==e['latest_active_request_id']
  assert incoming in edge['incoming_request_ids'] and reqs[incoming]['tab_id']==active
  assert incoming in e['queued_active_request_ids'] and len(e['queued_active_request_ids'])<=128
  assert len(set(e['queued_active_request_ids']))==len(e['queued_active_request_ids'])
  assert set(e['queued_active_request_ids'])<=set(edge['incoming_request_ids'])
  assert e['actual_inflight_count']>=2
  assert q['permission_reason']==edge['permission_reason']=='switch-evicts-previous-Warm'
  assert edge['declared_ms']<=q['permission_ms']==edge['permission_ms']<=e['mutation_ms']<=q['terminal_offered_ms']<=q['terminal_publish_ms']<=q['request_processing_returned_ms']
  if edge['closed_ms'] is not None:assert e['mutation_ms']<=edge['closed_ms']
  assert q['terminal_kind']==q['actual_terminal_offer_kind']=='canceled' and q['terminal_offer_current'] is False
  assert q['admitted_ms']<=q['started_ms']<=e['mutation_ms'] and q['skipped_closed_before_start'] is False
  assert q['bookkeeping_released_ms']<=r['index_ready_ms'] and q['request_processing_returned_ms']<=r['index_ready_ms']
  assert q['entries_emitted']>0 and q['started_source']==r['source'] and q['admitted_root_matches'] is True
  assert q['actual_admitted_root']==q['actual_started_root']==q['expected_root']
  assert 'partial canceled' in q['emitted_workload_semantics']
 for q in reqs.values():
  role=q['terminal_role'];assert role in ('required-latest-success','revoked-predecessor','completed-predecessor','declared-Warm-eviction-retained-last-good')
  assert q['latest_generation_required']==(q['latest_measured_request'] and role!='declared-Warm-eviction-retained-last-good')
  if q['latest_generation_required']:
   assert role=='required-latest-success' and q['terminal_kind']=='finished' and q['terminal_source']==r['source']
  if role=='declared-Warm-eviction-retained-last-good':
   v=retained[q['request_id']];assert v['tab_id']==q['tab_id'] and q['latest_measured_request'] is True
   assert q['latest_generation_required'] is False and q['planned_revoked_by'] is None
   assert len([e for e in events if e['victim_request_id']==q['request_id']])==1
   assert v['full_seed_oracle_after_t3'] is True and v['seed_request_id']!=q['request_id']
   assert v['seed_request_id']==v['actual_seed_request_id']
   assert v['seed_root']==v['actual_seed_root']==q['expected_root']
   assert v['seed_source']==v['actual_seed_source']==r['source']
   assert v['seed_all_count']==v['actual_all_count']==r['sample_entries']
   assert v['seed_visible_count']==v['actual_visible_count']==r['sample_entries']
   assert v['seed_all_signature']==v['actual_all_signature'] and v['seed_visible_signature']==v['actual_visible_signature']
   assert v['partial_emitted_entries']==q['entries_emitted']>0 and 'not 100k indexing throughput' in v['workload_semantics']
  else:assert q['request_id'] not in retained
 assert len(retained)==sum(q['terminal_role']=='declared-Warm-eviction-retained-last-good' for q in reqs.values())

groups=collections.defaultdict(list);keys=set()
for r in rows:
 key=(r['comparison'],r['source'],r['pair'],r['case']=='B0');assert key not in keys,('duplicate',key);keys.add(key)
 assert r['position']==(0 if (r['pair']%2==0)==(r['case']=='B0') else 1),'incorrect AB/BA position'
 assert r['order']==('AB' if r['pair']%2==0 else 'BA')
 if r['measurement_kind']=='headless-GUI-actual-workers':
  assert r['data_publish_end_ms'] <= r['terminal_publish_ms'] <= r['index_ready_ms'] <= r['results_ready_ms']
  if r['comparison']=='W1-truncated':
   assert r['actual_limit']==r['expected_final_logical_entries']==500000 and r['sample_entries']==500001
  else:
   assert r['index_sender_load_at_t2']['queued']==r['index_sender_load_at_t2']['inflight']==0
   assert set(r['allocated_request_ids'])==set(r['planned_request_ids'])==set(r['released_request_ids'])
   validate_victim_contract(r)
 groups[(r['comparison'],r['source'])].append(r)
expected_cells={(c['case'],c['source']) for c in metas[0]['supported_cells']} if metas[0]['runner']=='extended' else {('W1-truncated','Walker')}
assert set(groups)==expected_cells, 'source/case cells differ from META'
phases=['data_publish_end_ms','terminal_publish_ms','index_ready_ms','results_ready_ms','worker_drained_ms','full_wait_ms','max_frame_ms','max_ingest_gap_ms','max_no_work_progress_ms']
out=[]
for (case,source),rs in sorted(groups.items()):
 pairs=metas[0]['pairs'];assert len(rs)==pairs*2
 perpair={i:{r['case']=='B0':r for r in rs if r['pair']==i} for i in range(pairs)}
 assert all(set(x)=={False,True} for x in perpair.values())
 x={'case':case,'source':source,'comparison_kind':rs[0]['comparison_kind'],'measurement_kind':rs[0]['measurement_kind'],'pairs':pairs,'phases':{}}
 for phase in phases:
  valid=all(isinstance(r.get(phase),(int,float)) for r in rs)
  if not valid:continue
  aa=[perpair[i][True][phase] for i in range(pairs)];bb=[perpair[i][False][phase] for i in range(pairs)]
  ratios=[b/z if z else None for z,b in zip(aa,bb)]
  good=[r for r in ratios if r is not None]
  x['phases'][phase]={'control_median':statistics.median(aa),'control_max':max(aa),'condition_median':statistics.median(bb),'condition_max':max(bb),'paired_ratios':ratios,'paired_ratio_median':statistics.median(good) if good else None,'paired_ratio_max':max(good) if good else None}
 out.append(x)
a.output.write_text(json.dumps({'log':a.log.name,'log_sha256':hashlib.sha256(a.log.read_bytes()).hexdigest(),'metadata':metas[0],'raw_rows':len(rows),'comparisons':out},indent=2)+'\n')
print('PASS',len(groups),'cells',len(rows),'raw rows')
