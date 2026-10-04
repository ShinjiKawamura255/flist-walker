"""Fail-closed historical evidence collector. PASS cells alone supply admitted timings.
The real process exit and libtest trailer remain unchanged even for a mixed matrix.
Generic FAIL is never inferred to mean NON_ELIGIBLE or unsupported.
"""
import argparse, hashlib, json, math, re
from pathlib import Path
from fbd9_predicates import validate_and_aggregate
B=Path(__file__).resolve().parent
EXPECTED_GUARD_CHECKPOINT='19ba26595911a3f18de7175bb7b392d0f461fd3faa23c336f58cc91914c5988d'
EXPECTED_GUARD_CANONICAL='4f06d40e74ce344008de9f0e0c53d43ea20c4244fb89b1ef3c2dc3f6f8ca163f'
EXPECTED_PREDICATE_MODULE='22d9ca437991b3db8daeeac25618defea962c3dc89619e6ac1f2675354cb8429'
EXPECTED_COLLECTOR='fbd9bdf4f5b02d08857033a0309bec3775c4f9f605bdc65492a389bd748aa0aa'
PHASES=['data_publish_end_ms','terminal_publish_ms','index_ready_ms','results_ready_ms','worker_drained_ms','full_wait_ms','max_frame_ms','max_ingest_gap_ms','max_no_work_progress_ms']
ORDINARY_TEST='app::tests::indexing_perf::harness::extensions::runner::perf_indexing_extended_paired'
TRUNC_TEST=ORDINARY_TEST.replace('perf_indexing_extended_paired','perf_indexing_truncated_serial')
AA={'F1-files','F1-folders','R1-natural','H1-early','H1-late','W1-follow-links','W1-deep','W1-wide','W1-truncated','F1-filelist-parser-files','F1-filelist-parser-folders'}
AB_COST={'F1-ignore-list','F1-ignore-case','F1-mid-ignore','O1-name-shown','O1-modified-shown','O1-name-all','O1-modified-all'}
PROFILE_ORDER=['F1-files','F1-folders','F1-ignore-list','F1-ignore-case','F1-mid-ignore','S1-ignore','S2-files','O1-name-shown','O1-modified-shown','O1-name-all','O1-modified-all','T1-active-warm','T1-promotion','T1-A-B-C-A','T1-S1-selective','T1-S1-dense','T1-S2','R1-natural','T1-natural-reclaim','H1-early','H1-late','P1-preview','W1-follow-links','W1-deep','W1-wide','F1-filelist-parser-files','F1-filelist-parser-folders']
WALKER_ONLY={'F1-files','F1-folders','S2-files','W1-follow-links','W1-deep','W1-wide'}
FILELIST_ONLY={'H1-early','H1-late','F1-filelist-parser-files','F1-filelist-parser-folders'}
PARSER={'F1-filelist-parser-files','F1-filelist-parser-folders'}
def need(value, message):
    if not value: raise ValueError(message)
def sha(path): return hashlib.sha256(Path(path).read_bytes()).hexdigest()
def integer(value,minimum=0): return type(value) is int and value>=minimum
def finite(value): return type(value) in (int,float) and math.isfinite(value) and value>=0
def no_duplicate_keys(pairs):
    result={}
    for key,value in pairs:
        need(key not in result,'duplicate JSON key '+key);result[key]=value
    return result
def parse_json(value):
    try: return json.loads(value,object_pairs_hook=no_duplicate_keys,parse_constant=lambda x:(_ for _ in ()).throw(ValueError('nonfinite JSON number '+x)))
    except (json.JSONDecodeError,TypeError) as e:raise ValueError('malformed event JSON') from e
def finite_tree(value):
    if type(value) is float: need(math.isfinite(value),'nonfinite scalar')
    elif isinstance(value,dict):
        for x in value.values(): finite_tree(x)
    elif isinstance(value,list):
        for x in value: finite_tree(x)
def key(event):
    need(isinstance(event,dict) and type(event.get('case')) is str and type(event.get('source')) is str,'cell key schema')
    return (event['case'],event['source'])
def selected_cells(cases,sources):
    return [(c,s) for c in cases for s in (['Walker'] if c in WALKER_ONLY else ['FileList'] if c in FILELIST_ONLY else ['FileList','Walker']) if s in sources]
def bind_sidecar(text,side,identity):
    need(hashlib.sha256(json.dumps(identity,sort_keys=True,separators=(',',':')).encode()).hexdigest()==EXPECTED_GUARD_CANONICAL,'accepted guard checkpoint identity')
    need(side.get('guard_checkpoint_sha256')==EXPECTED_GUARD_CHECKPOINT,'sidecar accepted guard checkpoint')
    need(type(side.get('schema_version')) is int and side['schema_version']==1,'sidecar schema')
    need(side.get('evidence_kind') in ('live-historical','synthetic-control'),'explicit evidence kind')
    if side['evidence_kind']=='synthetic-control':need(isinstance(side.get('synthetic_provenance'),dict),'synthetic provenance')
    need(side.get('log_sha256')==hashlib.sha256(text.encode()).hexdigest(),'full-log SHA mismatch')
    need(type(side.get('exit_code')) is int and side['exit_code'] in (0,101),'observed original process exit required')
    for field in ['process_completed','actual_wait_returned','owned_process_group_absent']:need(side.get(field) is True,'actual process completion '+field)
    need(side.get('watchdog_expired') is False,'killed/lost process cannot admit rows')
    need(side.get('validator_sha256')==sha(__file__),'validator SHA mismatch')
    need(sha(B.parent/'summarize_extensions.py')==EXPECTED_COLLECTOR,'current collector immutable')
    reuse=parse_json((B/'fbd9-reuse-identity.json').read_text())
    need(reuse['frozen_collector_sha256']==EXPECTED_COLLECTOR and reuse['generated_module_sha256']==sha(B/'fbd9_predicates.py')==EXPECTED_PREDICATE_MODULE,'predicate identity mismatch')
    ident=side.get('source_identity');need(isinstance(ident,dict),'source identity missing')
    tags=[t for t in identity['tags'] if t['tag']==ident.get('tag')];need(len(tags)==1,'unknown tag');tag=tags[0]
    for f in ['original_commit','original_archive_sha256','Cargo_lock_sha256','source_manifest_sha256']:need(ident.get(f)==tag[f],f+' mismatch')
    need(ident.get('adapter_sha256')==tag['full_patch']['sha256'],'adapter SHA mismatch')
    need(sha(tag['full_patch']['path'])==tag['full_patch']['sha256'],'pinned adapter readback')
    source_map=parse_json(Path(tag['source_map']['path']).read_text());root=Path(tag['export'])/'rust'
    actual={str(p.relative_to(root/'src')):sha(p) for p in (root/'src').rglob('*') if p.is_file()}
    need(actual==source_map,'live source map mismatch');need(sha(root/'Cargo.lock')==tag['Cargo_lock_sha256'],'original lock mismatch')
    need(side.get('cwd')==str(root),'command source cwd mismatch')
    selection=side.get('selection');need(isinstance(selection,dict),'selection absent')
    runner=selection.get('runner');need(runner in ('extended','truncated-serial'),'runner identity')
    test=ORDINARY_TEST if runner=='extended' else TRUNC_TEST
    need(side.get('runner_test')==test,'selected exact test mismatch')
    need(side.get('command')==['cargo','+1.97.1','test','--release','--locked','--offline','--lib',test,'--','--ignored','--exact','--nocapture','--test-threads=1'],'original compiler/default-feature exact command mismatch')
    env=side.get('env');need(isinstance(env,dict),'env missing')
    for f,v in {'FLISTWALKER_SEARCH_THREADS':'12','FLISTWALKER_SEARCH_PARALLEL_THRESHOLD':'25000','FLISTWALKER_WALKER_MAX_ENTRIES':'500000'}.items():need(env.get(f)==v,'runtime setting mismatch '+f)
    need(type(env.get('CARGO_TARGET_DIR')) is str and Path(env['CARGO_TARGET_DIR']).is_absolute() and ident['tag'] in Path(env['CARGO_TARGET_DIR']).name,'tag-dedicated target required')
    pairs=selection.get('pairs');entries=selection.get('entries');need(integer(pairs,1) and integer(entries,1),'scale schema')
    need(env.get('FW_INDEX_PERF_EXTRA_PAIRS')==str(pairs),'pair env mismatch')
    cases=selection.get('selected_cases');sources=selection.get('selected_sources')
    need(isinstance(cases,list) and cases and len(set(cases))==len(cases) and all(type(c) is str for c in cases),'case selection schema')
    need(isinstance(sources,list) and sources and len(set(sources))==len(sources) and set(sources)<={'FileList','Walker'},'source selection schema')
    cells=[key(c) for c in selection.get('cells',[])];need(cells and len(set(cells))==len(cells),'independent requested cells')
    need(selection.get('cap')==500000,'actual cap selection')
    if runner=='extended':
        need(set(cases)<=set(PROFILE_ORDER),'unknown requested case');need(cells==selected_cells(cases,sources),'source API selection mismatch')
        need(env.get('FW_INDEX_PERF_EXTRA_ENTRIES')==str(entries),'entries env mismatch')
        need(env.get('FW_INDEX_PERF_EXTRA_CASES')==','.join(cases) and env.get('FW_INDEX_PERF_EXTRA_SOURCES')==','.join(sources),'selected trace env mismatch')
        coverage='all-extended-nontruncated' if cases==PROFILE_ORDER and sources==['FileList','Walker'] else 'selected-subset'
        need(selection.get('coverage_kind')==coverage,'coverage selection mismatch')
        if coverage=='all-extended-nontruncated':need(entries==100000 and pairs==7 and len(cells)==44,'approved full ordinary scale')
    else:
        need(cells==[('W1-truncated','Walker')] and cases==['W1-truncated'] and sources==['Walker'] and entries==500001 and selection.get('coverage_kind')=='cap+1','cap+1 selection')
    need(selection.get('expected_rows')==len(cells)*pairs*2,'independent expected rows mismatch')
    return selection,cells,test

def validate_meta(meta,selection,cells):
    need(isinstance(meta,dict) and type(meta.get('schema_version')) is int and meta['schema_version']==1,'META schema')
    for f in ['runner','pairs','selected_cases','selected_sources','expected_rows']:need(meta.get(f)==selection[f],'META selection mismatch '+f)
    if selection['runner']=='extended':
        need(meta.get('entries')==selection['entries'] and meta.get('selected_source_cells')==len(cells),'META scale/cell count')
        need([key(c) for c in meta.get('supported_cells',[])]==cells and meta.get('unsupported_cells')==[],'META exact matrix/unsupported mismatch')
        need(meta.get('coverage_kind')==selection['coverage_kind'] and meta.get('native') is False,'META coverage/native')
        config=meta.get('runtime_settings',{})
        for f,v in [('search_threads',12),('search_parallel_threshold',25000),('walker_max_entries',500000)]:need(type(config.get(f)) is int and config[f]==v,'actual META setting '+f)
        need(meta.get('environment_identity',{}).get('crate_version')==selection['_tag'].removeprefix('v'),'actual META tag version')
        need(meta.get('environment_identity',{}).get('optimized') is True and meta.get('environment_identity',{}).get('frame_period_ms')==16,'actual release/frame setting')
    else:
        need(meta.get('input_entries')==500001 and meta.get('actual_cap')==500000 and meta.get('comparison_kind')=='AA-variability' and meta.get('owned_global_config_restoration') is True,'actual cap metadata')

def run_sequence(cell,pairs,entries):
    c,s=cell
    return [{'profile':c,'source':s,'condition':v,'role':'untimed-warmup','entries':entries} for v in (False,True)]+[{'profile':c,'source':s,'condition':v,'role':'sample','entries':entries,'pair':p,'position':pos} for p in range(pairs) for pos,v in enumerate((False,True) if p%2==0 else (True,False))]

def validate_rows(rows,meta,cell,selection):
    c,s=cell;pairs=selection['pairs'];need(len(rows)==2*pairs,'PASS exact rows')
    for n,r in enumerate(rows):
        need(isinstance(r,dict) and type(r.get('schema_version')) is int and r['schema_version']==1 and r.get('correct') is True and r.get('contention_eligible') is True,'typed correct/eligible row')
        need(r.get('native') is False,'native label mismatch');finite_tree(r)
        need(r.get('comparison')==c and r.get('source')==s and r.get('case') in ('B0',c),'row cell identity')
        need(type(r.get('pair')) is int and r['pair']==n//2 and type(r.get('position')) is int and r['position']==n%2,'exact ordered sample pair/position')
        expected_case=('B0',c) if (n//2)%2==0 else (c,'B0');need(r['case']==expected_case[n%2],'ordered ABBA condition')
        need(r.get('sample_entries')==selection['entries'] and type(r['sample_entries']) is int,'actual workload input')
        kind='AA-variability' if c in AA else 'AB-operation-cost'
        need(r.get('comparison_kind')==kind,'comparison semantic kind')
        measurement='worker-only-parser' if c in PARSER else 'headless-GUI-actual-workers';need(r.get('measurement_kind')==measurement,'measurement semantic kind')
        for field in PHASES:
            v=r.get(field)
            nullable=field=='worker_drained_ms' or (measurement=='worker-only-parser' and field in ('index_ready_ms','results_ready_ms','max_frame_ms')) or (c=='W1-truncated' and field in ('max_ingest_gap_ms','worker_drained_ms'))
            need(finite(v) or (nullable and v is None),'finite numeric phase '+field)
        need(r['data_publish_end_ms']<=r['terminal_publish_ms'],'producer chronology')
        if measurement=='worker-only-parser':need(r.get('index_ready_ms') is None and r.get('results_ready_ms') is None and r.get('max_frame_ms') is None,'parser null GUI phases')
    sealed_meta=dict(meta)
    if selection['runner']=='extended':sealed_meta['supported_cells']=[{'case':c,'source':s}]
    try:return validate_and_aggregate(rows,sealed_meta)
    except (AssertionError,KeyError,TypeError,ValueError) as e:raise ValueError('fbd9 typed row/victim/ABBA/chronology predicate failed: '+str(e)) from e

def collect(text,sidecar,identity):
    selection,cells,test=bind_sidecar(text,sidecar,identity);selection=dict(selection,_tag=sidecar['source_identity']['tag'])
    meta=None;cursor=0;active=None;stopped=False;statuses=[];admitted=[];excluded=[];comparisons=[];runs=0;named=0;summaries=[]
    for line in text.splitlines():
        if line.startswith('running '):
            need(line=='running 1 test','exact one discovered runner');runs+=1;continue
        if line.startswith('test result:'):summaries.append(line);continue
        prefix='test '+test+' ... '
        if line.startswith(prefix):named+=1;line=line[len(prefix):]
        elif line.startswith('test ') and ' ... ' in line:raise ValueError('unexpected selected test')
        if not line.startswith('INDEX_PERF_'):continue
        # Other real native diagnostics are retained in full log, not interpreted.
        event_type,sep,value=line.partition(' ')
        if event_type not in {'INDEX_PERF_META','INDEX_PERF_CELL_START','INDEX_PERF_RUN_START','INDEX_PERF_SAMPLE','INDEX_PERF_CELL_STATUS','INDEX_PERF_UNSUPPORTED'}:continue
        need(bool(sep),'event delimiter');event=parse_json(value);finite_tree(event)
        if event_type=='INDEX_PERF_UNSUPPORTED':raise ValueError('unexpected unsupported emitted under frozen requested matrix')
        if event_type=='INDEX_PERF_META':
            need(meta is None and active is None and cursor==0,'duplicate/out-of-order META');validate_meta(event,selection,cells);meta=event;continue
        need(meta is not None,'event before META')
        if event_type=='INDEX_PERF_CELL_START':
            need(active is None and not stopped and cursor<len(cells),'START after uncertain cleanup/interleaved/unrequested')
            need(key(event)==cells[cursor],'ordered declared START cell')
            need(set(event)=={'case','source','pairs','entries','expected_rows'},'START schema')
            need(type(event['pairs']) is int and event['pairs']==selection['pairs'] and type(event['entries']) is int and event['entries']==selection['entries'] and type(event['expected_rows']) is int and event['expected_rows']==2*selection['pairs'],'START scale')
            active={'cell':cells[cursor],'rows':[],'run_cursor':0,'pending_sample':None};continue
        if event_type=='INDEX_PERF_CELL_STATUS':
            cell=key(event);status=event.get('status');need(status in ('PASS','FAIL','NOT_RUN'),'unknown status; typed NON_ELIGIBLE unsupported')
            need(cursor<len(cells) and cell==cells[cursor],'duplicate/out-of-order STATUS')
            if status=='NOT_RUN':
                need(active is None and stopped and set(event)=={'case','source','status','reason'} and type(event['reason']) is str and bool(event['reason']),'NOT_RUN needs prior uncertainty/no START/explicit reason')
                statuses.append(event);cursor+=1;continue
            need(active is not None and active['cell']==cell,'STATUS without matching START')
            required={'case','source','status','actual_partial_rows','accepted_rows','positive_cleanup','root_restored','cleanup_evidence','panic'}
            if selection['runner']=='truncated-serial':required|={'config_equal_before','config_restoration_authorized','actual_config_before','actual_config_after'}
            need(set(event)==required,'terminal STATUS schema')
            need(integer(event['actual_partial_rows']) and event['actual_partial_rows']==len(active['rows']) and integer(event['accepted_rows']),'actual status/raw row count')
            need(type(event['positive_cleanup']) is bool and type(event['root_restored']) is bool and type(event['cleanup_evidence']) is str and bool(event['cleanup_evidence']),'literal actual cleanup/restoration attestation')
            need(event['panic'] is None or type(event['panic']) is str,'panic reason schema')
            if selection['runner']=='truncated-serial':
                need(type(event['config_equal_before']) is bool and type(event['config_restoration_authorized']) is bool,'typed config restoration flags')
                need(isinstance(event['actual_config_before'],dict) and isinstance(event['actual_config_after'],dict),'serialized actual config schema')
                need(event['config_equal_before']==(event['actual_config_before']==event['actual_config_after']),'actual config equality attestation mismatch')
                for f,v in [('search_threads',12),('search_parallel_threshold',25000),('walker_max_entries',500000)]:need(type(event['actual_config_before'].get(f)) is int and event['actual_config_before'][f]==v,'actual original cap config '+f)
            if status=='PASS':
                need(event['positive_cleanup'] is True and event['root_restored'] is True and event['panic'] is None,'PASS requires positive physical cleanup/root restoration')
                need(event['accepted_rows']==event['actual_partial_rows']==2*selection['pairs'],'PASS complete accepted row count')
                if selection['runner']=='extended':need(active['run_cursor']==2+2*selection['pairs'] and active['pending_sample'] is None,'PASS exact warmup/sample run sequence')
                else:need(event['config_equal_before'] is True and event['config_restoration_authorized'] is True,'cap PASS restoration authorization')
                comparisons+=validate_rows(active['rows'],meta,cell,selection);admitted+=active['rows']
            else:
                need(event['accepted_rows']==0,'FAIL must exclude all partial rows');excluded+=active['rows']
                need(event['panic'] is not None or not event['positive_cleanup'] or not event['root_restored'] or len(active['rows'])!=2*selection['pairs'] or (selection['runner']=='truncated-serial' and (not event['config_equal_before'] or not event['config_restoration_authorized'])),'FAIL requires real declared reason')
            stopped=not event['positive_cleanup'] or not event['root_restored']
            if selection['runner']=='truncated-serial':stopped=stopped or not event['config_restoration_authorized'] or not event['config_equal_before']
            statuses.append(event);active=None;cursor+=1;continue
        need(active is not None,'RUN/SAMPLE outside active cell')
        if event_type=='INDEX_PERF_RUN_START':
            need(selection['runner']=='extended','cap runner has no RUN_START')
            need(type(event.get('condition')) is bool and type(event.get('entries')) is int,'typed RUN_START')
            if event.get('role')=='sample':need(type(event.get('pair')) is int and type(event.get('position')) is int,'typed sample RUN_START')
            sequence=run_sequence(active['cell'],selection['pairs'],selection['entries']);n=active['run_cursor']
            need(active['pending_sample'] is None and n<len(sequence) and event==sequence[n],'exact warmup/sample RUN_START chronology')
            active['run_cursor']+=1
            if event['role']=='sample':active['pending_sample']=event
            continue
        # Failed-cell partials retain identity/count/ordering even though timing checks are not admission.
        c,s=active['cell'];n=len(active['rows']);need(n<2*selection['pairs'],'extra SAMPLE')
        need(isinstance(event,dict) and event.get('comparison')==c and event.get('source')==s and event.get('case') in ('B0',c),'SAMPLE cell binding')
        expected_case=('B0',c) if (n//2)%2==0 else (c,'B0')
        need(type(event.get('pair')) is int and event['pair']==n//2 and type(event.get('position')) is int and event['position']==n%2 and event['case']==expected_case[n%2] and event.get('order')==('AB' if (n//2)%2==0 else 'BA'),'SAMPLE exact ABBA chronological identity')
        if selection['runner']=='extended':
            pending=active['pending_sample'];need(pending is not None and pending['pair']==event['pair'] and pending['position']==event['position'] and pending['condition']==(event['case']!='B0'),'SAMPLE must follow actual sample RUN_START');active['pending_sample']=None
        active['rows'].append(event)
    need(runs==named==1 and meta is not None,'exact one actual runner invocation/META')
    need(active is None and cursor==len(cells),'missing attempted status/cells')
    need(len(summaries)==1,'exact genuine libtest trailer')
    trailer=summaries[0];need(re.fullmatch(r'test result: (?:ok|FAILED)\. \d+ passed; \d+ failed; \d+ ignored; 0 measured; \d+ filtered out; finished in \d+(?:\.\d+)?s',trailer),'genuine exact summary schema');exit_code=sidecar['exit_code'];failed=any(s['status']!='PASS' for s in statuses)
    if exit_code==0:need(re.match(r'^test result: ok\. 1 passed; 0 failed; 0 ignored;',trailer) and not failed,'successful process/trailer must match all PASS')
    else:need(re.match(r'^test result: FAILED\. 0 passed; 1 failed; 0 ignored;',trailer) and failed,'failed process/trailer needs accounted-for failed cell; all-PASS outer failure invalid')
    counts={name:sum(s['status']==name for s in statuses) for name in ('PASS','FAIL','NOT_RUN')}
    return {'schema_version':1,'evidence_kind':sidecar['evidence_kind'],'protocol_validation':'PASS','benchmark_matrix_outcome':'ALL_REQUESTED_CELLS_PASSED' if not failed else 'MIXED_OR_FAILED; excluded cells cannot support speed comparisons','raw_exit_code':exit_code,'raw_libtest_trailer':trailer,'process_outcome':'FAILED' if failed else 'PASSED','source_identity':sidecar['source_identity'],'validator_sha256':sidecar['validator_sha256'],'frozen_row_predicates_sha256':EXPECTED_COLLECTOR,'metadata':meta,'selection':sidecar['selection'],'statuses':statuses,'status_counts':counts,'raw_rows':len(admitted)+len(excluded),'accepted_rows':len(admitted),'excluded_partial_rows':len(excluded),'admitted_cell_keys':[[s['case'],s['source']] for s in statuses if s['status']=='PASS'],'accepted_raw_rows':admitted,'excluded_raw_rows':excluded,'comparisons':comparisons,'full_log_sha256':sidecar['log_sha256'],'classification_caveat':'Generic observed FAIL retained. NON_ELIGIBLE/unsupported never inferred from panic strings. Cleanup Debug string remains diagnostic only; boolean gates bind to accepted exact source/guard checkpoint.'}
def main():
    p=argparse.ArgumentParser();p.add_argument('log',type=Path);p.add_argument('--sidecar',type=Path,required=True);p.add_argument('--identity-packet',type=Path,required=True);p.add_argument('--output',type=Path,required=True);a=p.parse_args()
    try:
        need(sha(a.identity_packet)==EXPECTED_GUARD_CHECKPOINT,'raw identity checkpoint SHA')
        out=collect(a.log.read_bytes().decode(),parse_json(a.sidecar.read_text()),parse_json(a.identity_packet.read_text()))
    except (ValueError,KeyError,TypeError) as e:
        print('REJECT evidence protocol:',e);raise SystemExit(2)
    out.update(full_log_path=str(a.log.resolve()),sidecar_path=str(a.sidecar.resolve()),sidecar_sha256=sha(a.sidecar),identity_packet_path=str(a.identity_packet.resolve()),identity_packet_sha256=sha(a.identity_packet))
    a.output.write_text(json.dumps(out,indent=2,allow_nan=False)+'\n');print('Validated historical evidence:',out['status_counts'],'accepted rows',out['accepted_rows'],'excluded partial rows',out['excluded_partial_rows'],'original exit',out['raw_exit_code'])
if __name__=='__main__':main()
