"""Explicit synthetic protocol controls; retained rows are not historical measurements."""
import copy, hashlib, importlib.util, json, re, unittest
from pathlib import Path
B=Path(__file__).resolve().parent;S=B.parent
spec=importlib.util.spec_from_file_location('historical_collector',B/'collect_historical.py');mod=importlib.util.module_from_spec(spec);spec.loader.exec_module(mod)
identity=json.loads((S/'newtag-repair-r4/guard-execution-packet.json').read_text())
source_log=S/'stale-full-extra-full.log';native_lines=source_log.read_bytes().decode().splitlines();raw=[json.loads(l.split(' ',1)[1]) for l in native_lines if l.startswith('INDEX_PERF_SAMPLE ')];native_meta=next(json.loads(l.split('INDEX_PERF_META ',1)[1]) for l in native_lines if 'INDEX_PERF_META 'in l)
PASS_TRAILER=next(l for l in native_lines if l.startswith('test result:'))
FAIL_TRAILER=next(l for l in (S/'warm-final-extra-full.log').read_text().splitlines() if l.startswith('test result:'))
RUNNER='app::tests::indexing_perf::harness::extensions::runner::perf_indexing_extended_paired'
DEFAULT_CELLS=[('F1-files','Walker'),('F1-folders','Walker'),('W1-deep','Walker')]
def event(name,x):return 'INDEX_PERF_'+name+' '+json.dumps(x,separators=(',',':'))
def make(statuses=('PASS','FAIL','PASS'),counts=None,cleanup=None,cells=DEFAULT_CELLS,truncated=False):
    counts=counts or [14]*len(statuses);cleanup=cleanup or [(True,True)]*len(statuses)
    meta=copy.deepcopy(native_meta);meta.update(selected_cases=list(dict.fromkeys(c for c,s in cells)),selected_sources=list(dict.fromkeys(s for c,s in cells)),supported_cells=[{'case':c,'source':s} for c,s in cells],selected_source_cells=len(cells),expected_rows=len(cells)*14,coverage_kind='selected-subset')
    test=RUNNER
    if truncated:
        tl=(S/'stale-full-extra-trunc.log').read_text().splitlines();meta=next(json.loads(l.split('INDEX_PERF_META ',1)[1]) for l in tl if 'INDEX_PERF_META 'in l);test=RUNNER.replace('perf_indexing_extended_paired','perf_indexing_truncated_serial');cells=[('W1-truncated','Walker')]
        tr=[json.loads(l.split(' ',1)[1]) for l in tl if l.startswith('INDEX_PERF_SAMPLE ')]
    lines=['running 1 test','test '+test+' ... '+event('META',meta)];retained=[]
    for index,((case,source),status) in enumerate(zip(cells,statuses)):
        if status=='NOT_RUN':lines.append(event('CELL_STATUS',{'case':case,'source':source,'status':'NOT_RUN','reason':'previous cell cleanup or restoration unproven'}));continue
        entries=500001 if truncated else 100000;lines.append(event('CELL_START',{'case':case,'source':source,'pairs':7,'entries':entries,'expected_rows':14}))
        rows=copy.deepcopy(tr if truncated else [r for r in raw if r['comparison']==case and r['source']==source]);assert len(rows)==14
        if not truncated:
            for cond in [False,True]:lines.append(event('RUN_START',{'profile':case,'source':source,'condition':cond,'role':'untimed-warmup','entries':entries}))
        for row in rows[:counts[index]]:
            if not truncated:lines.append(event('RUN_START',{'profile':case,'source':source,'condition':row['case']!='B0','role':'sample','entries':entries,'pair':row['pair'],'position':row['position']}))
            lines.append(event('SAMPLE',row));retained.append(row)
        a,z=cleanup[index];st={'case':case,'source':source,'status':status,'actual_partial_rows':counts[index],'accepted_rows':counts[index] if status=='PASS' else 0,'positive_cleanup':a,'root_restored':z,'cleanup_evidence':'synthetic-control; not a reconstructed Debug schema','panic':None if status=='PASS' else 'synthetic intentional failure'}
        if truncated:st.update(config_equal_before=True,config_restoration_authorized=True,actual_config_before=copy.deepcopy(native_meta['runtime_settings']),actual_config_after=copy.deepcopy(native_meta['runtime_settings']))
        lines.append(event('CELL_STATUS',st))
    failed=any(s!='PASS' for s in statuses);lines.append(FAIL_TRAILER if failed else PASS_TRAILER);text='\n'.join(lines)+'\n'
    tag=identity['tags'][-1];selection={'runner':meta['runner'],'cells':[{'case':c,'source':s} for c,s in cells],'selected_cases':meta['selected_cases'],'selected_sources':meta['selected_sources'],'pairs':7,'entries':500001 if truncated else 100000,'expected_rows':len(cells)*14,'coverage_kind':'cap+1' if truncated else 'selected-subset','cap':500000}
    env={'FLISTWALKER_SEARCH_THREADS':'12','FLISTWALKER_SEARCH_PARALLEL_THRESHOLD':'25000','FLISTWALKER_WALKER_MAX_ENTRIES':'500000','FW_INDEX_PERF_EXTRA_PAIRS':'7'}
    if not truncated:env.update(FW_INDEX_PERF_EXTRA_ENTRIES='100000',FW_INDEX_PERF_EXTRA_CASES=','.join(selection['selected_cases']),FW_INDEX_PERF_EXTRA_SOURCES=','.join(selection['selected_sources']))
    side={'schema_version':1,'guard_checkpoint_sha256':'19ba26595911a3f18de7175bb7b392d0f461fd3faa23c336f58cc91914c5988d','evidence_kind':'synthetic-control','synthetic_provenance':{'rows_log':str(source_log),'rows_sha256':hashlib.sha256(source_log.read_bytes()).hexdigest(),'failed_trailer_log':'warm-final-extra-full.log','notice':'No live historical test/performance outcome; copied trailers are explicit protocol inputs only; current collector never invoked'},'log_sha256':hashlib.sha256(text.encode()).hexdigest(),'exit_code':101 if failed else 0,'process_completed':True,'actual_wait_returned':True,'watchdog_expired':False,'owned_process_group_absent':True,'runner_test':test,'cwd':tag['export']+'/rust','command':['cargo','+1.97.1','test','--release','--locked','--offline','--lib',test,'--','--ignored','--exact','--nocapture','--test-threads=1'],'env':env|{'CARGO_TARGET_DIR':str(B/('synthetic-target-'+tag['tag']))},'selection':selection,'source_identity':{'tag':tag['tag'],'original_commit':tag['original_commit'],'original_archive_sha256':tag['original_archive_sha256'],'Cargo_lock_sha256':tag['Cargo_lock_sha256'],'source_manifest_sha256':tag['source_manifest_sha256'],'adapter_sha256':tag['full_patch']['sha256']},'validator_sha256':hashlib.sha256((B/'collect_historical.py').read_bytes()).hexdigest()}
    return text,side
class HistoricalControls(unittest.TestCase):
    def test_failed_cell_complete_rows_never_enter_aggregation(self):
        text,side=make();out=mod.collect(text,side,identity)
        self.assertEqual(out['accepted_rows'],28)
        self.assertEqual(out['excluded_partial_rows'],14)
        self.assertEqual(out['raw_exit_code'],101)
        self.assertEqual(out['process_outcome'],'FAILED')
        self.assertEqual(len(out['comparisons']),2)
if __name__=='__main__':unittest.main(verbosity=2)

CONTROL_RESULTS=[]
def rewrite(text,side,fn):
    out=[]
    for line in text.splitlines():
        if line.startswith('INDEX_PERF_'):
            kind,payload=line.split(' ',1);e=json.loads(payload);replacement=fn(kind,e)
            if replacement is None:continue
            if isinstance(replacement,list):out.extend(replacement);continue
            out.append(kind+' '+json.dumps(replacement))
        else:out.append(line)
    value='\n'.join(out)+'\n';side['log_sha256']=hashlib.sha256(value.encode()).hexdigest();return value,side

def mutate_once(kind,fn,predicate=lambda e:True):
    state={'done':False}
    def change(k,e):
        if k==kind and not state['done'] and predicate(e):state['done']=True;fn(e)
        return e
    return change

def set_field(field,value):return lambda e:e.__setitem__(field,value)

def negative_control(name,mutation,builder=lambda:make(('PASS','PASS','PASS'))):
    def test(self):
        text,side=builder();text,side=mutation(text,side)
        with self.assertRaises(ValueError):mod.collect(text,side,identity)
        CONTROL_RESULTS.append({'name':name,'expected':'REJECT','observed':'REJECT','passed':True,'kind':'synthetic-control'})
    test.__name__='test_reject_'+name;setattr(HistoricalControls,test.__name__,test)

def ev(kind,fn,predicate=lambda e:True):return lambda text,side:rewrite(text,side,mutate_once(kind,fn,predicate))
def sc(fn):
    def change(text,side):fn(side);return text,side
    return change

def raw_change(fn):
    def change(text,side):text=fn(text);side['log_sha256']=hashlib.sha256(text.encode()).hexdigest();return text,side
    return change

NEGATIVES={
'unknown_exit':sc(lambda s:s.__setitem__('exit_code',None)),
'compile_failure_exit_not_performance':sc(lambda s:s.__setitem__('exit_code',1)),
'watchdog_killed_process':sc(lambda s:s.__setitem__('watchdog_expired',True)),
'physical_process_wait_missing':sc(lambda s:s.__setitem__('actual_wait_returned',False)),
'owned_process_group_not_absent':sc(lambda s:s.__setitem__('owned_process_group_absent',False)),
'log_sha_mismatch':sc(lambda s:s.__setitem__('log_sha256','0'*64)),
'validator_sha_mismatch':sc(lambda s:s.__setitem__('validator_sha256','0'*64)),
'original_tag_commit_mismatch':sc(lambda s:s['source_identity'].__setitem__('original_commit','0'*40)),
'original_lock_mismatch':sc(lambda s:s['source_identity'].__setitem__('Cargo_lock_sha256','0'*64)),
'original_archive_mismatch':sc(lambda s:s['source_identity'].__setitem__('original_archive_sha256','0'*64)),
'final_source_map_mismatch':sc(lambda s:s['source_identity'].__setitem__('source_manifest_sha256','0'*64)),
'adapter_identity_mismatch':sc(lambda s:s['source_identity'].__setitem__('adapter_sha256','0'*64)),
'command_new_features_forbidden':sc(lambda s:s['command'].insert(7,'--all-features')),
'command_wrong_toolchain':sc(lambda s:s['command'].__setitem__(1,'+stable')),
'command_wrong_exact_runner':sc(lambda s:s.__setitem__('runner_test',mod.TRUNC_TEST)),
'command_wrong_cwd':sc(lambda s:s.__setitem__('cwd','/private/tmp/unrelated')),
'wrong_runtime_threshold':sc(lambda s:s['env'].__setitem__('FLISTWALKER_SEARCH_PARALLEL_THRESHOLD','1')),
'undesignated_target':sc(lambda s:s['env'].__setitem__('CARGO_TARGET_DIR','/private/tmp/shared-target')),
'independent_selection_missing_cell':sc(lambda s:s['selection']['cells'].pop()),
'independent_expected_rows_wrong':sc(lambda s:s['selection'].__setitem__('expected_rows',41)),
'META_wrong_scale':raw_change(lambda t:t.replace('"entries":100000','"entries":99999',1)),
'META_wrong_settings':raw_change(lambda t:t.replace('"search_threads":12','"search_threads":1',1)),
'META_duplicate':raw_change(lambda t:t.replace('INDEX_PERF_CELL_START ',next(line.split(' ... ',1)[1] for line in t.splitlines() if ' ... INDEX_PERF_META 'in line)+'\nINDEX_PERF_CELL_START ',1)),
'META_absent':raw_change(lambda t:'\n'.join(line for line in t.splitlines() if 'INDEX_PERF_META 'not in line)+'\n'),
'START_wrong_source':ev('INDEX_PERF_CELL_START',set_field('source','FileList')),
'START_out_of_order':ev('INDEX_PERF_CELL_START',set_field('case','F1-folders')),
'START_bool_pair':ev('INDEX_PERF_CELL_START',set_field('pairs',True)),
'RUN_START_bool_as_int':ev('INDEX_PERF_RUN_START',set_field('condition',0)),
'RUN_START_wrong_warmup_order':ev('INDEX_PERF_RUN_START',set_field('condition',True)),
'RUN_START_missing':raw_change(lambda t:'\n'.join(line for line in t.splitlines() if not line.startswith('INDEX_PERF_RUN_START '))+'\n'),
'SAMPLE_duplicate_pair_position':ev('INDEX_PERF_SAMPLE',set_field('pair',1)),
'SAMPLE_wrong_ABBA_order':ev('INDEX_PERF_SAMPLE',set_field('order','BA')),
'SAMPLE_wrong_position':ev('INDEX_PERF_SAMPLE',set_field('position',1)),
'SAMPLE_wrong_comparison':ev('INDEX_PERF_SAMPLE',set_field('comparison','W1-wide')),
'SAMPLE_wrong_source':ev('INDEX_PERF_SAMPLE',set_field('source','FileList')),
'SAMPLE_false_correct':ev('INDEX_PERF_SAMPLE',set_field('correct',False)),
'SAMPLE_false_eligible':ev('INDEX_PERF_SAMPLE',set_field('contention_eligible',False)),
'SAMPLE_truthy_correct':ev('INDEX_PERF_SAMPLE',set_field('correct','true')),
'SAMPLE_infinite_phase':ev('INDEX_PERF_SAMPLE',set_field('full_wait_ms',float('inf'))),
'SAMPLE_boolean_phase':ev('INDEX_PERF_SAMPLE',set_field('full_wait_ms',True)),
'SAMPLE_wrong_workload_count':ev('INDEX_PERF_SAMPLE',set_field('sample_entries',99999)),
'SAMPLE_wrong_semantic_kind':ev('INDEX_PERF_SAMPLE',set_field('comparison_kind','AB-operation-cost')),
'SAMPLE_wrong_measurement_kind':ev('INDEX_PERF_SAMPLE',set_field('measurement_kind','worker-only-parser')),
'SAMPLE_producer_chronology':ev('INDEX_PERF_SAMPLE',lambda e:e.__setitem__('data_publish_end_ms',e['terminal_publish_ms']+1)),
'SAMPLE_GUI_chronology':ev('INDEX_PERF_SAMPLE',lambda e:e.__setitem__('results_ready_ms',e['index_ready_ms']-1)),
'SAMPLE_sender_queued_at_t2':ev('INDEX_PERF_SAMPLE',lambda e:e['index_sender_load_at_t2'].__setitem__('queued',1)),
'SAMPLE_missing_exact_release':ev('INDEX_PERF_SAMPLE',lambda e:e['released_request_ids'].pop()),
'STATUS_wrong_actual_rows':ev('INDEX_PERF_CELL_STATUS',set_field('actual_partial_rows',13)),
'STATUS_wrong_accepted_rows':ev('INDEX_PERF_CELL_STATUS',set_field('accepted_rows',13)),
'STATUS_truthy_cleanup':ev('INDEX_PERF_CELL_STATUS',set_field('positive_cleanup','true')),
'STATUS_root_unrestored_PASS':ev('INDEX_PERF_CELL_STATUS',set_field('root_restored',False)),
'STATUS_missing_cleanup':ev('INDEX_PERF_CELL_STATUS',lambda e:e.pop('positive_cleanup')),
'STATUS_panic_on_PASS':ev('INDEX_PERF_CELL_STATUS',set_field('panic','real failure')),
'STATUS_noneligible_not_typed':ev('INDEX_PERF_CELL_STATUS',set_field('status','NON_ELIGIBLE')),
'STATUS_runtime_failure_not_unsupported':ev('INDEX_PERF_CELL_STATUS',set_field('status','UNSUPPORTED')),
'STATUS_duplicate':raw_change(lambda t:t.replace(next(l for l in t.splitlines() if l.startswith('INDEX_PERF_CELL_STATUS ')),next(l for l in t.splitlines() if l.startswith('INDEX_PERF_CELL_STATUS '))+'\n'+next(l for l in t.splitlines() if l.startswith('INDEX_PERF_CELL_STATUS ')),1)),
'STATUS_missing_after_lost_process':raw_change(lambda t:'\n'.join(l for l in t.splitlines() if not l.startswith('INDEX_PERF_CELL_STATUS '))+'\n'),
'whole_FAILED_all_PASS':raw_change(lambda t:t.replace(PASS_TRAILER,FAIL_TRAILER)),
'whole_exit0_FAILED_trailer':sc(lambda s:s.__setitem__('exit_code',101)),
'whole_absent_trailer':raw_change(lambda t:t.replace(PASS_TRAILER,'')),
'whole_duplicate_trailer':raw_change(lambda t:t+PASS_TRAILER+'\n'),
'extra_selected_test':raw_change(lambda t:t+'test unrelated::test ... ok\n'),
'zero_discovery':raw_change(lambda t:t.replace('running 1 test','running 0 tests')),
'panic_substring_is_not_META':raw_change(lambda t:t.replace('test '+RUNNER+' ... INDEX_PERF_META ','test '+RUNNER+' ... panic quoted INDEX_PERF_META ',1)),
'JSON_duplicate_key':raw_change(lambda t:t.replace('INDEX_PERF_CELL_START {','INDEX_PERF_CELL_START {"pairs":7,',1)),
'JSON_malformed_event':raw_change(lambda t:t.replace('INDEX_PERF_CELL_START {','INDEX_PERF_CELL_START [',1)),
}
for name,mutation in NEGATIVES.items():negative_control(name,mutation)

negative_control('FAIL_partial_accepted_rows_nonzero',ev('INDEX_PERF_CELL_STATUS',set_field('accepted_rows',14),lambda e:e['status']=='FAIL'),lambda:make())
negative_control('START_after_uncertain_cleanup',ev('INDEX_PERF_CELL_STATUS',set_field('positive_cleanup',False),lambda e:e['status']=='FAIL'),lambda:make())
negative_control('START_after_unrestored_root',ev('INDEX_PERF_CELL_STATUS',set_field('root_restored',False),lambda e:e['status']=='FAIL'),lambda:make())
negative_control('NOT_RUN_without_uncertainty',lambda t,s:(t,s),lambda:make(('PASS','NOT_RUN','NOT_RUN')))
negative_control('cap_equal_without_authorization',ev('INDEX_PERF_CELL_STATUS',set_field('config_restoration_authorized',False)),lambda:make(('PASS',),cells=[('W1-truncated','Walker')],truncated=True))
negative_control('cap_actual_config_mismatch',ev('INDEX_PERF_CELL_STATUS',lambda e:e['actual_config_after'].__setitem__('walker_max_entries',3)),lambda:make(('PASS',),cells=[('W1-truncated','Walker')],truncated=True))
negative_control('cap_wrong_actual_limit',ev('INDEX_PERF_SAMPLE',set_field('actual_limit',3)),lambda:make(('PASS',),cells=[('W1-truncated','Walker')],truncated=True))
negative_control('parser_false_GUI_phase',ev('INDEX_PERF_SAMPLE',set_field('index_ready_ms',1)),lambda:make(('PASS',),cells=[('F1-filelist-parser-files','FileList')]))

class HistoricalPositiveControls(unittest.TestCase):
    def test_all_PASS_complete_subset(self):
        t,s=make(('PASS','PASS','PASS'));o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],42);self.assertEqual(o['raw_exit_code'],0);self.assertEqual(o['status_counts']['PASS'],3)
    def test_FAIL_partial_rows_excluded(self):
        t,s=make(counts=[14,3,14]);o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],28);self.assertEqual(o['excluded_partial_rows'],3)
    def test_uncertain_FAIL_then_NOT_RUN(self):
        t,s=make(('PASS','FAIL','NOT_RUN'),[14,0,0],[(True,True),(False,False),(False,False)]);o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],14);self.assertEqual(o['status_counts'],{'PASS':1,'FAIL':1,'NOT_RUN':1})
    def test_all_excluded_uncertain_and_remaining_NOT_RUN(self):
        t,s=make(('FAIL','NOT_RUN','NOT_RUN'),[0,0,0],[(False,False)]*3);o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],0);self.assertEqual(o['comparisons'],[])
    def test_generic_eligibility_panic_remains_FAIL(self):
        t,s=make();t,s=ev('INDEX_PERF_CELL_STATUS',set_field('panic','actual operation must overlap'),lambda e:e['status']=='FAIL')(t,s);o=mod.collect(t,s,identity);self.assertEqual(o['status_counts']['FAIL'],1);self.assertNotIn('NON_ELIGIBLE',o['status_counts'])
    def test_cap_PASS_complete(self):
        t,s=make(('PASS',),cells=[('W1-truncated','Walker')],truncated=True);o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],14)
    def test_cap_config_failure_excludes_all(self):
        t,s=make(('FAIL',),cells=[('W1-truncated','Walker')],truncated=True)
        def change(e):e['panic']=None;e['actual_config_after']['walker_max_entries']=3;e['config_equal_before']=False
        t,s=ev('INDEX_PERF_CELL_STATUS',change)(t,s);o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],0);self.assertEqual(o['excluded_partial_rows'],14)
    def test_worker_only_parser_PASS(self):
        t,s=make(('PASS',),cells=[('F1-filelist-parser-files','FileList')]);o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],14)
    def test_actual_shaped_victim_contract_PASS(self):
        t,s=make(('PASS',),cells=[('T1-A-B-C-A','Walker')]);o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],14)

def victim_row(e):return e['case']!='B0' and bool(e.get('declared_retained_victims'))
def retained(e):return next(q for q in e['index_requests'] if q['terminal_role'] in mod.validate_and_aggregate.__globals__['RETAINED_ROLES'])
def latest_required(e):return next(q for q in e['index_requests'] if q['latest_generation_required'])
def victim_negative(name,fn,predicate=victim_row):
    negative_control('victim_'+name,ev('INDEX_PERF_SAMPLE',fn,predicate),lambda:make(('PASS',),cells=[('T1-A-B-C-A','Walker')]))
victim_negative('preemption_observer_overflow',set_field('preemption_observer_overflow',True))
victim_negative('removal_observer_overflow',set_field('warm_removal_observer_overflow',True))
victim_negative('wrong_bounded_limit',set_field('preemption_observer_limit',999))
victim_negative('wrong_seed_policy',set_field('retained_seed_validation_policy','old snapshot allowed'))
victim_negative('wrong_earliest_cause',lambda e:retained(e).__setitem__('actual_cause_kind',None))
victim_negative('wrong_earliest_time',lambda e:retained(e).__setitem__('invalidating_mutation_ms',None))
victim_negative('wrong_latest_ledger_role',lambda e:retained(e).__setitem__('latest_measured_request',False))
victim_negative('required_latest_cannot_cancel',lambda e:latest_required(e).__setitem__('terminal_kind','canceled'))
victim_negative('required_latest_cannot_use_old_seed',lambda e:latest_required(e).__setitem__('terminal_role','declared-Warm-eviction-retained-last-good'))
victim_negative('retained_cannot_be_required_generation',lambda e:retained(e).__setitem__('latest_generation_required',True))
victim_negative('retained_closed_receiver_beforestart_forbidden',lambda e:retained(e).__setitem__('skipped_closed_before_start',True))
victim_negative('retained_emitted_work_positive',lambda e:retained(e).__setitem__('entries_emitted',0))
victim_negative('retained_terminal_is_stale',lambda e:retained(e).__setitem__('terminal_offer_current',True))
victim_negative('retained_actual_started_root',lambda e:retained(e).__setitem__('actual_started_root','unrelated-root'))
victim_negative('retained_worker_body_must_return',lambda e:retained(e).__setitem__('request_processing_returned_ms',e['index_ready_ms']+1))
victim_negative('post_t3_seed_full_oracle',lambda e:e['declared_retained_victims'][0].__setitem__('full_seed_oracle_after_t3',False))
victim_negative('post_t3_seed_request_identity',lambda e:e['declared_retained_victims'][0].__setitem__('actual_seed_request_id',0))
victim_negative('post_t3_seed_root_identity',lambda e:e['declared_retained_victims'][0].__setitem__('actual_seed_root','wrong-root'))
victim_negative('post_t3_seed_source_identity',lambda e:e['declared_retained_victims'][0].__setitem__('actual_seed_source','FileList'))
victim_negative('post_t3_seed_full_membership',lambda e:e['declared_retained_victims'][0].__setitem__('actual_all_count',99999))
victim_negative('post_t3_seed_visible_membership',lambda e:e['declared_retained_victims'][0].__setitem__('actual_visible_count',99999))
victim_negative('post_t3_seed_signature',lambda e:e['declared_retained_victims'][0].__setitem__('actual_all_signature','wrong'))
victim_negative('post_t3_partial_entry_semantics',lambda e:e['declared_retained_victims'][0].__setitem__('partial_emitted_entries',0))
victim_negative('switch_ack_pending_activation',lambda e:e['declared_eviction_edges'][0]['actual_switch_ack'].__setitem__('pending_activation_tab_id',123))
victim_negative('switch_ack_root',lambda e:e['declared_eviction_edges'][0]['actual_switch_ack'].__setitem__('root','wrong-root'))
victim_negative('removal_route_tab',lambda e:e['actual_warm_removal_events'][0].__setitem__('route_tab',0),lambda e:bool(e.get('actual_warm_removal_events')))
victim_negative('removal_replacement_warm',lambda e:e['actual_warm_removal_events'][0].__setitem__('replacement_warm_tab',0),lambda e:bool(e.get('actual_warm_removal_events')))
victim_negative('preempt_actual_queued_trigger',lambda e:e['actual_preemption_events'][0].__setitem__('queued_active_request_ids',[]),lambda e:bool(e.get('actual_preemption_events')))
victim_negative('preempt_actual_active_tab',lambda e:e['actual_preemption_events'][0].__setitem__('actual_active_tab',0),lambda e:bool(e.get('actual_preemption_events')))
def stale_row(e):return any(q.get('terminal_role')=='declared-Warm-eviction-stale-Full-retained-last-good' for q in e.get('index_requests',[]))
def stale(e):return next(q for q in e['index_requests'] if q['terminal_role']=='declared-Warm-eviction-stale-Full-retained-last-good')
victim_negative('stale_Full_no_actual_abort',lambda e:stale(e).__setitem__('stale_full_data_abort',None),stale_row)
victim_negative('stale_Full_lookup_unknown',lambda e:stale(e)['stale_full_data_abort'].__setitem__('latest_lookup_succeeded',False),stale_row)
victim_negative('stale_Full_wrong_latest',lambda e:stale(e)['stale_full_data_abort'].__setitem__('latest_request_id',stale(e)['request_id']),stale_row)
victim_negative('stale_Full_native_shutdown_not_samecause',lambda e:stale(e)['stale_full_data_abort'].__setitem__('shutdown',True),stale_row)
victim_negative('stale_Full_after_terminal',lambda e:stale(e)['stale_full_data_abort'].__setitem__('at_ms',stale(e)['terminal_offered_ms']+1),stale_row)
victim_negative('stale_Full_failed_error_alone',lambda e:stale(e).__setitem__('terminal_offer_error','generic failure'),stale_row)

class HistoricalFullControl(unittest.TestCase):
    def test_full44_declared_matrix_all_PASS_synthetic(self):
        cells=[(x['case'],x['source']) for x in native_meta['supported_cells']]
        t,s=make(('PASS',)*44,cells=cells)
        line=next(l for l in t.splitlines() if ' ... INDEX_PERF_META 'in l);prefix,payload=line.split('INDEX_PERF_META ',1);m=json.loads(payload);m.update(selected_sources=['FileList','Walker'],coverage_kind='all-extended-nontruncated')
        t=t.replace(line,prefix+'INDEX_PERF_META '+json.dumps(m));s['log_sha256']=hashlib.sha256(t.encode()).hexdigest();s['selection'].update(selected_sources=['FileList','Walker'],coverage_kind='all-extended-nontruncated');s['env']['FW_INDEX_PERF_EXTRA_SOURCES']='FileList,Walker'
        o=mod.collect(t,s,identity);self.assertEqual(o['accepted_rows'],616);self.assertEqual(o['status_counts'],{'PASS':44,'FAIL':0,'NOT_RUN':0});self.assertEqual(len(o['comparisons']),44)
negative_control('guard_checkpoint_SHA_mismatch',sc(lambda s:s.__setitem__('guard_checkpoint_sha256','0'*64)))
negative_control('cap_config_equal_false_authority_string',ev('INDEX_PERF_CELL_STATUS',set_field('config_restoration_authorized','true')),lambda:make(('PASS',),cells=[('W1-truncated','Walker')],truncated=True))
negative_control('schema_version_bool_is_not_integer',sc(lambda s:s.__setitem__('schema_version',True)))
negative_control('META_schema_version_bool',raw_change(lambda t:t.replace('"schema_version":1','"schema_version":true',1)))
negative_control('SAMPLE_schema_version_bool',ev('INDEX_PERF_SAMPLE',set_field('schema_version',True)))
negative_control('run_pair_bool_is_not_integer',ev('INDEX_PERF_RUN_START',set_field('pair',False),lambda e:e['role']=='sample'))
negative_control('run_position_bool_is_not_integer',ev('INDEX_PERF_RUN_START',set_field('position',False),lambda e:e['role']=='sample'))
negative_control('rows_outside_active_cell',raw_change(lambda t:t+next(l for l in t.splitlines() if l.startswith('INDEX_PERF_SAMPLE '))+'\n'))
negative_control('NOT_RUN_must_have_reason',ev('INDEX_PERF_CELL_STATUS',lambda e:e.pop('reason'),lambda e:e['status']=='NOT_RUN'),lambda:make(('PASS','FAIL','NOT_RUN'),[14,0,0],[(True,True),(False,False),(False,False)]))
class HistoricalCheckpointControl(unittest.TestCase):
    def test_mutated_guard_checkpoint_rejected(self):
        t,s=make(('PASS','PASS','PASS'));changed=copy.deepcopy(identity);changed['tags'][0]['original_commit']='0'*40
        with self.assertRaises(ValueError):mod.collect(t,s,changed)
