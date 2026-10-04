from pathlib import Path
import hashlib,json,datetime
B=Path(__file__).resolve().parent;S=B.parent
def sha(p):return hashlib.sha256(p.read_bytes()).hexdigest()
def ref(p):return {'path':str(p),'sha256':sha(p),'bytes':p.stat().st_size}
controls=json.loads((B/'final-controls.json').read_text());assert controls['exit_code']==0 and controls['passed']==controls['actual_test_count']==130 and controls['failed']==controls['errors']==controls['skipped']==0;assert controls['source_before']==controls['source_after']
# frozen final modules match the actually executed suite
for name,h in controls['source_after'].items():assert sha(B/name)==h,name
r4=S/'newtag-repair-r4/guard-execution-packet.json';assert sha(r4)=='19ba26595911a3f18de7175bb7b392d0f461fd3faa23c336f58cc91914c5988d';packet=json.loads(r4.read_text())
for t in packet['tags']:
 src=Path(t['export'])/'rust/src';m={str(p.relative_to(src)):sha(p) for p in src.rglob('*') if p.is_file()};assert m==json.loads(Path(t['source_map']['path']).read_text())
current=json.loads((S/'stale-full-final-source-hashes.json').read_text())['all_rust_source'];assert len(current)==213
for path,h in current.items():assert sha(S.parents[1]/path)==h,path
assert sha(S/'summarize_extensions.py')=='fbd9bdf4f5b02d08857033a0309bec3775c4f9f605bdc65492a389bd748aa0aa'
for p in B.glob('*.py'):compile(p.read_bytes(),str(p),'exec')
refs=[ref(p) for p in B.iterdir() if p.is_file() and p.name!='validator-packet.json']
x={'status':'historical validator source/controls FROZEN; STOP for independent focused AFTER; performance NOT_RUN','frozen_at':datetime.datetime.now(datetime.timezone.utc).isoformat(),'guard_checkpoint':ref(r4),'accepted_R4_export_sources_unchanged':True,'current_213_unchanged':True,'current_collector':ref(S/'summarize_extensions.py'),'reuse_identity':json.loads((B/'fbd9-reuse-identity.json').read_text()),'synthetic_controls':{'actual_exit_code':0,'actual_passed':130,'actual_failed':0,'actual_errors':0,'actual_skipped':0,'evidence_kind':'synthetic-control; no Rust/historical/performance run','result':ref(B/'final-controls.json'),'log':ref(B/'final-controls.log')},'meaningful_first_red':ref(B/'first-red-valid-input-result.json'),'first_red_caveat':'Initial synthetic selection incomplete; exact initial logs retained, supplemented valid-selection admission RED. This does not claim live historical FAIL.','CLI_synthetic_mixed':{'actual_exit_code':json.loads((B/'synthetic-cli.json').read_text())['exit_code'],'accepted_rows':28,'excluded_partial_rows':14,'preserved_original_input_exit':101,'real_historical_evidence':False},'current_fbd9_actual_failed_log_rejection':ref(B/'current-fbd9-real-failed-rejection.json'),'classification':'PASS/FAIL/NOT_RUN only; never NON_ELIGIBLE/unsupported inferred from panic text','original_current69_controls':'immutable prior evidence; not replayed/claimed fresh here','actual_historical_performance':'NOT_RUN; no samples collected','protocol':ref(B/'protocol.txt'),'references':refs}
p=B/'validator-packet.json';assert not p.exists();p.write_text(json.dumps(x,indent=2)+'\n');print(json.dumps({'packet':str(p),'sha256':sha(p),'refs':len(refs),'controls':130}))
