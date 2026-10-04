from pathlib import Path
import sys,subprocess,json
B=Path(__file__).resolve().parent;tag=sys.argv[1];source=B.parent/'newtag-repair-r3'/tag/'rust';target=B/('target-r3-'+tag)
for name,selector in [('fixture','app::tests::indexing_perf::harness::extensions::fixture::'),('oracle','app::tests::indexing_perf::harness::extensions::oracle::')]:
 stage='r3-'+tag+'-'+name+'-discovery';r=subprocess.run([sys.executable,str(B/'run-stage.py'),stage,str(source),str(target),selector,'list-pattern']);d=json.loads((B/(stage+'.json')).read_text());assert r.returncode==0 and d['exit_code']==0 and len(d['discovered_tests'])>0,(tag,name,d)
 stage='r3-'+tag+'-'+name+'-guards';r=subprocess.run([sys.executable,str(B/'run-stage.py'),stage,str(source),str(target),selector,'pattern']);d=json.loads((B/(stage+'.json')).read_text());assert r.returncode==0 and d['exit_code']==0,(tag,name,d)
