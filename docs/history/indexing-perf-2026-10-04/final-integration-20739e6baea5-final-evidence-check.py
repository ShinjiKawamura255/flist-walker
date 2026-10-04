import gzip,hashlib,json,re,subprocess
from pathlib import Path
from urllib.parse import unquote
C=Path.cwd();D=C/'docs/history/indexing-perf-2026-10-04';S=C/'rust/target/indexing-perf-study'
sha=lambda b:hashlib.sha256(b).hexdigest()
manifest_records=[];count=0;legacy_bytes=[]
for p in sorted(D.glob('*.json')):
 try:x=json.loads(p.read_bytes())
 except (ValueError,UnicodeError):continue
 if not isinstance(x,dict) or not isinstance(x.get('assets'),list):continue
 for a in x['assets']:
  value=a.get('path',a.get('file'));assert value,value
  ap=C/value if str(value).startswith('docs/') else D/value
  b=ap.read_bytes();assert sha(b)==a['sha256'],ap
  plain=gzip.decompress(b) if ap.suffix=='.gz' and ('uncompressed_sha256'in a or 'plain_sha256'in a) else b
  if 'uncompressed_sha256'in a:assert sha(plain)==a['uncompressed_sha256'],ap
  if 'plain_sha256'in a:assert sha(plain)==a['plain_sha256'],ap
  if 'bytes'in a:
   assert a['bytes'] in (len(b),len(plain)),(ap,a['bytes'],len(b),len(plain))
   if a['bytes']!=len(b):legacy_bytes.append(str(ap.relative_to(C)))
  if 'uncompressed_bytes'in a:assert len(plain)==a['uncompressed_bytes'],ap
  count+=1
 manifest_records.append({'path':str(p.relative_to(C)),'sha256':sha(p.read_bytes()),'assets':len(x['assets'])})
x=json.loads((S/'stale-full-final-source-hashes.json').read_bytes())
actual={str(p.relative_to(C/'rust')):sha(p.read_bytes()) for p in sorted((C/'rust/src').rglob('*.rs'))}
assert actual==x['all_rust_source'] and len(actual)==213
assert sha(json.dumps(actual,sort_keys=True).encode())==x['all_rust_source_manifest_sha256']
paths=subprocess.check_output(['git','ls-files','--modified','--others','--exclude-standard','-z']).decode().split('\0')
md=[C/p for p in paths if p.endswith('.md') and not Path(p).name.startswith('EXECUTION-')]
links=0
for p in md:
 for v in re.findall(r'\[[^\]]*\]\(([^\s)]+)(?:\s+[^)]*)?\)',p.read_text()):
  if ':'in v or v.startswith('#'):continue
  q=unquote(v.split('#')[0]);assert (p.parent/q).exists(),(p,q);links+=1
# Frozen patch files are data, not added code; Git whitespace markers and retained CRLF must remain byte-exact.
cmd=['git','diff','--check','--','.',':(exclude)docs/history/indexing-perf-2026-10-04/frozen.patch']
p=subprocess.run(cmd,capture_output=True,text=True);assert p.returncode==0,p.stdout+p.stderr
result={'status':'PASS','manifests':manifest_records,'asset_references_checked':count,'legacy_bytes_fields_record_plain_length':legacy_bytes,'rust_source_files':len(actual),'rust_source_map':x['all_rust_source_manifest_sha256'],'markdown_documents_checked':len(md),'local_links_checked':links,'missing_links':0,'diff_check_exit':p.returncode,'patch_data_policy':'Stored SHA256 and uncompressed SHA256 exact; retained patch syntax/CR are never normalized or counted as production whitespace errors'}
(S/'final-evidence-check.json').write_text(json.dumps(result,indent=2)+'\n')
print('PASS',len(manifest_records),'manifests',count,'asset references;',len(actual),'Rust sources;',len(md),'Markdown files',links,'local links; diff check',p.returncode)
