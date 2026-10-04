"""Authorized first-tag-only serial historical run; preserves actual failed outcomes."""
import datetime,hashlib,json,os,re,signal,subprocess,sys,tempfile,time
from pathlib import Path
if not __debug__:raise SystemExit('wrapper refuses optimized Python')
B=Path(__file__).resolve().parent;S=B.parent;TAG='v0.27.0';ROOT=S/'newtag-repair-r5'/TAG/'rust';V=S/'historical-validator-r3';sys.path.insert(0,str(V));import collect_historical as validator
GUARD=S/'newtag-r5-guards-execution/guard-execution-packet.json';G=json.loads(GUARD.read_text());T=next(t for t in G['tags'] if t['tag']==TAG);TARGET=validator.dedicated_release_targets(G)[TAG]
ENV={'FLISTWALKER_SEARCH_THREADS':'12','FLISTWALKER_SEARCH_PARALLEL_THRESHOLD':'25000','FLISTWALKER_WALKER_MAX_ENTRIES':'500000','FW_INDEX_PERF_EXTRA_PAIRS':'7','FW_INDEX_PERF_EXTRA_ENTRIES':'100000','FW_INDEX_PERF_EXTRA_CASES':','.join(validator.PROFILE_ORDER),'FW_INDEX_PERF_EXTRA_SOURCES':'FileList,Walker','CARGO_TARGET_DIR':TARGET}
ORD=validator.ORDINARY_TEST;CAP=validator.TRUNC_TEST;RESULTS=[]
def now():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def sha(p):return hashlib.sha256(Path(p).read_bytes()).hexdigest()
def source():return {str(p.relative_to(ROOT/'src')):sha(p) for p in (ROOT/'src').rglob('*') if p.is_file()}
def source_identity():return {'tag':TAG,'original_commit':T['original_commit'],'original_archive_sha256':T['original_archive_sha256'],'Cargo_lock_sha256':T['Cargo_lock_sha256'],'source_manifest_sha256':T['source_manifest_sha256'],'adapter_sha256':T['full_patch']['sha256']}
def save(path,data):path.write_text(json.dumps(data,indent=2,allow_nan=False)+'\n')
def inventory():
    base=Path(tempfile.gettempdir());return sorted(str(p) for p in base.iterdir() if p.name.startswith(('fff-rs-app-indexing-perf','fff-rs-app-historical','flistwalker-historical')))
def group_absent(pid):
    try:os.killpg(pid,0);return False
    except ProcessLookupError:return True
    except PermissionError:return False

def command(test,mode):
    c=['cargo','+1.97.1','test','--release','--locked','--offline','--lib',test]
    return c+(['--no-run'] if mode=='build' else ['--','--ignored','--list','--exact'] if mode=='discover' else ['--','--ignored','--exact','--nocapture','--test-threads=1'])

def execute(stage,test,mode,bound):
    log=B/(stage+'.log');side=B/(stage+'.json')
    if log.exists() or side.exists():raise RuntimeError('no retry/overwrite authorized: '+stage)
    before=source();lock=sha(ROOT/'Cargo.lock');original_cargo=sha(ROOT/'Cargo.toml');base_inventory=inventory()
    if before!=json.loads(Path(T['source_map']['path']).read_text()) or lock!=T['Cargo_lock_sha256']:raise RuntimeError('source/lock pre-readback mismatch')
    if sha(V/'collect_historical.py')!='6cc7d3a8fa88a0d8c98dd8fffb67a28628be5be2197284681b31d9c11f035e3e':raise RuntimeError('frozen validator mismatch')
    env=os.environ.copy();env.update(ENV);cmd=command(test,mode)
    x={'stage':stage,'tag':TAG,'phase':mode,'status':'RUNNING','cwd':str(ROOT),'command':cmd,'env':ENV,'watchdog_seconds':bound,'watchdog_scope':'outer owned process bound only; native 30s progress/120s per-run/joins unchanged','started_utc':now(),'source_manifest_before':T['source_manifest_sha256'],'source_map_before':before,'Cargo_lock_before':lock,'Cargo_toml_before':original_cargo,'temp_base':tempfile.gettempdir(),'possible_test_roots_before':base_inventory,'script_sha256':sha(__file__)}
    save(side,x);start=time.monotonic();stats={'CELL_START':0,'CELL_STATUS':0,'SAMPLE':0};last=None;offset=0;carry=b'';events=[]
    def progress():
        nonlocal offset,carry,last
        with log.open('rb') as f:f.seek(offset);chunk=f.read();offset=f.tell()
        chunks=(carry+chunk).split(b'\n');carry=chunks.pop()
        for raw in chunks:
            line=raw.decode(errors='replace')
            for kind in stats:
                prefix='INDEX_PERF_'+kind+' '
                if line.startswith(prefix):
                    stats[kind]+=1
                    if kind=='CELL_STATUS':
                        try:e=json.loads(line[len(prefix):]);last={k:e.get(k) for k in ['case','source','status','positive_cleanup','root_restored','actual_partial_rows','accepted_rows']};events.append(e)
                        except json.JSONDecodeError:last={'malformed_status':line[:200]}
        print(json.dumps({'tag':TAG,'phase':stage,'elapsed_seconds':round(time.monotonic()-start,2),'utc':now(),'actual_counters':stats,'latest_actual_status':last}),flush=True)
    expired=False;signals=[];wait_exception=None
    with log.open('w') as output:
        p=subprocess.Popen(cmd,cwd=ROOT,env=env,stdout=output,stderr=subprocess.STDOUT,start_new_session=True)
        x['actual_pid']=p.pid;x['owned_process_group']=p.pid;save(side,x)
        print(json.dumps({'tag':TAG,'phase':stage,'started_pid':p.pid,'watchdog_seconds':bound,'utc':now()}),flush=True)
        while True:
            remaining=bound-(time.monotonic()-start)
            try:code=p.wait(timeout=max(0.001,min(45,remaining)));break
            except subprocess.TimeoutExpired:
                progress()
                if time.monotonic()-start<bound:continue
                expired=True
                # Signal only while original root process remains alive/owned.
                if p.poll() is None:
                    try:os.killpg(p.pid,signal.SIGTERM);signals.append('owned-group SIGTERM')
                    except ProcessLookupError:signals.append('group already absent before TERM')
                try:code=p.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    if p.poll() is None:
                        try:os.killpg(p.pid,signal.SIGKILL);signals.append('owned-group SIGKILL')
                        except ProcessLookupError:signals.append('group already absent before KILL')
                    try:code=p.wait(timeout=5)
                    except subprocess.TimeoutExpired:code=None;wait_exception='root physical reap unproven'
                break
    progress();after=source();roots=inventory();absent=group_absent(p.pid)
    text=log.read_bytes().decode(errors='replace');retained=[]
    for line in text.splitlines():
        if line.startswith(('INDEX_PERF_ROOT_RETAINED ','INDEX_PERF_SETTINGS_RETAINED ','INDEX_PERF_STARTUP_ROOT_RETAINED ','INDEX_PERF_CONFIG_RETAINED')):retained.append(line)
    x.update(status='COMPLETED' if code is not None else 'UNKNOWN',ended_utc=now(),elapsed_seconds=time.monotonic()-start,exit_code=code,actual_wait_returned=p.returncode is not None,owned_process_group_absent=absent,watchdog_expired=expired,watchdog_signals=signals,wait_exception=wait_exception,source_map_after=after,Cargo_lock_after=sha(ROOT/'Cargo.lock'),Cargo_toml_after=sha(ROOT/'Cargo.toml'),source_and_lock_readback_unchanged=before==after and lock==sha(ROOT/'Cargo.lock') and original_cargo==sha(ROOT/'Cargo.toml'),log_sha256=sha(log),actual_counters=stats,actual_statuses=events,possible_test_roots_after=roots,possible_new_test_root_candidates_retained=sorted(set(roots)-set(base_inventory)),actual_retention_log_records=retained,libtest_summaries=re.findall(r'^test result:.*$',text,re.M),first_compile_errors=re.findall(r'^error(?:\[[^\]]+\])?:.*$',text,re.M)[:8],settings_saved_content_readback='NOT_RUN: normal owned settings directories may already be deleted after physical writer cleanup. Actual META runtime config, cap STATUS configs and remaining directory presence are separate observed evidence.')
    save(side,x);RESULTS.append(x)
    if expired or code is None or not absent or not x['source_and_lock_readback_unchanged']:raise RuntimeError(stage+': timeout/physical process/source identity unknown; STOP, roots untouched')
    if mode=='build' and code!=0:raise RuntimeError(stage+': real compile failure retained; STOP, no retry')
    if mode=='discover':
        discovered=re.findall(r'^(\S+): test$',text,re.M);x['exact_discovered_tests']=discovered;save(side,x)
        if code!=0 or discovered!=[test]:raise RuntimeError(stage+': compile/discovery failure or zero matching test; STOP')
    return x,log

def measure(stage,test,bound,truncated=False):
    x,log=execute(stage,test,'measure',bound)
    cells=[('W1-truncated','Walker')] if truncated else validator.selected_cells(validator.PROFILE_ORDER,['FileList','Walker'])
    selection={'runner':'truncated-serial' if truncated else 'extended','cells':[{'case':c,'source':s} for c,s in cells],'selected_cases':['W1-truncated'] if truncated else validator.PROFILE_ORDER,'selected_sources':['Walker'] if truncated else ['FileList','Walker'],'pairs':7,'entries':500001 if truncated else 100000,'expected_rows':14 if truncated else 616,'coverage_kind':'cap+1' if truncated else 'all-extended-nontruncated','cap':500000}
    side={'schema_version':1,'guard_checkpoint_sha256':validator.EXPECTED_GUARD_CHECKPOINT,'evidence_kind':'live-historical','log_sha256':x['log_sha256'],'exit_code':x['exit_code'],'process_completed':x['status']=='COMPLETED','actual_wait_returned':x['actual_wait_returned'],'owned_process_group_absent':x['owned_process_group_absent'],'watchdog_expired':x['watchdog_expired'],'runner_test':test,'cwd':str(ROOT),'command':x['command'],'env':ENV,'selection':selection,'source_identity':source_identity(),'validator_sha256':sha(V/'collect_historical.py')}
    meta=B/(stage+'-collector-sidecar.json');save(meta,side)
    try:
        result=validator.collect(log.read_bytes().decode(),side,G)
        result.update(full_log_path=str(log),sidecar_path=str(meta),sidecar_sha256=sha(meta),identity_packet_path=str(GUARD),identity_packet_sha256=sha(GUARD));save(B/(stage+'-validated.json'),result)
    except Exception as e:
        save(B/(stage+'-validator-rejection.json'),{'error':repr(e),'log_sha256':sha(log),'actual_process_exit':x['exit_code'],'accepted_rows':0,'classification':'protocol rejection; no inferred unsupported/noneligible'});raise RuntimeError(stage+': validator rejection; STOP') from e
    # Positive per-cell product joins/restoration are not inferred from process exit.
    positive=all(e.get('status') in ('PASS','FAIL') and e.get('positive_cleanup') is True and e.get('root_restored') is True for e in result['statuses'])
    if truncated:positive=positive and all(e.get('config_restoration_authorized') is True and e.get('config_equal_before') is True and e.get('actual_config_before')==e.get('actual_config_after') for e in result['statuses'])
    x['validator_protocol']='ACCEPTED';x['accepted_rows']=result['accepted_rows'];x['excluded_partial_rows']=result['excluded_partial_rows'];x['actual_status_counts']=result['status_counts'];x['positive_product_cleanup_restoration_for_next_phase']=positive and not x['actual_retention_log_records'] and not x['possible_new_test_root_candidates_retained'];save(B/(stage+'.json'),x)
    print(json.dumps({'tag':TAG,'phase':stage,'actual_exit':x['exit_code'],'status_counts':result['status_counts'],'accepted_rows':result['accepted_rows'],'excluded_partial_rows':result['excluded_partial_rows'],'next_phase_cleanup_authorized':x['positive_product_cleanup_restoration_for_next_phase']}),flush=True)
    if not x['positive_product_cleanup_restoration_for_next_phase']:raise RuntimeError(stage+': physical cleanup/root/config/retention unknown; next phase NOT_RUN')
    return result

failure=None;cap='NOT_RUN'
try:
    execute('v27-release-build',ORD,'build',1200)
    execute('v27-ordinary-discovery',ORD,'discover',120)
    ordinary=measure('v27-ordinary-full',ORD,10800)
    execute('v27-cap-discovery',CAP,'discover',120)
    cap='RUNNING';measure('v27-cap-full',CAP,3600,True);cap='COMPLETED'
except Exception as e:failure=repr(e);print(json.dumps({'tag':TAG,'STOP':failure,'cap_status':cap,'roots':'no wrapper deletion/restoration performed'}),flush=True)
finally:
    refs=[{'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size} for p in B.iterdir() if p.is_file() and p.name!='v27-final-execution-packet.json']
    save(B/'v27-final-execution-packet.json',{'tag':TAG,'status':'STOP for main actual readback; v28 not authorized','failure':failure,'cap_status':cap,'phases':[{k:x.get(k) for k in ['stage','exit_code','actual_wait_returned','owned_process_group_absent','watchdog_expired','source_and_lock_readback_unchanged','actual_counters','actual_status_counts','positive_product_cleanup_restoration_for_next_phase']} for x in RESULTS],'source_identity':source_identity(),'guard_packet_sha256':sha(GUARD),'validator_packet_sha256':sha(V/'r3-validator-packet.json'),'script_sha256':sha(__file__),'v27_original_precleanup_Flush':'unobserved; proof covers added final owned cleanup Flush only','references':refs,'no_cross_tag_continuation':True})
raise SystemExit(2 if failure else 0)
