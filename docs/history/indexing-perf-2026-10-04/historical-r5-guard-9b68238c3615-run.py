from pathlib import Path
import datetime,hashlib,json,os,re,signal,subprocess,tempfile,time
B=Path(__file__).resolve().parent;S=B.parent;P=S/'newtag-repair-r5';packet=json.loads((P/'preparation-packet.json').read_text());records=[]
sha=lambda p:hashlib.sha256(Path(p).read_bytes()).hexdigest()
def inventory():return sorted(str(p) for p in Path(tempfile.gettempdir()).iterdir() if p.name.startswith(('flistwalker-historical-','fff-rs-app-historical-','fff-rs-app-indexing-perf')))
def source(root):return {str(p.relative_to(root/'src')):sha(p) for p in (root/'src').rglob('*') if p.is_file()}
def manifest(m):return hashlib.sha256(json.dumps(m,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def utc():return datetime.datetime.now(datetime.timezone.utc).isoformat()
def save(p,x):p.write_text(json.dumps(x,indent=2)+'\n')
def run(stage,root,target,selector,mode,expected_count=1,expected_exit=0):
 log=B/(stage+'.log');side=B/(stage+'.json');assert not log.exists() and not side.exists()
 cmd=['cargo','+1.97.1','test','--locked','--offline','--lib',selector]
 cmd+=['--no-run'] if mode=='build' else ['--','--list','--exact'] if mode=='list-exact' else ['--','--list'] if mode=='list-pattern' else ['--','--exact','--nocapture','--test-threads=1'] if mode=='exact' else ['--','--nocapture','--test-threads=1']
 explicit={'FLISTWALKER_SEARCH_THREADS':'12','FLISTWALKER_SEARCH_PARALLEL_THRESHOLD':'25000','FLISTWALKER_WALKER_MAX_ENTRIES':'500000','CARGO_TARGET_DIR':str(target)};env=os.environ.copy();env.update(explicit)
 before=source(root);lock=sha(root/'Cargo.lock');cargo=sha(root/'Cargo.toml');roots=inventory();bound=1200 if mode=='build' else 120 if mode.startswith('list') else 600
 x={'stage':stage,'cwd':str(root),'command':cmd,'env':explicit,'phase':mode,'status':'RUNNING','started_utc':utc(),'source_manifest_before':manifest(before),'Cargo_lock_before':lock,'Cargo_toml_before':cargo,'owned_guard_roots_before':roots,'watchdog_seconds':bound,'watchdog_scope':'outer owned cargo process; native deadlines unchanged','expected_count':expected_count,'expected_exit':expected_exit,'script_sha256':sha(__file__)};save(side,x);start=time.monotonic();expired=False;signals=[]
 with log.open('w') as f:
  p=subprocess.Popen(cmd,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT,start_new_session=True);x['actual_process_pid']=p.pid;x['owned_process_group']=p.pid;save(side,x);print(json.dumps({'stage':stage,'phase':mode,'pid':p.pid,'utc':utc(),'bound_seconds':bound}),flush=True)
  while True:
   try:code=p.wait(timeout=max(.001,min(45,bound-(time.monotonic()-start))));break
   except subprocess.TimeoutExpired:
    print(json.dumps({'stage':stage,'phase':mode,'elapsed_seconds':round(time.monotonic()-start,2),'utc':utc(),'log_bytes':log.stat().st_size,'actual_pass_lines':len(re.findall(r'^test .* \.\.\. ok$',log.read_text(),re.M))}),flush=True)
    if time.monotonic()-start<bound:continue
    expired=True
    if p.poll() is None:os.killpg(p.pid,signal.SIGTERM);signals.append('owned SIGTERM')
    try:code=p.wait(timeout=5)
    except subprocess.TimeoutExpired:
     if p.poll() is None:os.killpg(p.pid,signal.SIGKILL);signals.append('owned SIGKILL')
     code=p.wait(timeout=5)
    break
 try:os.killpg(p.pid,0);absent=False
 except ProcessLookupError:absent=True
 except PermissionError:absent=False
 text=log.read_text();after=source(root);root_after=inventory();summaries=re.findall(r'^test result:.*$',text,re.M);discovered=re.findall(r'^(\S+): test$',text,re.M);newroots=sorted(set(root_after)-set(roots))
 x.update(status='COMPLETED',exit_code=code,ended_utc=utc(),elapsed_seconds=time.monotonic()-start,actual_wait_returned=p.returncode is not None,owned_process_group_absent=absent,watchdog_expired=expired,watchdog_signals=signals,source_manifest_after=manifest(after),Cargo_lock_after=sha(root/'Cargo.lock'),Cargo_toml_after=sha(root/'Cargo.toml'),owned_guard_roots_after=root_after,possible_new_owned_roots_retained=newroots,log_sha256=sha(log),test_result_summaries=summaries,discovered_tests=discovered,first_errors=re.findall(r'^error(?:\[[^\]]+\])?:.*$',text,re.M)[:8],warnings=re.findall(r'^warning:.*$',text,re.M),continuation_authorized=not expired and absent and p.returncode is not None and before==after and lock==sha(root/'Cargo.lock') and cargo==sha(root/'Cargo.toml') and not newroots)
 save(side,x);records.append(x);print(json.dumps({k:x[k] for k in ['stage','exit_code','elapsed_seconds','test_result_summaries','first_errors','warnings','continuation_authorized']}),flush=True)
 assert x['continuation_authorized'],'cleanup/process/source uncertainty; STOP; roots untouched'
 assert code==expected_exit,'unexpected actual exit; STOP'
 assert not x['warnings'],'unexpected warning; STOP'
 if mode.startswith('list'):assert len(discovered)==expected_count,'discovery cardinality mismatch; STOP'
 elif mode!='build':
  wanted=f'test result: '+('FAILED. 0 passed; 1 failed; 0 ignored;' if expected_exit==101 else f'ok. {expected_count} passed; 0 failed; 0 ignored;')
  assert len(summaries)==1 and summaries[0].startswith(wanted),'unexpected actual libtest cardinality; STOP'
 return x
failure=None
try:
 for t in packet['tags']:
  assert source(Path(t['export'])/'rust')==json.loads(Path(t['source_map']['path']).read_text())
 red=packet['first_red_candidate'];root=Path(red['export'])/'rust';target=P/'target-normal-first-red-v0.27.0'
 names=[x['test'] for x in red['commands']]
 run('r5-red-build',root,target,names[0],'build',0)
 for n,name in enumerate(names):run(f'r5-red-{n}-discovery',root,target,name,'list-exact');run(f'r5-red-{n}-exact',root,target,name,'exact',1,101)
 for tag in ['v0.27.0','v0.28.0','v0.29.0','v0.30.0']:
  root=P/tag/'rust';target=P/('target-normal-'+tag);expected=20 if tag!='v0.30.0' else 19
  run(tag+'-build',root,target,'historical_','build',0)
  if tag=='v0.27.0':
   for n,name in enumerate(names):run(f'r5-green-{n}-discovery',root,target,name,'list-exact');run(f'r5-green-{n}-exact',root,target,name,'exact')
  for filt,count in [('historical_',expected),('fixture::',8),('oracle::',7)]:
   label=filt.replace('::','').strip('_');run(tag+'-'+label+'-discovery',root,target,filt,'list-pattern',count);run(tag+'-'+label+'-guards',root,target,filt,'pattern',count)
except Exception as e:failure=repr(e);print(json.dumps({'STOP':failure,'utc':utc(),'roots':'untouched; no retry or next stage'}),flush=True)
finally:
 save(B/'execution-summary.json',{'source_preparation_packet_sha256':sha(P/'preparation-packet.json'),'status':'STOP for main readback, no performance','failure':failure,'records':[{'stage':r['stage'],'exit_code':r['exit_code'],'actual_wait_returned':r['actual_wait_returned'],'owned_process_group_absent':r['owned_process_group_absent'],'source_manifest_before':r['source_manifest_before'],'source_manifest_after':r['source_manifest_after'],'summary':r['test_result_summaries'],'discovered_count':len(r['discovered_tests']),'continuation_authorized':r['continuation_authorized']} for r in records],'v27_precleanup_Flush':'unobserved; final owned cleanup Flush only','performance':'NOT_RUN'})
if failure:raise SystemExit(2)
