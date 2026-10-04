from pathlib import Path
import json,hashlib,tarfile,difflib,tempfile,subprocess,re,datetime
B=Path(__file__).resolve().parent;S=B.parent;E=S/'newtag-r1-guards-execution'
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def ref(p):return {'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size}
def map_src(src):return {str(p.relative_to(src)):sha(p) for p in src.rglob('*') if p.is_file()}
def compact(m):return hashlib.sha256(json.dumps(m,sort_keys=True,separators=(',',':')).encode()).hexdigest()
def patch(old,new):
 return ''.join(''.join(difflib.unified_diff(old.get(k,'').splitlines(True),new.get(k,'').splitlines(True),fromfile='a/rust/src/'+k,tofile='b/rust/src/'+k)) for k in sorted(set(old)|set(new)))
p1=json.loads((S/'newtag-repair-r1/repair-preparation-packet.json').read_text());p0=json.loads((S/'newtag-expanded/newtag-preparation-packet.json').read_text())
for packet in [p1,p0]:
 for path,v in packet['preparation_files'].items():assert sha(Path(path))==v['sha256'],path
for native in p1['tags']:
 source_map=json.loads(Path(native['source_map']['path']).read_text());assert map_src(S/'newtag-repair-r1'/native['tag']/'rust/src')==source_map
red=p1['first_red_candidate'];assert map_src(Path(red['export'])/'rust/src')==json.loads(Path(red['source_map']['path']).read_text())
for path,h in p0['frozen_basic_retained_assets_read_only'].items():assert sha(Path(path))==h,path
current=json.loads((S/'stale-full-final-source-hashes.json').read_text())['all_rust_source'];assert len(current)==213
for path,h in current.items():assert sha(S.parents[1]/path)==h,path
assert sha(S/'summarize_extensions.py')=='fbd9bdf4f5b02d08857033a0309bec3775c4f9f605bdc65492a389bd748aa0aa'
proof=[];refs=[]
for native in p1['tags']:
 tag=native['tag'];root=B/tag;src=root/'rust/src';m=map_src(src);sm=B/(tag+'-final-source-hashes.json');assert not sm.exists() or json.loads(sm.read_text())==m;sm.write_text(json.dumps(m,indent=2,sort_keys=True)+'\n');refs.append(ref(sm))
 new={k:(src/k).read_bytes().decode() for k in m}
 arc=S/'newtag-expanded'/(tag+'-original.tar')
 with tarfile.open(arc) as t:
  old={n[len('rust/src/'):]:t.extractfile(n).read().decode() for n in t.getnames() if n.startswith('rust/src/') and t.getmember(n).isfile()}
  cargo_equal=all(t.extractfile('rust/'+n).read()==(root/'rust'/n).read_bytes() for n in ['Cargo.toml','Cargo.lock']);assert cargo_equal
 rt='app/worker/runtime.rs';summary=lambda txt:re.search(r'pub\(in crate::app\) struct WorkerJoinSummary \{[^}]*\}',txt).group()
 assert summary(old[rt])==summary(new[rt])
 full=B/(tag+'-final-full.patch');assert not full.exists();full.write_text(patch(old,new));refs.append(ref(full))
 basic=S/'newtag-expanded'/(tag+'-basic-source');add=B/(tag+'-final-additive-basic.patch');add.write_text(patch({str(p.relative_to(basic)):p.read_bytes().decode() for p in basic.rglob('*') if p.is_file()},new));refs.append(ref(add))
 r1=S/'newtag-repair-r1'/tag/'rust/src';parent=B/(tag+'-final-additive-r1.patch');parent.write_text(patch({str(p.relative_to(r1)):p.read_bytes().decode() for p in r1.rglob('*') if p.is_file()},new));refs.append(ref(parent))
 with tempfile.TemporaryDirectory(prefix=tag+'-patch-replay-',dir=B) as tmp:
  tmp=Path(tmp)
  for k,v in old.items():p=tmp/'rust/src'/k;p.parent.mkdir(parents=True,exist_ok=True);p.write_text(v)
  r=subprocess.run(['patch','--no-backup-if-mismatch','-p1','-i',str(full)],cwd=tmp,text=True,capture_output=True);assert r.returncode==0,(tag,r.stderr);assert map_src(tmp/'rust/src')==m
  replay={'exit_code':r.returncode,'exact_source_map_matches':True,'output':r.stdout,'stderr':r.stderr}
 short=tag.replace('0.','').replace('.0','');stages=['r4-'+short+'-historical-discovery','r4-'+short+'-historical-guards']+[f'r4-{tag}-{name}-{kind}' for name in ['fixture','oracle'] for kind in ['discovery','guards']]
 results=[]
 for stage in stages:
  d=json.loads((E/(stage+'.json')).read_text());assert d['exit_code']==0 and d['continuation_authorized'];assert d['source_manifest_before']==d['source_manifest_after']==compact(m);assert d['Cargo_lock_before']==d['Cargo_lock_after']==native['lock_sha256'];assert not re.findall(r'^warning:',(E/(stage+'.log')).read_text(),re.M)
  expected=16 if 'historical' in stage else 8 if 'fixture' in stage else 7
  if stage.endswith('discovery'):assert len(d['discovered_tests'])==expected
  else:assert len(d['test_result_summaries'])==1 and re.match(r'test result: ok\. '+str(expected)+r' passed; 0 failed; 0 ignored;',d['test_result_summaries'][0])
  results.append({'stage':stage,'exit_code':0,'actual_summary':d['test_result_summaries'],'actual_discovered':len(d['discovered_tests']),'watchdog_expired':False,'actual_wait_returned':d['actual_wait_returned'],'owned_process_group_absent':d['owned_process_group_absent'],'source_manifest_sha256':d['source_manifest_after'],'owned_guard_roots_before':d['owned_guard_roots_before'],'owned_guard_roots_after':d['owned_guard_roots_after']});refs.extend([ref(E/(stage+'.json')),ref(E/(stage+'.log'))])
 proof.append({'tag':tag,'export':str(root),'original_commit':native['original_commit'],'original_archive_sha256':sha(arc),'immutable_basic_patch_sha256':native['immutable_basic_patch_sha256'],'Cargo_and_lock_original_bytes':cargo_equal,'Cargo_lock_sha256':sha(root/'rust/Cargo.lock'),'native_summary_original_bytes':True,'source_subjects':len(m),'source_manifest_sha256':compact(m),'source_map':ref(sm),'full_patch':ref(full),'additive_basic_patch':ref(add),'additive_r1_patch':ref(parent),'full_patch_replay':replay,'guards':results})
old_stages=['first-red-discovery','first-red-exact','r1-v30-green-discovery','r2-v29-historical-discovery','r3-v27-historical-discovery']
for stage in old_stages:refs.extend([ref(E/(stage+'.json')),ref(E/(stage+'.log'))])
for p in E.iterdir():
 if p.is_file() and p.suffix in ['.py','.json','.log','.txt']:refs.append(ref(p))
for checkpoint in ['newtag-repair-r2','newtag-repair-r3']:
 for p in (S/checkpoint).iterdir():
  if p.is_file():refs.append(ref(p))
for p in B.iterdir():
 if p.is_file() and p.name!='guard-execution-packet.json' and not any(x['path']==str(p) for x in refs):refs.append(ref(p))
refs=list({x['path']:x for x in refs}.values())
p={'status':'R4 compile/discovery/controlled guards COMPLETE; source FROZEN for focused compatibility review; NO historical performance run','frozen_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'tags':proof,'aggregate_guards':{'historical':64,'fixture':32,'oracle':28,'failed':0,'ignored':0,'warnings':0},'first_red':{'discovery_exact':1,'actual_test_exit':101,'actual_test_result':'0 passed;1 failed','classification':'meaningful original startup root-ownership assertion after caught panic; immutable snippet retained'},'compile_failures':[{'stage':'r1-v30-green-discovery','errors':14},{'stage':'r2-v29-historical-discovery','errors':3},{'stage':'r3-v27-historical-discovery','errors':5}],'previous_evidence':'All previous exact-source results retained at their point in time; not retroactively reassigned watchdog coverage. R4 v28/v29/v30 copies have identical source bytes to preceding checkpoints but were executed again with final live R4 identity.','current_213_sha256':hashlib.sha256(json.dumps(current,sort_keys=True).encode()).hexdigest(),'current_213_unchanged':True,'collector_sha256':sha(S/'summarize_extensions.py'),'basic_11_assets_unchanged':len(p0['frozen_basic_retained_assets_read_only']),'R1_preparation_40_refs_unchanged':len(p1['preparation_files']),'v27_Flush_caveat':'Original pre-cleanup ignored Flush outcome remains unobserved; positive proof covers added final owned cleanup Flush only.','process_wrapper_proof':'Bounded cargo-owned process group and actual root process wait; not evidence of product SIG shutdown. Watchdog expiry would STOP with uncertain owned roots retained; no timeout occurred.','normal_guard_coverage':'16 historical plus fixture8/oracle7 pertag. Full allprofile smoke with historical correctness/eligibility behavior differences deferred to explicit matrix; not skipped or run/claimed PASS.','performance':'NOT_RUN; no samples or classifications fabricated','source_frozen':True,'references':refs}
q=B/'guard-execution-packet.json';assert not q.exists();q.write_text(json.dumps(p,indent=2)+'\n');print(json.dumps({'packet':str(q),'sha256':sha(q),'references':len(refs),'sources':{x['tag']:x['source_manifest_sha256'] for x in proof},'guards':p['aggregate_guards']}))
