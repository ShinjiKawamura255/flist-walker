from pathlib import Path
import json,subprocess,sys,os,time,hashlib,datetime,tempfile,re
stage,export,target,selector,mode=sys.argv[1:]
B=Path(__file__).resolve().parent;root=Path(export);log=B/(stage+'.log');side=B/(stage+'.json')
assert not log.exists(),stage
cmd=['cargo','+1.97.1','test','--locked','--offline','--lib',selector,'--']
cmd+=['--list','--exact'] if mode=='list-exact' else ['--list'] if mode=='list-pattern' else ['--exact','--nocapture','--test-threads=1'] if mode=='exact' else ['--nocapture','--test-threads=1']
env=os.environ.copy();env['CARGO_TARGET_DIR']=target
explicit={'FLISTWALKER_SEARCH_THREADS':'12','FLISTWALKER_SEARCH_PARALLEL_THRESHOLD':'25000','FLISTWALKER_WALKER_MAX_ENTRIES':'500000'};env.update(explicit)
def inventory():
 return sorted(str(p) for p in Path(tempfile.gettempdir()).iterdir() if p.name.startswith(('flistwalker-historical-','fff-rs-app-historical-')))
def sources():
 src=root/'src';m={str(p.relative_to(src)):hashlib.sha256(p.read_bytes()).hexdigest() for p in src.rglob('*') if p.is_file()}
 return hashlib.sha256(json.dumps(m,sort_keys=True,separators=(',',':')).encode()).hexdigest()
meta={'stage':stage,'cwd':str(root),'command':cmd,'env':explicit|{'CARGO_TARGET_DIR':target},'started_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'source_manifest_before':sources(),'Cargo_lock_before':hashlib.sha256((root/'Cargo.lock').read_bytes()).hexdigest(),'owned_guard_roots_before':inventory(),'scope':'compile/discovery/normal guards only; no perf','status':'running'}
side.write_text(json.dumps(meta,indent=2)+'\n');start=time.monotonic()
with log.open('w') as f:p=subprocess.Popen(cmd,cwd=root,env=env,stdout=f,stderr=subprocess.STDOUT);code=p.wait()
text=log.read_text();meta.update({'status':'completed','exit_code':code,'ended_utc':datetime.datetime.now(datetime.timezone.utc).isoformat(),'elapsed_seconds':time.monotonic()-start,'log_sha256':hashlib.sha256(log.read_bytes()).hexdigest(),'source_manifest_after':sources(),'Cargo_lock_after':hashlib.sha256((root/'Cargo.lock').read_bytes()).hexdigest(),'owned_guard_roots_after':inventory(),'test_result_summaries':re.findall(r'^test result:.*$',text,re.M),'discovered_tests':re.findall(r'^(\S+): test$',text,re.M),'first_errors':re.findall(r'^error(?:\[[^\]]+\])?:.*$',text,re.M)[:8]})
side.write_text(json.dumps(meta,indent=2)+'\n');print(json.dumps({k:meta[k] for k in ['stage','exit_code','elapsed_seconds','test_result_summaries','first_errors']}))
