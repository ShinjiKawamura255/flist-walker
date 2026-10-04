"""Synthetic schema controls; no rows are actual performance evidence."""
from pathlib import Path
prior=Path(__file__).with_name('test_warm_removal_collector.py')
exec(compile(prior.read_text().split('\nresults=[]\n',1)[0],str(prior),'exec'))
ROLE='declared-Warm-eviction-stale-Full-retained-last-good'
def sample():
 rr=warm(9);row=rr[1];q=row['index_requests'][0];v=row['declared_retained_victims'][0]
 q.update(terminal_role=ROLE,terminal_kind='failed',actual_terminal_offer_kind='failed',terminal_offer_error='index receiver closed',terminal_offer_current=False,data_publish_end_ms=None,stale_full_data_abort_limit=1,stale_full_data_abort_duplicate=False,stale_full_data_abort=dict(reason='stale-full-data',request_id=6,tab_id=2,response_request_id=6,data_kind='batch',at_ms=4.25,latest_request_id=None,latest_lookup_succeeded=True,shutdown=False),emitted_workload_semantics='actual partial failed work; not full generation throughput')
 v.update(role=ROLE,workload_semantics='actual partial stale-full-failed work; not 100k indexing throughput')
 return rr
results=[]
with tempfile.TemporaryDirectory(prefix='stale-full-collector-',dir=HERE) as tmp:
 tmp=Path(tmp)
 def check(name,rr,expected=False):
  log=tmp/(name+'.log');out=tmp/(name+'.json');log.write_text('INDEX_PERF_META '+json.dumps(meta)+'\n'+''.join('INDEX_PERF_SAMPLE '+json.dumps(r)+'\n' for r in rr)+SUMMARY+'\n')
  p=subprocess.run(['python3',str(COLLECTOR),str(log),'--rows','2','--output',str(out)],text=True,stdout=subprocess.PIPE,stderr=subprocess.STDOUT)
  assert (p.returncode==0)==expected,(name,p.stdout);assert out.exists()==expected
  results.append(dict(name=name,exit_code=p.returncode,accepted=expected))
 check('valid-real-shape-stale-full-failed',sample(),True)
 rr=sample();rr[1]['actual_preemption_events'][0]['mutation_ms']=5;rr[1]['index_requests'][0]['followup_preempt_ms']=5;rr[1]['index_requests'][0]['stale_full_data_abort'].update(latest_request_id=0,data_kind='replace-all',at_ms=5.25);check('valid-zero-marker-replaceall',rr,True)
 for i in range(24):
  rr=sample();q=rr[1]['index_requests'][0];a=q['stale_full_data_abort'];v=rr[1]['declared_retained_victims'][0]
  if i==0:q['stale_full_data_abort']=None
  elif i==1:q['stale_full_data_abort_duplicate']=True
  elif i==2:q['stale_full_data_abort_limit']=2
  elif i==3:a['request_id']=5
  elif i==4:a['tab_id']=1
  elif i==5:a['response_request_id']=5
  elif i==6:a['data_kind']='terminal'
  elif i==7:a['latest_lookup_succeeded']=False
  elif i==8:a['shutdown']=True
  elif i==9:a['latest_request_id']=6
  elif i==10:a['at_ms']=3
  elif i==11:a['at_ms']=8
  elif i==12:a['reason']='closed'
  elif i==13:q['terminal_offer_error']='other error'
  elif i==14:q['terminal_offer_current']=True
  elif i==15:q['terminal_kind']='finished'
  elif i==16:q['actual_terminal_offer_kind']='canceled'
  elif i==17:q['terminal_publish_ms']=None
  elif i==18:q['request_processing_returned_ms']=None
  elif i==19:q['data_publish_end_ms']=1
  elif i==20:q['terminal_role']='declared-Warm-eviction-retained-last-good'
  elif i==21:rr[1]['declared_eviction_edges'][0]['stage']=1
  elif i==22:v['role']='declared-Warm-eviction-retained-last-good'
  elif i==23:v['actual_all_signature']='corrupt'
  check('invalid-stale-full-'+str(i),rr)
 for tab in [1,3]:
  rr=sample();q=next(q for q in rr[1]['index_requests'] if q['tab_id']==tab)
  q.update(terminal_kind='failed',actual_terminal_offer_kind='failed',terminal_offer_error='index receiver closed',terminal_offer_current=False)
  check('required-latest-failed-tab'+str(tab),rr)
 packet=dict(purpose='synthetic schema controls only',collector_sha256=hashlib.sha256(COLLECTOR.read_bytes()).hexdigest(),passed=len(results),results=results)
 (HERE/'stale-full-collector-guards.json').write_text(json.dumps(packet,indent=2)+'\n')
print('PASS',len(results),'stale Full collector checks')
